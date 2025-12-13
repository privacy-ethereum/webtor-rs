//! Root CA Trust Store
//!
//! This module provides a two-tier trust store:
//! 1. Embedded minimal CAs (Let's Encrypt) for Tor infrastructure bootstrap
//! 2. Lazy-loaded full Mozilla CA bundle fetched via Tor for complete coverage
//!
//! The embedded CAs allow connecting to Tor infrastructure (Snowflake broker, etc.)
//! Once Tor is working, we fetch the full CA bundle through Tor for privacy.

use crate::error::{Result, TlsError};
use std::cell::RefCell;
use std::rc::Rc;
use tracing::info;
use x509_parser::prelude::*;

/// ISRG Root X1 - Let's Encrypt RSA root (used by torproject.org)
/// Valid until 2035-06-04
const ISRG_ROOT_X1_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIFazCCA1OgAwIBAgIRAIIQz7DSQONZRGPgu2OCiwAwDQYJKoZIhvcNAQELBQAw
TzELMAkGA1UEBhMCVVMxKTAnBgNVBAoTIEludGVybmV0IFNlY3VyaXR5IFJlc2Vh
cmNoIEdyb3VwMRUwEwYDVQQDEwxJU1JHIFJvb3QgWDEwHhcNMTUwNjA0MTEwNDM4
WhcNMzUwNjA0MTEwNDM4WjBPMQswCQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJu
ZXQgU2VjdXJpdHkgUmVzZWFyY2ggR3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBY
MTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAK3oJHP0FDfzm54rVygc
h77ct984kIxuPOZXoHj3dcKi/vVqbvYATyjb3miGbESTtrFj/RQSa78f0uoxmyF+
0TM8ukj13Xnfs7j/EvEhmkvBioZxaUpmZmyPfjxwv60pIgbz5MDmgK7iS4+3mX6U
A5/TR5d8mUgjU+g4rk8Kb4Mu0UlXjIB0ttov0DiNewNwIRt18jA8+o+u3dpjq+sW
T8KOEUt+zwvo/7V3LvSye0rgTBIlDHCNAymg4VMk7BPZ7hm/ELNKjD+Jo2FR3qyH
B5T0Y3HsLuJvW5iB4YlcNHlsdu87kGJ55tukmi8mxdAQ4Q7e2RCOFvu396j3x+UC
B5iPNgiV5+I3lg02dZ77DnKxHZu8A/lJBdiB3QW0KtZB6awBdpUKD9jf1b0SHzUv
KBds0pjBqAlkd25HN7rOrFleaJ1/ctaJxQZBKT5ZPt0m9STJEadao0xAH0ahmbWn
OlFuhjuefXKnEgV4We0+UXgVCwOPjdAvBbI+e0ocS3MFEvzG6uBQE3xDk3SzynTn
jh8BCNAw1FtxNrQHusEwMFxIt4I7mKZ9YIqioymCzLq9gwQbooMDQaHWBfEbwrbw
qHyGO0aoSCqI3Haadr8faqU9GY/rOPNk3sgrDQoo//fb4hVC1CLQJ13hef4Y53CI
rU7m2Ys6xt0nUW7/vGT1M0NPAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNV
HRMBAf8EBTADAQH/MB0GA1UdDgQWBBR5tFnme7bl5AFzgAiIyBpY9umbbjANBgkq
hkiG9w0BAQsFAAOCAgEAVR9YqbyyqFDQDLHYGmkgJykIrGF1XIpu+ILlaS/V9lZL
ubhzEFnTIZd+50xx+7LSYK05qAvqFyFWhfFQDlnrzuBZ6brJFe+GnY+EgPbk6ZGQ
3BebYhtF8GaV0nxvwuo77x/Py9auJ/GpsMiu/X1+mvoiBOv/2X/qkSsisRcOj/KK
NFtY2PwByVS5uCbMiogziUwthDyC3+6WVwW6LLv3xLfHTjuCvjHIInNzktHCgKQ5
ORAzI4JMPJ+GslWYHb4phowim57iaztXOoJwTdwJx4nLCgdNbOhdjsnvzqvHu7Ur
TkXWStAmzOVyyghqpZXjFaH3pO3JLF+l+/+sKAIuvtd7u+Nxe5AW0wdeRlN8NwdC
jNPElpzVmbUq4JUagEiuTDkHzsxHpFKVK7q4+63SM1N95R1NbdWhscdCb+ZAJzVc
oyi3B43njTOQ5yOf+1CceWxG1bQVs5ZufpsMljq4Ui0/1lvh+wjChP4kqKOJ2qxq
4RgqsahDYVvTH9w7jXbyLeiNdd8XM2w9U/t7y0Ff/9yi0GE44Za4rF2LN9d11TPA
mRGunUHBcnWEvgJBQl9nJEiU0Zsnvgc/ubhPgXRR4Xq37Z0j4r7g1SgEEzwxA57d
emyPxgcYxn/eR44/KJ4EBs+lVDR3veyJm+kXQ99b21/+jh5Xos1AnX5iItreGCc=
-----END CERTIFICATE-----"#;

/// ISRG Root X2 - Let's Encrypt ECDSA root
/// Valid until 2040-09-17
const ISRG_ROOT_X2_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIICGzCCAaGgAwIBAgIQQdKd0XLq7qeAwSxs6S+HUjAKBggqhkjOPQQDAzBPMQsw
CQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJuZXQgU2VjdXJpdHkgUmVzZWFyY2gg
R3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBYMjAeFw0yMDA5MDQwMDAwMDBaFw00
MDA5MTcxNjAwMDBaME8xCzAJBgNVBAYTAlVTMSkwJwYDVQQKEyBJbnRlcm5ldCBT
ZWN1cml0eSBSZXNlYXJjaCBHcm91cDEVMBMGA1UEAxMMSVNSRyBSb290IFgyMHYw
EAYHKoZIzj0CAQYFK4EEACIDYgAEzZvVn4CDCuwJSvMWSj5cz3es3mcFDR0HttwW
+1qLFNvicWDEukWVEYmO6gbf9yoWHKS5xcUy4APgHoIYOIvXRdgKam7mAHf7AlF9
ItgKbppbd9/w+kHsOdx1ymgHDB/qo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0T
AQH/BAUwAwEB/zAdBgNVHQ4EFgQUfEKWrt5LSDv6kviejM9ti6lyN5UwCgYIKoZI
zj0EAwMDaAAwZQIwe3lORlCEwkSHRhtFcP9Ymd70/aTSVaYgLXTWNLxBo1BfASdW
tL4ndQavEi51mI38AjEAi/V3bNTIZargCyzuFJ0nN6T5U6VR5CmD1/iQMVtCnwr1
/q4AaOeMSQ+2b1tbFfLn
-----END CERTIFICATE-----"#;

/// DigiCert Global Root G2 - Used by some CDNs and cloud providers
/// This is useful for fetching the full CA bundle
const DIGICERT_GLOBAL_ROOT_G2_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIDjjCCAnagAwIBAgIQAzrx5qcRqaC7KGSxHQn65TANBgkqhkiG9w0BAQsFADBh
MQswCQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3
d3cuZGlnaWNlcnQuY29tMSAwHgYDVQQDExdEaWdpQ2VydCBHbG9iYWwgUm9vdCBH
MjAeFw0xMzA4MDExMjAwMDBaFw0zODAxMTUxMjAwMDBaMGExCzAJBgNVBAYTAlVT
MRUwEwYDVQQKEwxEaWdpQ2VydCBJbmMxGTAXBgNVBAsTEHd3dy5kaWdpY2VydC5j
b20xIDAeBgNVBAMTF0RpZ2lDZXJ0IEdsb2JhbCBSb290IEcyMIIBIjANBgkqhkiG
9w0BAQEFAAOCAQ8AMIIBCgKCAQEAuzfNNNx7a8myaJCtSnX/RrohCgiN9RlUyfuI
2/Ou8jqJkTx65qsGGmvPrC3oXgkkRLpimn7Wo6h+4FR1IAWsULecYxpsMNzaHxmx
1x7e/dfgy5SDN67sH0NO3Xss0r0upS/kqbitOtSZpLYl6ZtrAGCSYP9PIUkY92eQ
q2EGnI/yuum06ZIya7XzV+hdG82MHauVBJVJ8zUtluNJbd134/tJS7SsVQepj5Wz
tCO7TG1F8PapspUwtP1MVYwnSlcUfIKdzXOS0xZKBgyMUNGPHgm+F6HmIcr9g+UQ
vIOlCsRnKPZzFBQ9RnbDhxSJITRNrw9FDKZJobq7nMWxM4MphQIDAQABo0IwQDAP
BgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBhjAdBgNVHQ4EFgQUTiJUIBiV
5uNu5g/6+rkS7QYXjzkwDQYJKoZIhvcNAQELBQADggEBAGBnKJRvDkhj6zHd6mcY
1Yl9PMWLSn/pvtsrF9+wX3N3KjITOYFnQoQj8kVnNeyIv/iPsGEMNKSuIEyExtv4
NeF22d+mQrvHRAiGfzZ0JFrabA0UWTW98kndth/Jsw1HKj2ZL7tcu7XUIOGZX1NG
Fdtom/DzMNU+MeKNhJ7jitralj41E6Vf8PlwUHBHQRFXGU7Aj64GxJUTFy8bJZ91
8rGOmaFvE7FBcf6IKshPECBV1/MUReXgRPTqh5Uykw7+U0b6LJ3/iyK5S9kJRaTe
pLiaWN0bfVKfjllDiIGknibVb63dDcY3fe0Dkhvld1927jyNxF1WW6LZZm6zNTfl
MrY=
-----END CERTIFICATE-----"#;

/// A parsed root certificate
#[derive(Clone)]
pub struct RootCertificate {
    /// DER-encoded certificate
    pub der: Vec<u8>,
    /// Subject name for matching
    pub subject: String,
}

impl RootCertificate {
    /// Parse a PEM-encoded certificate
    pub fn from_pem(pem: &str) -> Result<Self> {
        let pem_bytes = pem.as_bytes();
        let (_, pem_data) = x509_parser::pem::parse_x509_pem(pem_bytes)
            .map_err(|e| TlsError::certificate(format!("Failed to parse PEM: {}", e)))?;

        let (_, cert) = X509Certificate::from_der(&pem_data.contents)
            .map_err(|e| TlsError::certificate(format!("Failed to parse certificate: {}", e)))?;

        let subject = cert.subject().to_string();

        Ok(Self {
            der: pem_data.contents,
            subject,
        })
    }
}

/// Trust store with embedded and lazy-loaded certificates
pub struct TrustStore {
    /// Embedded root certificates (Let's Encrypt for Tor infrastructure)
    embedded_roots: Vec<RootCertificate>,
    /// Lazy-loaded full CA bundle (fetched via Tor)
    extended_roots: Rc<RefCell<Option<Vec<RootCertificate>>>>,
    /// URL to fetch the full CA bundle from
    ca_bundle_url: String,
}

impl TrustStore {
    /// Create a new trust store with embedded Let's Encrypt roots
    pub fn new() -> Result<Self> {
        let mut embedded_roots = Vec::new();

        // Parse embedded root certificates
        embedded_roots.push(RootCertificate::from_pem(ISRG_ROOT_X1_PEM)?);
        embedded_roots.push(RootCertificate::from_pem(ISRG_ROOT_X2_PEM)?);
        embedded_roots.push(RootCertificate::from_pem(DIGICERT_GLOBAL_ROOT_G2_PEM)?);

        info!(
            "Initialized trust store with {} embedded root CAs",
            embedded_roots.len()
        );

        Ok(Self {
            embedded_roots,
            extended_roots: Rc::new(RefCell::new(None)),
            ca_bundle_url: "https://curl.se/ca/cacert.pem".to_string(),
        })
    }

    /// Create a trust store with a custom CA bundle URL
    pub fn with_ca_bundle_url(mut self, url: &str) -> Self {
        self.ca_bundle_url = url.to_string();
        self
    }

    /// Check if we have the extended CA bundle loaded
    pub fn has_extended_roots(&self) -> bool {
        self.extended_roots.borrow().is_some()
    }

    /// Get all available root certificates (embedded only for now)
    pub fn get_roots(&self) -> Vec<&RootCertificate> {
        self.embedded_roots.iter().collect()
    }

    /// Find a root certificate that matches the given issuer
    ///
    /// Returns the DER-encoded certificate if found. Returns an owned value
    /// to support both embedded and dynamically-loaded extended roots.
    pub fn find_root_for_issuer(&self, issuer_der: &[u8]) -> Option<Vec<u8>> {
        // Parse the issuer to get subject
        if let Ok((_, issuer_cert)) = X509Certificate::from_der(issuer_der) {
            let issuer_subject = issuer_cert.subject().to_string();

            // Check embedded roots first
            for root in &self.embedded_roots {
                if root.subject == issuer_subject {
                    return Some(root.der.clone());
                }
            }

            // Check extended roots
            if let Some(ref extended) = *self.extended_roots.borrow() {
                for root in extended {
                    if root.subject == issuer_subject {
                        return Some(root.der.clone());
                    }
                }
            }
        }

        None
    }

    /// Find a root certificate reference (embedded roots only)
    ///
    /// For cases where a reference is needed and extended roots are not required.
    pub fn find_embedded_root_for_issuer(&self, issuer_der: &[u8]) -> Option<&RootCertificate> {
        if let Ok((_, issuer_cert)) = X509Certificate::from_der(issuer_der) {
            let issuer_subject = issuer_cert.subject().to_string();

            for root in &self.embedded_roots {
                if root.subject == issuer_subject {
                    return Some(root);
                }
            }
        }

        None
    }

    /// Check if a certificate is a trusted root
    pub fn is_trusted_root(&self, cert_der: &[u8]) -> bool {
        // Check embedded roots
        for root in &self.embedded_roots {
            if root.der == cert_der {
                return true;
            }
        }

        // Check extended roots
        if let Some(ref extended) = *self.extended_roots.borrow() {
            for root in extended {
                if root.der == cert_der {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a certificate was issued by a trusted root
    pub fn is_issued_by_trusted_root(&self, cert_der: &[u8]) -> bool {
        if let Ok((_, cert)) = X509Certificate::from_der(cert_der) {
            let issuer = cert.issuer().to_string();

            // Check embedded roots
            for root in &self.embedded_roots {
                if root.subject == issuer {
                    return true;
                }
            }

            // Check extended roots
            if let Some(ref extended) = *self.extended_roots.borrow() {
                for root in extended {
                    if root.subject == issuer {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Load the extended CA bundle from a PEM string
    pub fn load_extended_roots(&self, pem_bundle: &str) -> Result<usize> {
        let mut roots = Vec::new();

        // Parse all certificates from the PEM bundle
        let mut remaining = pem_bundle.as_bytes();
        while !remaining.is_empty() {
            match x509_parser::pem::parse_x509_pem(remaining) {
                Ok((rest, pem_data)) => {
                    if let Ok((_, cert)) = X509Certificate::from_der(&pem_data.contents) {
                        let subject = cert.subject().to_string();
                        roots.push(RootCertificate {
                            der: pem_data.contents,
                            subject,
                        });
                    }
                    remaining = rest;
                }
                Err(_) => break,
            }
        }

        let count = roots.len();
        *self.extended_roots.borrow_mut() = Some(roots);

        info!("Loaded {} extended root CAs", count);
        Ok(count)
    }

    /// Get the URL for fetching the full CA bundle
    pub fn ca_bundle_url(&self) -> &str {
        &self.ca_bundle_url
    }
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new().expect("Failed to create default trust store")
    }
}

// Global trust store instance
thread_local! {
    static TRUST_STORE: RefCell<Option<TrustStore>> = RefCell::new(None);
}

/// Get or initialize the global trust store
pub fn get_trust_store() -> Result<TrustStore> {
    TrustStore::new()
}

/// Embedded root certificate count (for bundle size estimation)
pub const EMBEDDED_ROOT_COUNT: usize = 3;
/// Approximate size of embedded roots in bytes
pub const EMBEDDED_ROOTS_SIZE: usize = 3500; // ~3.5KB for 3 certs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_embedded_roots() {
        let store = TrustStore::new().unwrap();
        assert_eq!(store.embedded_roots.len(), 3);
    }

    #[test]
    fn test_isrg_root_x1() {
        let root = RootCertificate::from_pem(ISRG_ROOT_X1_PEM).unwrap();
        assert!(root.subject.contains("ISRG Root X1"));
    }

    #[test]
    fn test_isrg_root_x2() {
        let root = RootCertificate::from_pem(ISRG_ROOT_X2_PEM).unwrap();
        assert!(root.subject.contains("ISRG Root X2"));
    }

    #[test]
    fn test_digicert_root() {
        let root = RootCertificate::from_pem(DIGICERT_GLOBAL_ROOT_G2_PEM).unwrap();
        assert!(root.subject.contains("DigiCert"));
    }
}
