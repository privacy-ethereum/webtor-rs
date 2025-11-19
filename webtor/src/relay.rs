//! Tor relay management and selection

use crate::error::{Result, TorError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{debug, info, warn};

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
}

/// Relay selection criteria
#[derive(Debug, Clone)]
pub struct RelayCriteria {
    pub need_flags: HashSet<String>,
    pub exclude_flags: HashSet<String>,
    pub min_bandwidth: u64,
    pub max_selection: usize,
}

impl Default for RelayCriteria {
    fn default() -> Self {
        Self {
            need_flags: HashSet::new(),
            exclude_flags: HashSet::new(),
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
        let candidates = self.select_relays(criteria)?;
        
        if candidates.is_empty() {
            return Err(TorError::relay_selection("No suitable relays found"));
        }
        
        // For now, just select the first one (highest consensus weight)
        // In a real implementation, we'd add some randomization
        Ok(candidates[0].clone())
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
        Relay {
            fingerprint: fingerprint.to_string(),
            nickname: format!("test_{}", fingerprint),
            address: "127.0.0.1".to_string(),
            or_port: 9001,
            dir_port: Some(9030),
            flags: flags.into_iter().map(String::from).collect(),
            bandwidth: 1000000,
            consensus_weight: 100,
            version: "0.4.5.6".to_string(),
            microdescriptor_hash: "test_hash".to_string(),
        }
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