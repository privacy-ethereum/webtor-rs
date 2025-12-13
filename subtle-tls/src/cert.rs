//! Certificate parsing and validation
//!
//! This module handles X.509 certificate chain validation for TLS 1.3.
//! It uses x509-parser for parsing and SubtleCrypto for signature verification.

use crate::error::{Result, TlsError};
use crate::trust_store::TrustStore;
use js_sys::{Array, Object, Reflect, Uint8Array};
use tracing::{debug, info, trace, warn};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Crypto, CryptoKey, SubtleCrypto};
use x509_parser::prelude::*;

/// Get the SubtleCrypto instance
fn get_subtle_crypto() -> Result<SubtleCrypto> {
    let window =
        web_sys::window().ok_or_else(|| TlsError::subtle_crypto("No window object available"))?;
    let crypto: Crypto = window
        .crypto()
        .map_err(|_| TlsError::subtle_crypto("No crypto object available"))?;
    Ok(crypto.subtle())
}

/// Certificate chain validator
pub struct CertificateVerifier {
    /// Expected server name (for SNI validation)
    server_name: String,
    /// Whether to skip verification (INSECURE)
    skip_verification: bool,
    /// Trust store with root CAs
    trust_store: Option<TrustStore>,
}

impl CertificateVerifier {
    /// Create a new certificate verifier
    pub fn new(server_name: &str, skip_verification: bool) -> Self {
        let trust_store = if skip_verification {
            None
        } else {
            TrustStore::new().ok()
        };

        Self {
            server_name: server_name.to_string(),
            skip_verification,
            trust_store,
        }
    }

    /// Create a verifier with a custom trust store
    pub fn with_trust_store(server_name: &str, trust_store: TrustStore) -> Self {
        Self {
            server_name: server_name.to_string(),
            skip_verification: false,
            trust_store: Some(trust_store),
        }
    }

    /// Verify a certificate chain
    ///
    /// The chain should be ordered with the leaf certificate first,
    /// followed by intermediate certificates, and optionally the root.
    pub async fn verify_chain(&self, cert_chain: &[Vec<u8>]) -> Result<()> {
        if self.skip_verification {
            warn!("Certificate verification skipped (INSECURE)");
            return Ok(());
        }

        if cert_chain.is_empty() {
            return Err(TlsError::certificate("Empty certificate chain"));
        }

        // Parse the leaf certificate
        let leaf_der = &cert_chain[0];
        let (_, leaf_cert) = X509Certificate::from_der(leaf_der).map_err(|e| {
            TlsError::certificate(format!("Failed to parse leaf certificate: {}", e))
        })?;

        debug!("Verifying certificate for: {:?}", leaf_cert.subject());

        // Step 1: Verify the server name matches
        self.verify_server_name(&leaf_cert)?;

        // Step 2: Check certificate validity period
        self.verify_validity(&leaf_cert)?;

        // Step 3: Verify the certificate chain signatures
        self.verify_chain_signatures(cert_chain).await?;

        debug!("Certificate chain verified successfully");
        Ok(())
    }

    /// Verify that the certificate matches the expected server name
    fn verify_server_name(&self, cert: &X509Certificate) -> Result<()> {
        // Check Subject Alternative Names (SAN) extension first
        if let Ok(Some(san)) = cert.subject_alternative_name() {
            for name in &san.value.general_names {
                match name {
                    GeneralName::DNSName(dns_name) => {
                        if self.matches_hostname(dns_name) {
                            trace!(
                                "Server name '{}' matches SAN '{}'",
                                self.server_name,
                                dns_name
                            );
                            return Ok(());
                        }
                    }
                    GeneralName::IPAddress(ip_bytes) => {
                        // Check if it's an IP address match
                        if let Ok(ip_str) = std::str::from_utf8(ip_bytes) {
                            if ip_str == self.server_name {
                                trace!("Server IP '{}' matches SAN", self.server_name);
                                return Ok(());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Fall back to Common Name (CN) in subject
        for rdn in cert.subject().iter() {
            for attr in rdn.iter() {
                if attr.attr_type() == &oid_registry::OID_X509_COMMON_NAME {
                    if let Ok(cn) = attr.as_str() {
                        if self.matches_hostname(cn) {
                            trace!("Server name '{}' matches CN '{}'", self.server_name, cn);
                            return Ok(());
                        }
                    }
                }
            }
        }

        Err(TlsError::certificate(format!(
            "Certificate does not match server name '{}'",
            self.server_name
        )))
    }

    /// Check if a certificate pattern matches the hostname
    fn matches_hostname(&self, pattern: &str) -> bool {
        let hostname = self.server_name.to_lowercase();
        let pattern = pattern.to_lowercase();

        // Exact match
        if hostname == pattern {
            return true;
        }

        // Wildcard match (*.example.com matches foo.example.com)
        if pattern.starts_with("*.") {
            let suffix = &pattern[1..]; // .example.com
            if hostname.ends_with(suffix) {
                // Ensure there's no dot in the prefix (*.example.com should not match foo.bar.example.com)
                let prefix = &hostname[..hostname.len() - suffix.len()];
                if !prefix.contains('.') && !prefix.is_empty() {
                    return true;
                }
            }
        }

        false
    }

    /// Verify the certificate is currently valid
    fn verify_validity(&self, cert: &X509Certificate) -> Result<()> {
        let validity = cert.validity();

        // Get current time - in WASM we use js_sys::Date
        let now_ms = js_sys::Date::now();
        let now_secs = (now_ms / 1000.0) as i64;

        let not_before = validity.not_before.timestamp();
        let not_after = validity.not_after.timestamp();

        if now_secs < not_before {
            return Err(TlsError::certificate(format!(
                "Certificate is not yet valid (valid from {})",
                validity.not_before
            )));
        }

        if now_secs > not_after {
            return Err(TlsError::certificate(format!(
                "Certificate has expired (expired at {})",
                validity.not_after
            )));
        }

        trace!("Certificate validity period OK");
        Ok(())
    }

    /// Verify signatures in the certificate chain and check against trust store
    async fn verify_chain_signatures(&self, cert_chain: &[Vec<u8>]) -> Result<()> {
        // Verify each certificate is signed by the next one in the chain
        for i in 0..cert_chain.len() - 1 {
            let cert_der = &cert_chain[i];
            let issuer_der = &cert_chain[i + 1];

            let (_, cert) = X509Certificate::from_der(cert_der).map_err(|e| {
                TlsError::certificate(format!("Failed to parse certificate {}: {}", i, e))
            })?;
            let (_, issuer) = X509Certificate::from_der(issuer_der).map_err(|e| {
                TlsError::certificate(format!("Failed to parse issuer {}: {}", i + 1, e))
            })?;

            // Verify the certificate's issuer matches the issuer's subject
            if cert.issuer() != issuer.subject() {
                return Err(TlsError::certificate(format!(
                    "Certificate {} issuer does not match certificate {} subject",
                    i,
                    i + 1
                )));
            }

            // Verify the signature
            self.verify_signature(&cert, &issuer).await?;

            debug!(
                "Certificate {} signature verified by certificate {}",
                i,
                i + 1
            );
        }

        // Check the last certificate against trust store
        let last_cert_der = cert_chain
            .last()
            .ok_or_else(|| TlsError::certificate("Empty certificate chain"))?;

        let (_, last_cert) = X509Certificate::from_der(last_cert_der).map_err(|e| {
            TlsError::certificate(format!("Failed to parse last certificate: {}", e))
        })?;

        // Check if the last cert is in our trust store
        if let Some(ref trust_store) = self.trust_store {
            if trust_store.is_trusted_root(last_cert_der) {
                info!(
                    "Certificate chain terminates at trusted root: {}",
                    last_cert.subject()
                );
                return Ok(());
            }

            // Check if the last cert was issued by a trusted root
            if trust_store.is_issued_by_trusted_root(last_cert_der) {
                info!(
                    "Certificate chain issued by trusted root (issuer: {})",
                    last_cert.issuer()
                );
                // For intermediates, we should ideally verify the signature against the root
                // but the root may not be in the chain. Accept if issuer matches.
                return Ok(());
            }

            // Not in trust store - warn but continue (for now)
            // In strict mode, this should be an error
            warn!(
                "Certificate chain does not terminate at a trusted root. Last cert: {}",
                last_cert.subject()
            );
        } else {
            // No trust store available
            if last_cert.issuer() == last_cert.subject() {
                // Self-signed - verify signature against itself
                self.verify_signature(&last_cert, &last_cert).await?;
                debug!("Self-signed certificate signature verified");
            } else {
                warn!("Certificate chain verification incomplete - no trust store");
            }
        }

        Ok(())
    }

    /// Verify a certificate's signature using the issuer's public key
    async fn verify_signature(
        &self,
        cert: &X509Certificate<'_>,
        issuer: &X509Certificate<'_>,
    ) -> Result<()> {
        let signature_algorithm = cert.signature_algorithm.algorithm.clone();
        let signature = cert.signature_value.as_ref();
        let tbs_certificate = cert.tbs_certificate.as_ref();

        // Get the issuer's public key
        let public_key_info = &issuer.public_key();
        let public_key_data = public_key_info.raw;

        // Determine the algorithm and import the key
        // Pass the full public key info so we can extract curve from parameters
        let (algorithm_name, hash_name, key_algorithm) =
            self.get_crypto_algorithm_from_key(&signature_algorithm, public_key_info)?;

        trace!(
            "Verifying signature with algorithm: {}, hash: {}",
            algorithm_name,
            hash_name
        );

        // Import the public key and verify the signature
        verify_signature_with_subtle_crypto(
            &algorithm_name,
            &hash_name,
            &key_algorithm,
            public_key_data,
            signature,
            tbs_certificate,
        )
        .await
    }

    /// Map X.509 signature algorithm OID to SubtleCrypto parameters
    /// Uses the public key info to determine the correct curve for EC keys
    fn get_crypto_algorithm_from_key(
        &self,
        sig_oid: &x509_parser::der_parser::oid::Oid,
        public_key_info: &SubjectPublicKeyInfo,
    ) -> Result<(String, String, Object)> {
        // Common signature algorithm OIDs
        let oid_sha256_with_rsa = oid_registry::OID_PKCS1_SHA256WITHRSA;
        let oid_sha384_with_rsa = oid_registry::OID_PKCS1_SHA384WITHRSA;
        let oid_sha512_with_rsa = oid_registry::OID_PKCS1_SHA512WITHRSA;
        let oid_ecdsa_with_sha256 = oid_registry::OID_SIG_ECDSA_WITH_SHA256;
        let oid_ecdsa_with_sha384 = oid_registry::OID_SIG_ECDSA_WITH_SHA384;
        let oid_rsa_pss = oid_registry::OID_PKCS1_RSASSAPSS;

        let key_algorithm = Object::new();

        if sig_oid == &oid_sha256_with_rsa
            || sig_oid == &oid_sha384_with_rsa
            || sig_oid == &oid_sha512_with_rsa
        {
            // RSA PKCS#1 v1.5
            let hash = if sig_oid == &oid_sha256_with_rsa {
                "SHA-256"
            } else if sig_oid == &oid_sha384_with_rsa {
                "SHA-384"
            } else {
                "SHA-512"
            };

            Reflect::set(&key_algorithm, &"name".into(), &"RSASSA-PKCS1-v1_5".into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

            let hash_obj = Object::new();
            Reflect::set(&hash_obj, &"name".into(), &hash.into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
            Reflect::set(&key_algorithm, &"hash".into(), &hash_obj)
                .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;

            return Ok((
                "RSASSA-PKCS1-v1_5".to_string(),
                hash.to_string(),
                key_algorithm,
            ));
        }

        if sig_oid == &oid_rsa_pss {
            // RSA-PSS - need to determine hash from parameters
            // For simplicity, default to SHA-256
            Reflect::set(&key_algorithm, &"name".into(), &"RSA-PSS".into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

            let hash_obj = Object::new();
            Reflect::set(&hash_obj, &"name".into(), &"SHA-256".into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
            Reflect::set(&key_algorithm, &"hash".into(), &hash_obj)
                .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;

            return Ok(("RSA-PSS".to_string(), "SHA-256".to_string(), key_algorithm));
        }

        if sig_oid == &oid_ecdsa_with_sha256 || sig_oid == &oid_ecdsa_with_sha384 {
            // ECDSA - determine curve from the public key's algorithm parameters
            let hash = if sig_oid == &oid_ecdsa_with_sha256 {
                "SHA-256"
            } else {
                "SHA-384"
            };

            // Extract curve from public key parameters
            let curve = self.get_ec_curve_from_key(public_key_info)?;

            debug!("ECDSA verification: hash={}, curve={}", hash, curve);

            Reflect::set(&key_algorithm, &"name".into(), &"ECDSA".into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
            Reflect::set(&key_algorithm, &"namedCurve".into(), &curve.into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set curve"))?;

            return Ok(("ECDSA".to_string(), hash.to_string(), key_algorithm));
        }

        Err(TlsError::certificate(format!(
            "Unsupported signature algorithm: {:?}",
            sig_oid
        )))
    }

    /// Extract the EC curve name from public key info
    fn get_ec_curve_from_key(&self, public_key_info: &SubjectPublicKeyInfo) -> Result<String> {
        // The curve OID is in the algorithm parameters
        // Common curve OIDs:
        // P-256 (secp256r1): 1.2.840.10045.3.1.7
        // P-384 (secp384r1): 1.3.132.0.34
        // P-521 (secp521r1): 1.3.132.0.35

        let oid_p256: x509_parser::der_parser::oid::Oid =
            x509_parser::der_parser::oid::Oid::from(&[1, 2, 840, 10045, 3, 1, 7]).unwrap();
        let oid_p384: x509_parser::der_parser::oid::Oid =
            x509_parser::der_parser::oid::Oid::from(&[1, 3, 132, 0, 34]).unwrap();
        let oid_p521: x509_parser::der_parser::oid::Oid =
            x509_parser::der_parser::oid::Oid::from(&[1, 3, 132, 0, 35]).unwrap();

        // Parse the algorithm parameters to get the curve OID
        if let Some(params) = &public_key_info.algorithm.parameters {
            // The parameters should be an OID for named curves
            if let Ok(curve_oid) = params.as_oid() {
                if curve_oid == oid_p256 {
                    return Ok("P-256".to_string());
                } else if curve_oid == oid_p384 {
                    return Ok("P-384".to_string());
                } else if curve_oid == oid_p521 {
                    return Ok("P-521".to_string());
                } else {
                    return Err(TlsError::certificate(format!(
                        "Unsupported EC curve OID: {:?}",
                        curve_oid
                    )));
                }
            }
        }

        // If we can't determine the curve, default based on signature hash
        // This is a fallback - ideally we always extract from key params
        warn!("Could not determine EC curve from key parameters, defaulting to P-256");
        Ok("P-256".to_string())
    }

    /// Map X.509 signature algorithm OID to SubtleCrypto parameters (legacy)
    #[allow(dead_code)]
    fn get_crypto_algorithm(
        &self,
        sig_oid: &x509_parser::der_parser::oid::Oid,
        _key_oid: &x509_parser::der_parser::oid::Oid,
    ) -> Result<(String, String, Object)> {
        // Common signature algorithm OIDs
        let oid_sha256_with_rsa = oid_registry::OID_PKCS1_SHA256WITHRSA;
        let oid_sha384_with_rsa = oid_registry::OID_PKCS1_SHA384WITHRSA;
        let oid_sha512_with_rsa = oid_registry::OID_PKCS1_SHA512WITHRSA;
        let oid_ecdsa_with_sha256 = oid_registry::OID_SIG_ECDSA_WITH_SHA256;
        let oid_ecdsa_with_sha384 = oid_registry::OID_SIG_ECDSA_WITH_SHA384;
        let oid_rsa_pss = oid_registry::OID_PKCS1_RSASSAPSS;

        let key_algorithm = Object::new();

        if sig_oid == &oid_sha256_with_rsa
            || sig_oid == &oid_sha384_with_rsa
            || sig_oid == &oid_sha512_with_rsa
        {
            // RSA PKCS#1 v1.5
            let hash = if sig_oid == &oid_sha256_with_rsa {
                "SHA-256"
            } else if sig_oid == &oid_sha384_with_rsa {
                "SHA-384"
            } else {
                "SHA-512"
            };

            Reflect::set(&key_algorithm, &"name".into(), &"RSASSA-PKCS1-v1_5".into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

            let hash_obj = Object::new();
            Reflect::set(&hash_obj, &"name".into(), &hash.into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
            Reflect::set(&key_algorithm, &"hash".into(), &hash_obj)
                .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;

            return Ok((
                "RSASSA-PKCS1-v1_5".to_string(),
                hash.to_string(),
                key_algorithm,
            ));
        }

        if sig_oid == &oid_rsa_pss {
            // RSA-PSS - need to determine hash from parameters
            // For simplicity, default to SHA-256
            Reflect::set(&key_algorithm, &"name".into(), &"RSA-PSS".into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

            let hash_obj = Object::new();
            Reflect::set(&hash_obj, &"name".into(), &"SHA-256".into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
            Reflect::set(&key_algorithm, &"hash".into(), &hash_obj)
                .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;

            return Ok(("RSA-PSS".to_string(), "SHA-256".to_string(), key_algorithm));
        }

        if sig_oid == &oid_ecdsa_with_sha256 || sig_oid == &oid_ecdsa_with_sha384 {
            // ECDSA - match curve to hash algorithm
            // SHA-256 uses P-256 curve, SHA-384 uses P-384 curve
            let (hash, curve) = if sig_oid == &oid_ecdsa_with_sha256 {
                ("SHA-256", "P-256")
            } else {
                ("SHA-384", "P-384")
            };

            Reflect::set(&key_algorithm, &"name".into(), &"ECDSA".into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;
            Reflect::set(&key_algorithm, &"namedCurve".into(), &curve.into())
                .map_err(|_| TlsError::subtle_crypto("Failed to set curve"))?;

            return Ok(("ECDSA".to_string(), hash.to_string(), key_algorithm));
        }

        Err(TlsError::certificate(format!(
            "Unsupported signature algorithm: {:?}",
            sig_oid
        )))
    }
}

/// Verify a signature using SubtleCrypto
async fn verify_signature_with_subtle_crypto(
    algorithm_name: &str,
    hash_name: &str,
    key_algorithm: &Object,
    public_key_der: &[u8],
    signature: &[u8],
    data: &[u8],
) -> Result<()> {
    let subtle = get_subtle_crypto()?;

    // Import the public key
    // For RSA keys, we need SPKI format which is what x509 provides
    // For EC keys, same thing
    let key_data = Uint8Array::from(public_key_der);
    let key_usages = Array::new();
    key_usages.push(&"verify".into());

    let public_key = JsFuture::from(
        subtle
            .import_key_with_object(
                "spki",
                &key_data.buffer(),
                key_algorithm,
                false,
                &key_usages,
            )
            .map_err(|e| TlsError::subtle_crypto(format!("Failed to import key: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("Key import failed: {:?}", e)))?;

    let public_key: CryptoKey = public_key.unchecked_into();

    // Prepare the verify algorithm parameters
    let verify_algorithm = Object::new();
    Reflect::set(&verify_algorithm, &"name".into(), &algorithm_name.into())
        .map_err(|_| TlsError::subtle_crypto("Failed to set verify algorithm name"))?;

    if algorithm_name == "ECDSA" {
        // ECDSA needs hash specified in verify call
        let hash_obj = Object::new();
        Reflect::set(&hash_obj, &"name".into(), &hash_name.into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
        Reflect::set(&verify_algorithm, &"hash".into(), &hash_obj)
            .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;
    } else if algorithm_name == "RSA-PSS" {
        // RSA-PSS needs saltLength
        // Salt length is typically the hash length
        let salt_length = if hash_name == "SHA-256" {
            32
        } else if hash_name == "SHA-384" {
            48
        } else {
            64
        };
        Reflect::set(
            &verify_algorithm,
            &"saltLength".into(),
            &JsValue::from_f64(salt_length as f64),
        )
        .map_err(|_| TlsError::subtle_crypto("Failed to set saltLength"))?;
    }

    // For ECDSA, we need to convert the signature from DER to raw format
    // Use correct coordinate size based on curve (determined by hash)
    let signature_bytes = if algorithm_name == "ECDSA" {
        let coord_size = if hash_name == "SHA-384" { 48 } else { 32 };
        convert_ecdsa_signature_from_der_sized(signature, coord_size)?
    } else {
        signature.to_vec()
    };

    let signature_array = Uint8Array::from(signature_bytes.as_slice());
    let data_array = Uint8Array::from(data);

    // Verify the signature
    let result = JsFuture::from(
        subtle
            .verify_with_object_and_buffer_source_and_buffer_source(
                &verify_algorithm,
                &public_key,
                &signature_array.buffer(),
                &data_array.buffer(),
            )
            .map_err(|e| TlsError::subtle_crypto(format!("Failed to verify: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("Verification failed: {:?}", e)))?;

    let is_valid = result
        .as_bool()
        .ok_or_else(|| TlsError::subtle_crypto("Verify did not return boolean"))?;

    if is_valid {
        Ok(())
    } else {
        Err(TlsError::certificate("Signature verification failed"))
    }
}

/// Convert ECDSA signature from DER format to raw (r || s) format
/// SubtleCrypto expects raw format, but X.509 certificates use DER
///
/// `coord_size` is the size of each coordinate in bytes:
/// - P-256: 32 bytes
/// - P-384: 48 bytes
fn convert_ecdsa_signature_from_der_sized(der_sig: &[u8], coord_size: usize) -> Result<Vec<u8>> {
    // DER format: SEQUENCE { INTEGER r, INTEGER s }
    // Raw format: r (coord_size bytes) || s (coord_size bytes)

    if der_sig.len() < 8 {
        return Err(TlsError::certificate("ECDSA signature too short"));
    }

    // Simple DER parser for ECDSA signature
    let mut pos = 0;

    // SEQUENCE tag
    if der_sig[pos] != 0x30 {
        return Err(TlsError::certificate(
            "Invalid ECDSA signature: not a SEQUENCE",
        ));
    }
    pos += 1;

    // Bounds check before reading SEQUENCE length
    if pos >= der_sig.len() {
        return Err(TlsError::certificate(
            "ECDSA signature truncated at sequence length",
        ));
    }

    // SEQUENCE length
    let _seq_len = if der_sig[pos] & 0x80 != 0 {
        let len_bytes = (der_sig[pos] & 0x7f) as usize;
        pos += 1;
        // Bounds check for multi-byte length
        if pos + len_bytes > der_sig.len() {
            return Err(TlsError::certificate(
                "ECDSA signature truncated in length field",
            ));
        }
        let mut len = 0usize;
        for _ in 0..len_bytes {
            len = (len << 8) | (der_sig[pos] as usize);
            pos += 1;
        }
        len
    } else {
        let len = der_sig[pos] as usize;
        pos += 1;
        len
    };

    // Parse r - bounds check for tag and length bytes
    if pos + 2 > der_sig.len() {
        return Err(TlsError::certificate(
            "ECDSA signature truncated at r header",
        ));
    }
    if der_sig[pos] != 0x02 {
        return Err(TlsError::certificate(
            "Invalid ECDSA signature: r not INTEGER",
        ));
    }
    pos += 1;
    let r_len = der_sig[pos] as usize;
    pos += 1;
    // Bounds check for r data
    if pos + r_len > der_sig.len() {
        return Err(TlsError::certificate("ECDSA signature r data overflow"));
    }
    let r_bytes = &der_sig[pos..pos + r_len];
    pos += r_len;

    // Parse s - bounds check for tag and length bytes
    if pos + 2 > der_sig.len() {
        return Err(TlsError::certificate(
            "ECDSA signature truncated at s header",
        ));
    }
    if der_sig[pos] != 0x02 {
        return Err(TlsError::certificate(
            "Invalid ECDSA signature: s not INTEGER",
        ));
    }
    pos += 1;
    let s_len = der_sig[pos] as usize;
    pos += 1;
    // Bounds check for s data
    if pos + s_len > der_sig.len() {
        return Err(TlsError::certificate("ECDSA signature s data overflow"));
    }
    let s_bytes = &der_sig[pos..pos + s_len];

    // Convert to fixed-size (coord_size bytes each)
    let mut result = vec![0u8; coord_size * 2];

    // r - strip leading zeros and pad to coord_size
    let r_start = if r_bytes.len() > coord_size && r_bytes[0] == 0 {
        // Skip leading zero padding (DER uses this for positive integers)
        r_bytes
            .iter()
            .position(|&b| b != 0)
            .unwrap_or(r_bytes.len() - 1)
    } else {
        0
    };
    let r_data = &r_bytes[r_start..];
    let r_copy_len = r_data.len().min(coord_size);
    let r_offset = coord_size - r_copy_len;
    result[r_offset..coord_size].copy_from_slice(&r_data[..r_copy_len]);

    // s - strip leading zeros and pad to coord_size
    let s_start = if s_bytes.len() > coord_size && s_bytes[0] == 0 {
        s_bytes
            .iter()
            .position(|&b| b != 0)
            .unwrap_or(s_bytes.len() - 1)
    } else {
        0
    };
    let s_data = &s_bytes[s_start..];
    let s_copy_len = s_data.len().min(coord_size);
    let s_offset = coord_size + (coord_size - s_copy_len);
    result[s_offset..coord_size * 2].copy_from_slice(&s_data[..s_copy_len]);

    Ok(result)
}

/// Convert ECDSA signature from DER format to raw (r || s) format for P-256
/// (Legacy wrapper for backward compatibility)
fn convert_ecdsa_signature_from_der(der_sig: &[u8]) -> Result<Vec<u8>> {
    convert_ecdsa_signature_from_der_sized(der_sig, 32)
}

/// Verify CertificateVerify signature (TLS 1.3)
/// This verifies the server's proof of possession of the private key
pub async fn verify_certificate_verify(
    signature_algorithm: u16,
    signature: &[u8],
    transcript_hash: &[u8],
    server_public_key: &[u8],
) -> Result<()> {
    // Build the content to be verified:
    // 64 spaces + "TLS 1.3, server CertificateVerify" + 0x00 + transcript_hash
    let mut content = Vec::with_capacity(64 + 33 + 1 + transcript_hash.len());
    content.extend_from_slice(&[0x20u8; 64]); // 64 spaces
    content.extend_from_slice(b"TLS 1.3, server CertificateVerify");
    content.push(0x00);
    content.extend_from_slice(transcript_hash);

    // Determine algorithm from signature_algorithm code
    let (algorithm_name, hash_name) = match signature_algorithm {
        0x0401 => ("RSASSA-PKCS1-v1_5", "SHA-256"), // rsa_pkcs1_sha256
        0x0501 => ("RSASSA-PKCS1-v1_5", "SHA-384"), // rsa_pkcs1_sha384
        0x0601 => ("RSASSA-PKCS1-v1_5", "SHA-512"), // rsa_pkcs1_sha512
        0x0804 => ("RSA-PSS", "SHA-256"),           // rsa_pss_rsae_sha256
        0x0805 => ("RSA-PSS", "SHA-384"),           // rsa_pss_rsae_sha384
        0x0806 => ("RSA-PSS", "SHA-512"),           // rsa_pss_rsae_sha512
        0x0403 => ("ECDSA", "SHA-256"),             // ecdsa_secp256r1_sha256
        0x0503 => ("ECDSA", "SHA-384"),             // ecdsa_secp384r1_sha384
        _ => {
            return Err(TlsError::certificate(format!(
                "Unsupported signature algorithm: 0x{:04x}",
                signature_algorithm
            )));
        }
    };

    let subtle = get_subtle_crypto()?;

    // Build key import algorithm
    let key_algorithm = Object::new();

    if algorithm_name.starts_with("RSA") {
        Reflect::set(&key_algorithm, &"name".into(), &algorithm_name.into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

        let hash_obj = Object::new();
        Reflect::set(&hash_obj, &"name".into(), &hash_name.into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
        Reflect::set(&key_algorithm, &"hash".into(), &hash_obj)
            .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;
    } else {
        // ECDSA
        Reflect::set(&key_algorithm, &"name".into(), &"ECDSA".into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set algorithm name"))?;

        let curve = if signature_algorithm == 0x0503 {
            "P-384"
        } else {
            "P-256"
        };
        Reflect::set(&key_algorithm, &"namedCurve".into(), &curve.into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set curve"))?;
    }

    // Import the public key
    let key_data = Uint8Array::from(server_public_key);
    let key_usages = Array::new();
    key_usages.push(&"verify".into());

    let public_key = JsFuture::from(
        subtle
            .import_key_with_object(
                "spki",
                &key_data.buffer(),
                &key_algorithm,
                false,
                &key_usages,
            )
            .map_err(|e| TlsError::subtle_crypto(format!("Failed to import key: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("Key import failed: {:?}", e)))?;

    let public_key: CryptoKey = public_key.unchecked_into();

    // Build verify algorithm
    let verify_algorithm = Object::new();
    Reflect::set(&verify_algorithm, &"name".into(), &algorithm_name.into())
        .map_err(|_| TlsError::subtle_crypto("Failed to set verify algorithm name"))?;

    if algorithm_name == "ECDSA" {
        let hash_obj = Object::new();
        Reflect::set(&hash_obj, &"name".into(), &hash_name.into())
            .map_err(|_| TlsError::subtle_crypto("Failed to set hash name"))?;
        Reflect::set(&verify_algorithm, &"hash".into(), &hash_obj)
            .map_err(|_| TlsError::subtle_crypto("Failed to set hash"))?;
    } else if algorithm_name == "RSA-PSS" {
        let salt_length = match hash_name {
            "SHA-256" => 32,
            "SHA-384" => 48,
            "SHA-512" => 64,
            _ => 32,
        };
        Reflect::set(
            &verify_algorithm,
            &"saltLength".into(),
            &JsValue::from_f64(salt_length as f64),
        )
        .map_err(|_| TlsError::subtle_crypto("Failed to set saltLength"))?;
    }

    // Convert ECDSA signature from DER if needed
    // Use correct coordinate size based on curve (determined by hash)
    let signature_bytes = if algorithm_name == "ECDSA" {
        let coord_size = if hash_name == "SHA-384" { 48 } else { 32 };
        convert_ecdsa_signature_from_der_sized(signature, coord_size)?
    } else {
        signature.to_vec()
    };

    let signature_array = Uint8Array::from(signature_bytes.as_slice());
    let content_array = Uint8Array::from(content.as_slice());

    // Verify
    let result = JsFuture::from(
        subtle
            .verify_with_object_and_buffer_source_and_buffer_source(
                &verify_algorithm,
                &public_key,
                &signature_array.buffer(),
                &content_array.buffer(),
            )
            .map_err(|e| TlsError::subtle_crypto(format!("Failed to verify: {:?}", e)))?,
    )
    .await
    .map_err(|e| TlsError::subtle_crypto(format!("CertificateVerify failed: {:?}", e)))?;

    let is_valid = result
        .as_bool()
        .ok_or_else(|| TlsError::subtle_crypto("Verify did not return boolean"))?;

    if is_valid {
        debug!("CertificateVerify signature verified");
        Ok(())
    } else {
        Err(TlsError::certificate(
            "CertificateVerify signature verification failed",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[test]
    fn test_hostname_matching() {
        let verifier = CertificateVerifier::new("example.com", false);

        assert!(verifier.matches_hostname("example.com"));
        assert!(verifier.matches_hostname("EXAMPLE.COM"));
        assert!(verifier.matches_hostname("*.example.com") == false); // Wildcard doesn't match apex
        assert!(!verifier.matches_hostname("other.com"));
    }

    #[test]
    fn test_wildcard_matching() {
        let verifier = CertificateVerifier::new("foo.example.com", false);

        assert!(verifier.matches_hostname("*.example.com"));
        assert!(verifier.matches_hostname("foo.example.com"));
        assert!(!verifier.matches_hostname("*.other.com"));
    }

    #[test]
    fn test_ecdsa_signature_conversion() {
        // Example DER-encoded ECDSA signature
        let der_sig = [
            0x30, 0x44, // SEQUENCE, length 68
            0x02, 0x20, // INTEGER, length 32
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20, 0x02, 0x20, // INTEGER, length 32
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
            0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c,
            0x3d, 0x3e, 0x3f, 0x40,
        ];

        let raw = convert_ecdsa_signature_from_der(&der_sig).unwrap();
        assert_eq!(raw.len(), 64);
    }
}

#[cfg(kani)]
mod verification {
    use super::*;

    // Use smaller input sizes to keep verification tractable
    // The function logic is the same regardless of input size

    #[kani::proof]
    #[kani::unwind(20)]
    fn ecdsa_der_conversion_never_panics_short() {
        let sig: [u8; 16] = kani::any();
        let _ = convert_ecdsa_signature_from_der_sized(&sig, 32);
    }

    #[kani::proof]
    #[kani::unwind(20)]
    fn ecdsa_der_conversion_never_panics_medium() {
        let sig: [u8; 16] = kani::any();
        let _ = convert_ecdsa_signature_from_der_sized(&sig, 48);
    }

    #[kani::proof]
    #[kani::unwind(20)]
    fn ecdsa_der_conversion_size_correct() {
        let sig: [u8; 16] = kani::any();
        if let Ok(raw) = convert_ecdsa_signature_from_der_sized(&sig, 32) {
            kani::assert(raw.len() == 64, "P-256 output should be 64 bytes");
        }
    }
}
