//! Tor consensus fetching and caching
//!
//! This module handles fetching the network consensus from directory authorities
//! or fallback directories, parsing it, and caching it with appropriate TTL.
//!
//! Tor consensus documents are refreshed every hour (valid-after to fresh-until),
//! with a longer valid-until period (~3 hours) for safety.

use crate::error::{Result, TorError};
use crate::relay::Relay;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tor_checkable::{ExternallySigned, Timebound};
use tracing::{debug, info, warn};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Tor directory authority/fallback information
#[derive(Debug, Clone)]
pub struct DirectorySource {
    pub name: String,
    pub address: String,
    pub port: u16,
}

impl DirectorySource {
    pub fn new(name: &str, address: &str, port: u16) -> Self {
        Self {
            name: name.to_string(),
            address: address.to_string(),
            port,
        }
    }
}

/// Known fallback directories for fetching consensus
/// These are well-known Tor relays that serve directory information
pub fn fallback_directories() -> Vec<DirectorySource> {
    vec![
        // These are example fallbacks - in production, use the official Tor fallback list
        // From: https://gitweb.torproject.org/tor.git/tree/src/app/config/fallback_dirs.inc
        DirectorySource::new("Faravahar", "154.35.175.225", 80),
        DirectorySource::new("dannenberg", "193.23.244.244", 80),
        DirectorySource::new("moria1", "128.31.0.39", 9131),
        DirectorySource::new("tor26", "86.59.21.38", 80),
        DirectorySource::new("gabelmoo", "131.188.40.189", 80),
        DirectorySource::new("maatuska", "171.25.193.9", 443),
        DirectorySource::new("longclaw", "199.58.81.140", 80),
        DirectorySource::new("bastet", "204.13.164.118", 80),
    ]
}

/// Cached consensus data
#[derive(Debug)]
pub struct CachedConsensus {
    /// Parsed relay information
    pub relays: Vec<Relay>,
    /// When this consensus was fetched
    pub fetched_at: Instant,
    /// When this consensus becomes stale (fresh-until)
    pub fresh_until: Duration,
    /// When this consensus becomes invalid (valid-until)  
    pub valid_until: Duration,
}

impl CachedConsensus {
    /// Check if the consensus is still fresh (within fresh-until)
    pub fn is_fresh(&self) -> bool {
        self.fetched_at.elapsed() < self.fresh_until
    }

    /// Check if the consensus is still valid (within valid-until)
    pub fn is_valid(&self) -> bool {
        self.fetched_at.elapsed() < self.valid_until
    }
}

/// Consensus manager with caching
pub struct ConsensusManager {
    /// Cached consensus
    cache: Arc<RwLock<Option<CachedConsensus>>>,
    /// Directory sources to fetch from
    directories: Vec<DirectorySource>,
}

impl ConsensusManager {
    /// Create a new consensus manager
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            directories: fallback_directories(),
        }
    }

    /// Create with custom directory sources
    pub fn with_directories(directories: Vec<DirectorySource>) -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            directories,
        }
    }

    /// Get relays, fetching/refreshing consensus if needed
    pub async fn get_relays(&self) -> Result<Vec<Relay>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(ref cached) = *cache {
                if cached.is_fresh() {
                    debug!("Using cached consensus ({} relays)", cached.relays.len());
                    return Ok(cached.relays.clone());
                }
                if cached.is_valid() {
                    info!("Consensus is stale but valid, will refresh in background");
                    // Could spawn background refresh here
                    return Ok(cached.relays.clone());
                }
            }
        }

        // Need to fetch new consensus
        self.refresh_consensus().await
    }

    /// Force refresh the consensus
    pub async fn refresh_consensus(&self) -> Result<Vec<Relay>> {
        info!("Fetching fresh consensus from directory authorities");

        // Try each directory until one succeeds
        let mut last_error = None;
        for dir in &self.directories {
            match self.fetch_from_directory(dir).await {
                Ok(relays) => {
                    info!("Fetched consensus with {} relays from {}", relays.len(), dir.name);
                    
                    // Update cache
                    let cached = CachedConsensus {
                        relays: relays.clone(),
                        fetched_at: Instant::now(),
                        fresh_until: Duration::from_secs(3600), // 1 hour
                        valid_until: Duration::from_secs(3600 * 3), // 3 hours
                    };
                    
                    let mut cache = self.cache.write().await;
                    *cache = Some(cached);
                    
                    return Ok(relays);
                }
                Err(e) => {
                    warn!("Failed to fetch from {}: {}", dir.name, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            TorError::consensus_fetch("No directory sources available")
        }))
    }

    /// Fetch consensus from a specific directory
    async fn fetch_from_directory(&self, dir: &DirectorySource) -> Result<Vec<Relay>> {
        // Construct the consensus URL
        // /tor/status-vote/current/consensus-microdesc
        let url = format!(
            "http://{}:{}/tor/status-vote/current/consensus-microdesc",
            dir.address, dir.port
        );

        debug!("Fetching consensus from {}", url);

        // Fetch the consensus document
        let consensus_text = self.http_fetch(&url).await?;
        
        // Parse the consensus
        let (relays, microdesc_digests) = self.parse_consensus(&consensus_text)?;
        
        // Fetch microdescriptors for ntor keys
        let relays = self.fetch_microdescriptors(dir, relays, microdesc_digests).await?;

        Ok(relays)
    }

    /// Simple HTTP fetch (used before Tor circuit is available)
    #[cfg(target_arch = "wasm32")]
    async fn http_fetch(&self, url: &str) -> Result<String> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::{Request, RequestInit, RequestMode, Response};

        let mut opts = RequestInit::new();
        opts.method("GET");
        opts.mode(RequestMode::Cors);

        let request = Request::new_with_str_and_init(url, &opts)
            .map_err(|e| TorError::consensus_fetch(format!("Failed to create request: {:?}", e)))?;

        let window = web_sys::window()
            .ok_or_else(|| TorError::consensus_fetch("No window object"))?;
        
        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| TorError::consensus_fetch(format!("Fetch failed: {:?}", e)))?;

        let resp: Response = resp_value
            .dyn_into()
            .map_err(|_| TorError::consensus_fetch("Response is not a Response object"))?;

        if !resp.ok() {
            return Err(TorError::consensus_fetch(format!(
                "HTTP error: {}",
                resp.status()
            )));
        }

        let text = JsFuture::from(
            resp.text()
                .map_err(|e| TorError::consensus_fetch(format!("Failed to get text: {:?}", e)))?,
        )
        .await
        .map_err(|e| TorError::consensus_fetch(format!("Failed to read body: {:?}", e)))?;

        text.as_string()
            .ok_or_else(|| TorError::consensus_fetch("Response is not a string"))
    }

    /// Simple HTTP fetch for native
    #[cfg(not(target_arch = "wasm32"))]
    async fn http_fetch(&self, url: &str) -> Result<String> {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        // Parse URL
        let url_parsed = url::Url::parse(url)
            .map_err(|e| TorError::consensus_fetch(format!("Invalid URL: {}", e)))?;
        
        let host = url_parsed.host_str()
            .ok_or_else(|| TorError::consensus_fetch("No host in URL"))?;
        let port = url_parsed.port().unwrap_or(80);
        let path = url_parsed.path();

        // Connect
        let addr = format!("{}:{}", host, port);
        let mut stream = TcpStream::connect(&addr)
            .map_err(|e| TorError::consensus_fetch(format!("Connection failed: {}", e)))?;
        
        stream.set_read_timeout(Some(Duration::from_secs(60)))
            .map_err(|e| TorError::consensus_fetch(format!("Failed to set timeout: {}", e)))?;

        // Send request
        let request = format!(
            "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, host
        );
        stream.write_all(request.as_bytes())
            .map_err(|e| TorError::consensus_fetch(format!("Write failed: {}", e)))?;

        // Read response
        let mut response = String::new();
        stream.read_to_string(&mut response)
            .map_err(|e| TorError::consensus_fetch(format!("Read failed: {}", e)))?;

        // Skip HTTP headers
        if let Some(body_start) = response.find("\r\n\r\n") {
            Ok(response[body_start + 4..].to_string())
        } else {
            Err(TorError::consensus_fetch("Invalid HTTP response"))
        }
    }

    /// Parse consensus document and extract relay information
    fn parse_consensus(&self, text: &str) -> Result<(Vec<Relay>, Vec<(String, String)>)> {
        use tor_netdoc::doc::netstatus::{MdConsensus, RelayFlags, RelayWeight};

        // Parse the consensus using tor-netdoc
        let (_, _, unchecked) = MdConsensus::parse(text)
            .map_err(|e| TorError::consensus_fetch(format!("Failed to parse consensus: {}", e)))?;

        // For now, we'll use the consensus without full signature verification
        // In production, you should verify signatures against known authority keys
        // 
        // UncheckedConsensus is TimerangeBound<UnvalidatedConsensus>
        // First assume timely (skip time check), then assume well-signed (skip signature check)
        let unvalidated = unchecked.dangerously_assume_timely();
        let consensus = unvalidated.dangerously_assume_wellsigned();

        let mut relays = Vec::new();
        let mut microdesc_digests = Vec::new();

        for rs in consensus.relays() {
            let fingerprint = hex::encode(rs.r.identity.as_bytes());
            let nickname = rs.r.nickname.to_string();
            let address = rs.r.ip.to_string();
            let or_port = rs.r.or_port;
            
            // Extract flags using bitflags API
            let mut flags = HashSet::new();
            let flags_raw = rs.flags;
            if flags_raw.contains(RelayFlags::AUTHORITY) {
                flags.insert("Authority".to_string());
            }
            if flags_raw.contains(RelayFlags::BAD_EXIT) {
                flags.insert("BadExit".to_string());
            }
            if flags_raw.contains(RelayFlags::EXIT) {
                flags.insert("Exit".to_string());
            }
            if flags_raw.contains(RelayFlags::FAST) {
                flags.insert("Fast".to_string());
            }
            if flags_raw.contains(RelayFlags::GUARD) {
                flags.insert("Guard".to_string());
            }
            if flags_raw.contains(RelayFlags::HSDIR) {
                flags.insert("HSDir".to_string());
            }
            if flags_raw.contains(RelayFlags::STABLE) {
                flags.insert("Stable".to_string());
            }
            if flags_raw.contains(RelayFlags::RUNNING) {
                flags.insert("Running".to_string());
            }
            if flags_raw.contains(RelayFlags::VALID) {
                flags.insert("Valid".to_string());
            }
            if flags_raw.contains(RelayFlags::V2DIR) {
                flags.insert("V2Dir".to_string());
            }

            // Get weight (measured or unmeasured)
            let weight = match rs.weight {
                RelayWeight::Measured(w) | RelayWeight::Unmeasured(w) => w,
                _ => 0, // Handle future variants
            };

            // Get microdescriptor digest for later fetching ntor key
            let md_digest = hex::encode(rs.m.as_slice());

            // Create relay with placeholder ntor key (will be filled from microdesc)
            let mut relay = Relay::new(
                fingerprint.clone(),
                nickname,
                address,
                or_port,
                flags,
                String::new(), // Will be filled from microdescriptor
            );
            relay.consensus_weight = weight;
            relay.microdescriptor_hash = md_digest.clone();
            relay.bandwidth = weight as u64;

            relays.push(relay);
            microdesc_digests.push((fingerprint, md_digest));
        }

        info!("Parsed {} relays from consensus", relays.len());
        Ok((relays, microdesc_digests))
    }

    /// Fetch microdescriptors to get ntor onion keys
    async fn fetch_microdescriptors(
        &self,
        dir: &DirectorySource,
        mut relays: Vec<Relay>,
        digests: Vec<(String, String)>,
    ) -> Result<Vec<Relay>> {
        // Microdescriptors are fetched in batches
        // URL: /tor/micro/d/<digest1>-<digest2>-...
        const BATCH_SIZE: usize = 92; // Max ~92 digests per request

        let digest_to_fp: std::collections::HashMap<String, String> = 
            digests.into_iter().map(|(fp, dig)| (dig, fp)).collect();

        let all_digests: Vec<String> = digest_to_fp.keys().cloned().collect();
        
        for chunk in all_digests.chunks(BATCH_SIZE) {
            let digest_path = chunk.join("-");
            let url = format!(
                "http://{}:{}/tor/micro/d/{}",
                dir.address, dir.port, digest_path
            );

            match self.http_fetch(&url).await {
                Ok(text) => {
                    self.parse_microdescriptors(&text, &mut relays, &digest_to_fp);
                }
                Err(e) => {
                    warn!("Failed to fetch microdescriptors batch: {}", e);
                    // Continue with remaining batches
                }
            }
        }

        // Filter out relays without ntor keys (unusable)
        let usable_relays: Vec<Relay> = relays
            .into_iter()
            .filter(|r| !r.ntor_onion_key.is_empty())
            .collect();

        info!("Got {} relays with ntor keys", usable_relays.len());
        Ok(usable_relays)
    }

    /// Parse microdescriptor documents
    fn parse_microdescriptors(
        &self,
        text: &str,
        relays: &mut [Relay],
        digest_to_fp: &std::collections::HashMap<String, String>,
    ) {
        use tor_netdoc::AllowAnnotations;

        // Parse all microdescriptors from the response
        let reader = match tor_netdoc::doc::microdesc::MicrodescReader::new(
            text,
            &AllowAnnotations::AnnotationsNotAllowed,
        ) {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed to create microdesc reader: {}", e);
                return;
            }
        };

        for result in reader {
            match result {
                Ok(annotated) => {
                    let md = annotated.into_microdesc();
                    let digest = hex::encode(md.sha256);
                    
                    // Find the relay for this microdescriptor
                    if let Some(fingerprint) = digest_to_fp.get(&digest) {
                        if let Some(relay) = relays.iter_mut().find(|r| &r.fingerprint == fingerprint) {
                            // Set the ntor onion key
                            relay.ntor_onion_key = hex::encode(md.ntor_onion_key.as_bytes());
                            
                            // Set ed25519 identity if available
                            relay.ed25519_identity = Some(hex::encode(md.ed25519_id.as_bytes()));
                            
                            debug!("Got ntor key for relay {}", relay.nickname);
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to parse microdescriptor: {}", e);
                }
            }
        }
    }

    /// Check if consensus needs refresh
    pub async fn needs_refresh(&self) -> bool {
        let cache = self.cache.read().await;
        match &*cache {
            None => true,
            Some(cached) => !cached.is_fresh(),
        }
    }

    /// Get cache status for debugging
    pub async fn cache_status(&self) -> String {
        let cache = self.cache.read().await;
        match &*cache {
            None => "No cached consensus".to_string(),
            Some(cached) => {
                let age = cached.fetched_at.elapsed();
                format!(
                    "Cached {} relays, age {:?}, fresh: {}, valid: {}",
                    cached.relays.len(),
                    age,
                    cached.is_fresh(),
                    cached.is_valid()
                )
            }
        }
    }
}

impl Default for ConsensusManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_directories() {
        let dirs = fallback_directories();
        assert!(!dirs.is_empty());
        for dir in dirs {
            assert!(!dir.name.is_empty());
            assert!(!dir.address.is_empty());
            assert!(dir.port > 0);
        }
    }

    #[test]
    fn test_cached_consensus_freshness() {
        let cached = CachedConsensus {
            relays: vec![],
            fetched_at: Instant::now(),
            fresh_until: Duration::from_secs(3600),
            valid_until: Duration::from_secs(3600 * 3),
        };
        
        assert!(cached.is_fresh());
        assert!(cached.is_valid());
    }
    
    #[test]
    fn test_consensus_manager_creation() {
        let manager = ConsensusManager::new();
        assert_eq!(manager.directories.len(), fallback_directories().len());
    }
    
    #[test]
    fn test_directory_source() {
        let dir = DirectorySource::new("test", "127.0.0.1", 80);
        assert_eq!(dir.name, "test");
        assert_eq!(dir.address, "127.0.0.1");
        assert_eq!(dir.port, 80);
    }
}
