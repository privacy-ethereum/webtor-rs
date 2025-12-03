//! Tor relay management and selection

use crate::error::{Result, TorError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{debug, info};
use tor_linkspec::OwnedCircTarget;
use tor_llcrypto::pk::{rsa::RsaIdentity, ed25519::Ed25519Identity, curve25519::PublicKey as Curve25519PublicKey};
use tor_protover::Protocols;
use std::str::FromStr;

/// Tor relay information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relay {
    pub fingerprint: String,
    pub nickname: String,
    pub address: String,
    pub or_port: u16,
    pub dir_port: Option<u16>,
    pub flags: HashSet<String>,
    pub bandwidth: u64,
    pub consensus_weight: u32,
    pub version: String,
    pub microdescriptor_hash: String,
    
    // New fields for keys
    #[serde(default)]
    pub ed25519_identity: Option<String>, // Hex encoded
    #[serde(default)]
    pub ntor_onion_key: String, // Hex encoded
}

impl Relay {
    /// Create a new Relay instance
    pub fn new(
        fingerprint: String,
        nickname: String,
        address: String,
        or_port: u16,
        flags: HashSet<String>,
        ntor_onion_key: String,
    ) -> Self {
        Self {
            fingerprint,
            nickname,
            address,
            or_port,
            dir_port: None,
            flags,
            bandwidth: 0,
            consensus_weight: 0,
            version: String::new(),
            microdescriptor_hash: String::new(),
            ed25519_identity: None,
            ntor_onion_key,
        }
    }

    /// Convert to OwnedCircTarget for circuit creation
    pub fn as_circ_target(&self) -> Result<OwnedCircTarget> {
        let mut builder = OwnedCircTarget::builder();
        
        {
            let chan_builder = builder.chan_target();
            
            // Add addresses
            if let Ok(ip) = std::net::IpAddr::from_str(&self.address) {
                chan_builder.addrs(vec![std::net::SocketAddr::new(ip, self.or_port)]);
            } else {
                 return Err(TorError::Configuration(format!("Invalid relay address: {}", self.address)));
            }
            
            // RSA Identity
            if let Ok(bytes) = hex::decode(&self.fingerprint) {
                 if bytes.len() == 20 {
                     if let Some(rsa_id) = RsaIdentity::from_bytes(&bytes) {
                         chan_builder.rsa_identity(rsa_id);
                     } else {
                         return Err(TorError::Configuration("Invalid RSA identity".to_string()));
                     }
                 } else {
                     return Err(TorError::Configuration("Invalid RSA identity length".to_string()));
                 }
            } else {
                return Err(TorError::Configuration("Invalid RSA identity hex".to_string()));
            }
            
            // Ed25519 Identity
            if let Some(ref ed_hex) = self.ed25519_identity {
                 if let Ok(bytes) = hex::decode(ed_hex) {
                     if let Some(ed_id) = Ed25519Identity::from_bytes(&bytes) {
                         chan_builder.ed_identity(ed_id);
                     }
                 }
            }
        }
        
        // ntor key
        if let Ok(bytes) = hex::decode(&self.ntor_onion_key) {
             if bytes.len() == 32 {
                 let mut key_bytes = [0u8; 32];
                 key_bytes.copy_from_slice(&bytes);
                 builder.ntor_onion_key(Curve25519PublicKey::from(key_bytes));
             } else {
                 return Err(TorError::Configuration("Invalid ntor key length".to_string()));
             }
        } else {
            return Err(TorError::Configuration("Invalid ntor key hex".to_string()));
        };
        
        // Protocols
        builder.protocols(Protocols::default());
        
        builder.build()
            .map_err(|e| TorError::Internal(format!("Failed to build circ target: {}", e)))
    }
}

/// Relay selection criteria
#[derive(Debug, Clone)]
pub struct RelayCriteria {
    pub need_flags: HashSet<String>,
    pub exclude_flags: HashSet<String>,
    pub exclude_fingerprints: HashSet<String>,
    pub min_bandwidth: u64,
    pub max_selection: usize,
}

impl Default for RelayCriteria {
    fn default() -> Self {
        Self {
            need_flags: HashSet::new(),
            exclude_flags: HashSet::new(),
            exclude_fingerprints: HashSet::new(),
            min_bandwidth: 0,
            max_selection: 10,
        }
    }
}

impl RelayCriteria {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_flag(mut self, flag: &str) -> Self {
        self.need_flags.insert(flag.to_string());
        self
    }
    
    pub fn without_flag(mut self, flag: &str) -> Self {
        self.exclude_flags.insert(flag.to_string());
        self
    }
    
    pub fn without_fingerprint(mut self, fingerprint: &str) -> Self {
        self.exclude_fingerprints.insert(fingerprint.to_string());
        self
    }
    
    pub fn without_fingerprints(mut self, fingerprints: Vec<String>) -> Self {
        for fp in fingerprints {
            self.exclude_fingerprints.insert(fp);
        }
        self
    }
    
    pub fn with_min_bandwidth(mut self, bandwidth: u64) -> Self {
        self.min_bandwidth = bandwidth;
        self
    }
    
    pub fn with_max_selection(mut self, max: usize) -> Self {
        self.max_selection = max;
        self
    }
}

/// Relay manager for selecting appropriate relays
pub struct RelayManager {
    relays: Vec<Relay>,
}

impl RelayManager {
    pub fn new(relays: Vec<Relay>) -> Self {
        Self { relays }
    }
    
    /// Select relays matching the given criteria
    pub fn select_relays(&self, criteria: &RelayCriteria) -> Result<Vec<Relay>> {
        let mut candidates: Vec<&Relay> = self.relays.iter()
            .filter(|relay| {
                // Check excluded fingerprints
                if criteria.exclude_fingerprints.contains(&relay.fingerprint) {
                    return false;
                }

                // Check required flags
                for flag in &criteria.need_flags {
                    if !relay.flags.contains(flag) {
                        return false;
                    }
                }
                
                // Check excluded flags
                for flag in &criteria.exclude_flags {
                    if relay.flags.contains(flag) {
                        return false;
                    }
                }
                
                // Check bandwidth
                relay.bandwidth >= criteria.min_bandwidth
            })
            .collect();
        
        if candidates.is_empty() {
            return Err(TorError::relay_selection(
                "No relays match the selection criteria"
            ));
        }
        
        // Sort by consensus weight (higher is better)
        candidates.sort_by_key(|relay| std::cmp::Reverse(relay.consensus_weight));
        
        // Take top candidates
        let selected: Vec<Relay> = candidates
            .into_iter()
            .take(criteria.max_selection)
            .cloned()
            .collect();
        
        info!("Selected {} relays from {} total", selected.len(), self.relays.len());
        debug!("Selection criteria: {:?}", criteria);
        
        Ok(selected)
    }
    
    /// Select a single relay randomly from candidates
    pub fn select_relay(&self, criteria: &RelayCriteria) -> Result<Relay> {
        use rand::seq::SliceRandom;
        
        let candidates = self.select_relays(criteria)?;
        
        if candidates.is_empty() {
            return Err(TorError::relay_selection("No suitable relays found"));
        }
        
        // Select a random relay from the top candidates to avoid deterministic paths
        // and ensure load balancing
        let mut rng = rand::thread_rng();
        candidates.choose(&mut rng)
            .cloned()
            .ok_or_else(|| TorError::relay_selection("Failed to choose random relay"))
    }
    
    /// Get relay by fingerprint
    pub fn get_relay(&self, fingerprint: &str) -> Option<&Relay> {
        self.relays.iter().find(|relay| relay.fingerprint == fingerprint)
    }
    
    /// Update relay list from consensus
    pub fn update_relays(&mut self, new_relays: Vec<Relay>) {
        info!("Updating relay list: {} -> {} relays", self.relays.len(), new_relays.len());
        self.relays = new_relays;
    }
}

/// Common relay flag constants
pub mod flags {
    pub const AUTHORITY: &str = "Authority";
    pub const BAD_EXIT: &str = "BadExit";
    pub const EXIT: &str = "Exit";
    pub const FAST: &str = "Fast";
    pub const GUARD: &str = "Guard";
    pub const HSDIR: &str = "HSDir";
    pub const NAMED: &str = "Named";
    pub const STABLE: &str = "Stable";
    pub const RUNNING: &str = "Running";
    pub const VALID: &str = "Valid";
    pub const V2DIR: &str = "V2Dir";
}

/// Helper functions for common relay selections
pub mod selection {
    use super::*;
    
    /// Select middle relays (Fast, Stable, V2Dir)
    pub fn middle_relays() -> RelayCriteria {
        RelayCriteria::new()
            .with_flag(flags::FAST)
            .with_flag(flags::STABLE)
            .with_flag(flags::V2DIR)
    }
    
    /// Select exit relays (Fast, Stable, Exit, not BadExit)
    pub fn exit_relays() -> RelayCriteria {
        RelayCriteria::new()
            .with_flag(flags::FAST)
            .with_flag(flags::STABLE)
            .with_flag(flags::EXIT)
            .without_flag(flags::BAD_EXIT)
    }
    
    /// Select guard relays (Fast, Stable, Guard)
    pub fn guard_relays() -> RelayCriteria {
        RelayCriteria::new()
            .with_flag(flags::FAST)
            .with_flag(flags::STABLE)
            .with_flag(flags::GUARD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_relay(fingerprint: &str, flags: Vec<&str>) -> Relay {
        Relay::new(
            fingerprint.to_string(),
            format!("test_{}", fingerprint),
            "127.0.0.1".to_string(),
            9001,
            flags.into_iter().map(String::from).collect(),
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        )
    }
    
    #[test]
    fn test_relay_selection() {
        let relays = vec![
            create_test_relay("relay1", vec![flags::FAST, flags::STABLE, flags::V2DIR]),
            create_test_relay("relay2", vec![flags::FAST, flags::STABLE, flags::EXIT]),
            create_test_relay("relay3", vec![flags::FAST, flags::STABLE, flags::EXIT, flags::BAD_EXIT]),
        ];
        
        let manager = RelayManager::new(relays);
        
        // Test middle relay selection
        let middle_criteria = selection::middle_relays();
        let middle_relays = manager.select_relays(&middle_criteria).unwrap();
        assert_eq!(middle_relays.len(), 1);
        assert!(middle_relays[0].flags.contains(flags::V2DIR));
        
        // Test exit relay selection
        let exit_criteria = selection::exit_relays();
        let exit_relays = manager.select_relays(&exit_criteria).unwrap();
        assert_eq!(exit_relays.len(), 1);
        assert!(exit_relays[0].flags.contains(flags::EXIT));
        assert!(!exit_relays[0].flags.contains(flags::BAD_EXIT));
    }
}
