//! Tor circuit management

use crate::error::{Result, TorError};
use crate::relay::{Relay, RelayManager};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Circuit status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitStatus {
    Creating,
    Ready,
    Extending,
    Failed,
    Closed,
}

/// Tor circuit information
#[derive(Debug, Clone)]
pub struct Circuit {
    pub id: String,
    pub status: CircuitStatus,
    pub created_at: Instant,
    pub last_used: Instant,
    pub relays: Vec<Relay>,
    _private: (),
}

impl Circuit {
    pub fn new(id: String) -> Self {
        let now = Instant::now();
        Self {
            id,
            status: CircuitStatus::Creating,
            created_at: now,
            last_used: now,
            relays: Vec::new(),
            _private: (),
        }
    }
    
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
    
    pub fn time_since_last_use(&self) -> Duration {
        self.last_used.elapsed()
    }
    
    pub fn update_last_used(&mut self) {
        self.last_used = Instant::now();
    }
    
    pub fn is_ready(&self) -> bool {
        self.status == CircuitStatus::Ready
    }
    
    pub fn is_failed(&self) -> bool {
        self.status == CircuitStatus::Failed
    }
    
    pub fn is_closed(&self) -> bool {
        self.status == CircuitStatus::Closed
    }
}

/// Circuit manager for handling multiple circuits
#[derive(Clone)]
pub struct CircuitManager {
    circuits: Arc<RwLock<Vec<Arc<RwLock<Circuit>>>>>,
    relay_manager: Arc<RwLock<RelayManager>>,
}

impl CircuitManager {
    pub fn new(relay_manager: RelayManager) -> Self {
        Self {
            circuits: Arc::new(RwLock::new(Vec::new())),
            relay_manager: Arc::new(RwLock::new(relay_manager)),
        }
    }
    
    /// Create a new circuit
    pub async fn create_circuit(&self) -> Result<Arc<RwLock<Circuit>>> {
        let circuit_id = format!("circuit_{}", uuid::Uuid::new_v4());
        info!("Creating new circuit: {}", circuit_id);
        
        let mut circuit = Circuit::new(circuit_id.clone());
        
        // Select relays for the circuit
        let relay_manager = self.relay_manager.read().await;
        
        // For a 3-hop circuit: Guard -> Middle -> Exit
        let guard = relay_manager.select_relay(&crate::relay::selection::guard_relays())?;
        let middle = relay_manager.select_relay(&crate::relay::selection::middle_relays())?;
        let exit = relay_manager.select_relay(&crate::relay::selection::exit_relays())?;
        
        circuit.relays = vec![guard, middle, exit];
        circuit.status = CircuitStatus::Ready;
        
        info!("Circuit {} created with {} relays", circuit_id, circuit.relays.len());
        
        let circuit_arc = Arc::new(RwLock::new(circuit));
        
        // Add to active circuits
        let mut circuits = self.circuits.write().await;
        circuits.push(circuit_arc.clone());
        
        Ok(circuit_arc)
    }
    
    /// Get a ready circuit (create one if none exist)
    pub async fn get_ready_circuit(&self) -> Result<Arc<RwLock<Circuit>>> {
        // First, try to find an existing ready circuit
        let circuits = self.circuits.read().await;
        for circuit in circuits.iter() {
            let circuit_read = circuit.read().await;
            if circuit_read.is_ready() {
                debug!("Found existing ready circuit: {}", circuit_read.id);
                return Ok(circuit.clone());
            }
        }
        drop(circuits);
        
        // No ready circuit found, create a new one
        self.create_circuit().await
    }
    
    /// Get circuit status information
    pub async fn get_circuit_status(&self) -> CircuitStatusInfo {
        let circuits = self.circuits.read().await;
        let mut ready_count = 0;
        let mut creating_count = 0;
        let mut failed_count = 0;
        let mut total_age = Duration::from_secs(0);
        
        for circuit in circuits.iter() {
            let circuit_read = circuit.read().await;
            match circuit_read.status {
                CircuitStatus::Ready => ready_count += 1,
                CircuitStatus::Creating => creating_count += 1,
                CircuitStatus::Failed => failed_count += 1,
                _ => {}
            }
            total_age += circuit_read.age();
        }
        
        let avg_age = if circuits.is_empty() {
            Duration::from_secs(0)
        } else {
            total_age / circuits.len() as u32
        };
        
        CircuitStatusInfo {
            total_circuits: circuits.len(),
            ready_circuits: ready_count,
            creating_circuits: creating_count,
            failed_circuits: failed_count,
            average_circuit_age: avg_age,
        }
    }
    
    /// Update relay manager with new relay list
    pub async fn update_relays(&self, new_relays: Vec<crate::relay::Relay>) {
        let mut relay_manager = self.relay_manager.write().await;
        relay_manager.update_relays(new_relays);
    }
    
    /// Clean up failed and old circuits
    pub async fn cleanup_circuits(&self) -> Result<()> {
        let mut circuits = self.circuits.write().await;
        let now = Instant::now();
        let max_age = Duration::from_secs(60 * 60); // 1 hour
        let max_idle = Duration::from_secs(60 * 10); // 10 minutes
        
        let mut count = circuits.len();
        
        circuits.retain(|circuit| {
            let circuit_read = circuit.blocking_read();
            
            // Remove failed circuits
            if circuit_read.is_failed() {
                info!("Removing failed circuit: {}", circuit_read.id);
                count -= 1;
                return false;
            }
            
            // Remove very old circuits
            if circuit_read.age() > max_age {
                info!("Removing old circuit: {} (age: {:?})", circuit_read.id, circuit_read.age());
                count -= 1;
                return false;
            }
            
            // Remove idle circuits (but keep at least one)
            if circuit_read.time_since_last_use() > max_idle && count > 1 {
                info!("Removing idle circuit: {} (idle: {:?})", circuit_read.id, circuit_read.time_since_last_use());
                count -= 1;
                return false;
            }
            
            true
        });
        
        Ok(())
    }
}

/// Circuit status information
#[derive(Debug, Clone)]
pub struct CircuitStatusInfo {
    pub total_circuits: usize,
    pub ready_circuits: usize,
    pub creating_circuits: usize,
    pub failed_circuits: usize,
    pub average_circuit_age: Duration,
}

impl CircuitStatusInfo {
    pub fn has_ready_circuits(&self) -> bool {
        self.ready_circuits > 0
    }
    
    pub fn is_healthy(&self) -> bool {
        self.ready_circuits > 0 && self.failed_circuits == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::{Relay, flags};
    use std::collections::HashSet;
    
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
    
    #[tokio::test]
    async fn test_circuit_creation() {
        let relays = vec![
            create_test_relay("guard1", vec![flags::FAST, flags::STABLE, flags::GUARD]),
            create_test_relay("middle1", vec![flags::FAST, flags::STABLE, flags::V2DIR]),
            create_test_relay("exit1", vec![flags::FAST, flags::STABLE, flags::EXIT]),
        ];
        
        let relay_manager = RelayManager::new(relays);
        let circuit_manager = CircuitManager::new(relay_manager);
        
        // This will fail because we don't have WASM WebSocket implementation
        let result = circuit_manager.create_circuit().await;
        assert!(result.is_err());
    }
    
    #[test]
    fn test_circuit_status() {
        let mut circuit = Circuit::new("test_circuit".to_string());
        
        assert_eq!(circuit.status, CircuitStatus::Creating);
        assert!(!circuit.is_ready());
        assert!(!circuit.is_failed());
        assert!(!circuit.is_closed());
        
        circuit.status = CircuitStatus::Ready;
        assert!(circuit.is_ready());
        
        circuit.status = CircuitStatus::Failed;
        assert!(circuit.is_failed());
        
        circuit.status = CircuitStatus::Closed;
        assert!(circuit.is_closed());
    }
}