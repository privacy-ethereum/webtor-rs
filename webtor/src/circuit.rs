//! Tor circuit management

use crate::config::MAX_CIRCUITS_PER_ISOLATION_KEY;
use crate::error::{Result, TorError};
use crate::isolation::IsolationKey;
use crate::relay::{Relay, RelayManager};
use crate::time::Instant;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tor_linkspec::HasRelayIds;
use tor_proto::ccparams::{
    Algorithm, CongestionControlParamsBuilder, CongestionWindowParamsBuilder,
    FixedWindowParamsBuilder, RoundTripEstimatorParamsBuilder,
};
use tor_proto::channel::Channel;
use tor_proto::circuit::CircParameters;
use tor_proto::client::circuit::TimeoutEstimator;
use tor_proto::client::stream::DataStream;
use tor_proto::{CellCount, ClientTunnel, FlowCtrlParameters};
use tor_units::Percentage;
use tracing::{debug, error, info};

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
pub struct Circuit {
    pub id: String,
    pub status: CircuitStatus,
    pub created_at: Instant,
    pub last_used: Instant,
    pub relays: Vec<Relay>,
    pub(crate) internal_circuit: Option<Arc<ClientTunnel>>,
    /// Stream isolation key - circuits are bound to a single isolation key
    /// None means the circuit is unassigned and can be bound to any key
    pub isolation_key: Option<IsolationKey>,
    _private: (),
}

impl std::fmt::Debug for Circuit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Circuit")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("created_at", &self.created_at)
            .field("last_used", &self.last_used)
            .field("relays", &self.relays)
            .field("internal_circuit", &self.internal_circuit.is_some())
            .field("isolation_key", &self.isolation_key)
            .finish()
    }
}

impl Circuit {
    pub fn new(id: String, internal_circuit: Option<Arc<ClientTunnel>>) -> Self {
        let now = Instant::now();
        Self {
            id,
            status: CircuitStatus::Creating,
            created_at: now,
            last_used: now,
            relays: Vec::new(),
            internal_circuit,
            isolation_key: None,
            _private: (),
        }
    }

    /// Bind this circuit to an isolation key (can only be done once)
    pub fn set_isolation_key(&mut self, key: IsolationKey) {
        if self.isolation_key.is_none() {
            self.isolation_key = Some(key);
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

    /// Begin a TCP stream to the given host and port through this circuit.
    ///
    /// The hostname resolution is performed by the exit relay, so you can
    /// pass hostnames instead of IP addresses.
    pub async fn begin_stream(&self, host: &str, port: u16) -> Result<DataStream> {
        let tunnel = self
            .internal_circuit
            .as_ref()
            .ok_or_else(|| TorError::Internal("No internal circuit available".to_string()))?;

        debug!("Beginning stream to {}:{}", host, port);

        let stream = tunnel
            .begin_stream(host, port, None)
            .await
            .map_err(|e| TorError::Internal(format!("Failed to begin stream: {}", e)))?;

        info!("Stream established to {}:{}", host, port);
        Ok(stream)
    }
}

// Helper to create default circuit parameters
pub fn make_circ_params() -> Result<CircParameters> {
    // 1. Fixed Window Params (Fallback)
    let fixed_window_params = FixedWindowParamsBuilder::default()
        .circ_window_start(1000)
        .circ_window_min(100)
        .circ_window_max(1000)
        .build()
        .map_err(|e| TorError::Internal(format!("Failed to build fixed window params: {}", e)))?;

    // 2. Congestion Window Params
    let cwnd_params = CongestionWindowParamsBuilder::default()
        .cwnd_init(1000)
        .cwnd_inc_pct_ss(Percentage::new(100))
        .cwnd_inc(1)
        .cwnd_inc_rate(1)
        .cwnd_min(100)
        .cwnd_max(1000)
        .sendme_inc(31)
        .build()
        .map_err(|e| TorError::Internal(format!("Failed to build cwnd params: {}", e)))?;

    // 3. Round Trip Estimator Params
    let rtt_params = RoundTripEstimatorParamsBuilder::default()
        .ewma_cwnd_pct(Percentage::new(50))
        .ewma_max(10)
        .ewma_ss_max(10)
        .rtt_reset_pct(Percentage::new(50))
        .build()
        .map_err(|e| TorError::Internal(format!("Failed to build rtt params: {}", e)))?;

    // 4. Congestion Control Params
    let ccontrol = CongestionControlParamsBuilder::default()
        .alg(Algorithm::FixedWindow(fixed_window_params.clone()))
        .fixed_window_params(fixed_window_params)
        .cwnd_params(cwnd_params)
        .rtt_params(rtt_params)
        .build()
        .map_err(|e| TorError::Internal(format!("Failed to build cc params: {}", e)))?;

    // 5. Flow Control Params
    let flow_ctrl = FlowCtrlParameters {
        cc_xoff_client: CellCount::new(500),
        cc_xoff_exit: CellCount::new(500),
        cc_xon_rate: CellCount::new(500),
        cc_xon_change_pct: 25,
        cc_xon_ewma_cnt: 2,
    };

    Ok(CircParameters::new(true, ccontrol, flow_ctrl))
}

pub(crate) struct SimpleTimeoutEstimator;
impl TimeoutEstimator for SimpleTimeoutEstimator {
    fn circuit_build_timeout(&self, _length: usize) -> Duration {
        Duration::from_secs(60)
    }
}

/// Circuit manager for handling multiple circuits
#[derive(Clone)]
pub struct CircuitManager {
    circuits: Arc<RwLock<Vec<Arc<RwLock<Circuit>>>>>,
    relay_manager: Arc<RwLock<RelayManager>>,
    channel: Arc<RwLock<Option<Arc<Channel>>>>,
    prebuild_in_progress: Arc<AtomicBool>,
}

impl CircuitManager {
    pub fn new(
        relay_manager: Arc<RwLock<RelayManager>>,
        channel: Arc<RwLock<Option<Arc<Channel>>>>,
    ) -> Self {
        Self {
            circuits: Arc::new(RwLock::new(Vec::new())),
            relay_manager,
            channel,
            prebuild_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a new circuit, optionally binding it to an isolation key
    ///
    /// If an isolation key is provided, the circuit will be bound to it
    /// BEFORE being added to the circuit list, preventing races where
    /// another request could steal the unassigned circuit.
    pub async fn create_circuit_with_isolation(
        &self,
        isolation_key: Option<IsolationKey>,
    ) -> Result<Arc<RwLock<Circuit>>> {
        let circuit_id = format!("circuit_{}", uuid::Uuid::new_v4());
        info!("Creating new circuit: {}", circuit_id);

        // Log relay manager state
        let relay_manager = self.relay_manager.read().await;
        let total_relays = relay_manager.relays.len();
        drop(relay_manager);
        info!("Relay manager has {} total relays", total_relays);

        if total_relays == 0 {
            error!("No relays available in relay manager - cannot create circuit");
            return Err(TorError::Internal("No relays available".to_string()));
        }

        let channel_guard = self.channel.read().await;
        let channel = channel_guard
            .as_ref()
            .ok_or_else(|| TorError::Internal("Channel not established".to_string()))?
            .clone();
        drop(channel_guard);

        // Create pending tunnel
        let (pending_tunnel, reactor) = channel
            .new_tunnel(Arc::new(SimpleTimeoutEstimator) as Arc<dyn TimeoutEstimator>)
            .await
            .map_err(|e| TorError::Internal(format!("Failed to create pending tunnel: {}", e)))?;

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = reactor.run().await {
                error!("Circuit reactor finished with error: {}", e);
            }
        });

        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            if let Err(e) = reactor.run().await {
                error!("Circuit reactor finished with error: {}", e);
            }
        });

        // First hop (Bridge) - FAST handshake
        let params = make_circ_params()?;
        let tunnel = pending_tunnel
            .create_firsthop_fast(params)
            .await
            .map_err(|e| TorError::Internal(format!("Failed to create first hop: {}", e)))?;

        info!("First hop created (FAST)");

        // Construct Relay object for the bridge from the channel target
        // Note: The channel target might not have all relay info (like ntor key for fast handshake),
        // but we construct a best-effort representation for the circuit info.
        let channel_target = channel.target();
        let bridge_fingerprint = channel_target
            .rsa_identity()
            .map(|id| hex::encode(id.as_bytes()))
            .unwrap_or_else(|| "0000000000000000000000000000000000000000".to_string());

        // We don't have the real address easily accessible in string format from OwnedChanTarget without some work,
        // or the ntor key if it wasn't known.
        // For visual consistency in the circuit list, we create a placeholder if needed.
        // For Snowflake, we use WebRTC so there's no traditional IP address.
        let bridge_relay = Relay::new(
            bridge_fingerprint.clone(),
            "Snowflake".to_string(), // More meaningful name for the proxy
            "0.0.0.0".to_string(),   // Placeholder - will be shown as "Snowflake (WebRTC)" in UI
            0,
            std::collections::HashSet::new(),
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        );

        // Select relays
        let relay_manager = self.relay_manager.read().await;
        info!(
            "Selecting relays from {} available",
            relay_manager.relays.len()
        );

        // Middle
        // Ensure we don't select the bridge as middle
        let middle_criteria =
            crate::relay::selection::middle_relays().without_fingerprint(&bridge_fingerprint);
        debug!("Middle relay criteria: {:?}", middle_criteria);

        let middle = match relay_manager.select_relay(&middle_criteria) {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Failed to select middle relay: {} (available: {})",
                    e,
                    relay_manager.relays.len()
                );
                return Err(e);
            }
        };
        let middle_target = middle.as_circ_target()?;

        info!(
            "Extending to middle: {} (fp={})",
            middle.nickname,
            &middle.fingerprint[..8.min(middle.fingerprint.len())]
        );
        let params = make_circ_params()?;
        tunnel
            .as_single_circ()
            .map_err(|e| {
                TorError::Internal(format!(
                    "Failed to get single circ for middle extend: {}",
                    e
                ))
            })?
            .extend(&middle_target, params)
            .await
            .map_err(|e| TorError::Internal(format!("Failed to extend to middle: {}", e)))?;

        // Exit
        // Ensure we don't select bridge or middle as exit
        let exit_criteria = crate::relay::selection::exit_relays()
            .without_fingerprint(&bridge_fingerprint)
            .without_fingerprint(&middle.fingerprint);
        debug!("Exit relay criteria: {:?}", exit_criteria);

        let exit = match relay_manager.select_relay(&exit_criteria) {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Failed to select exit relay: {} (available: {})",
                    e,
                    relay_manager.relays.len()
                );
                return Err(e);
            }
        };
        let exit_target = exit.as_circ_target()?;

        info!(
            "Extending to exit: {} (fp={})",
            exit.nickname,
            &exit.fingerprint[..8.min(exit.fingerprint.len())]
        );
        let params = make_circ_params()?;
        tunnel
            .as_single_circ()
            .map_err(|e| {
                TorError::Internal(format!("Failed to get single circ for exit extend: {}", e))
            })?
            .extend(&exit_target, params)
            .await
            .map_err(|e| TorError::Internal(format!("Failed to extend to exit: {}", e)))?;

        info!("Circuit established successfully");

        let mut circuit = Circuit::new(circuit_id.clone(), Some(Arc::new(tunnel)));

        // Store relays
        circuit.relays = vec![bridge_relay, middle, exit];
        circuit.status = CircuitStatus::Ready;

        // Bind isolation key BEFORE adding to list to prevent races
        if let Some(key) = isolation_key {
            circuit.set_isolation_key(key);
        }

        info!(
            "Circuit {} created with {} relays",
            circuit_id,
            circuit.relays.len()
        );

        let circuit_arc = Arc::new(RwLock::new(circuit));

        // Add to active circuits
        let mut circuits = self.circuits.write().await;
        circuits.push(circuit_arc.clone());

        Ok(circuit_arc)
    }

    /// Create a new circuit (unassigned, for prebuilding)
    pub async fn create_circuit(&self) -> Result<Arc<RwLock<Circuit>>> {
        self.create_circuit_with_isolation(None).await
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

    /// Get a ready circuit and mark it as used (updates last_used timestamp)
    pub async fn get_ready_circuit_and_mark_used(&self) -> Result<Arc<RwLock<Circuit>>> {
        let circuit = self.get_ready_circuit().await?;
        {
            let mut circ = circuit.write().await;
            circ.update_last_used();
        }
        Ok(circuit)
    }

    /// Get or create a circuit bound to the given isolation key
    ///
    /// This implements stream isolation by ensuring requests to different
    /// domains use different circuits, preventing cross-site correlation.
    pub async fn get_circuit_for_isolation_key(
        &self,
        key: Option<IsolationKey>,
    ) -> Result<Arc<RwLock<Circuit>>> {
        // If no isolation key, fall back to legacy behavior
        let key = match key {
            Some(k) => k,
            None => return self.get_ready_circuit_and_mark_used().await,
        };

        // 1. Look for a ready circuit already bound to this key
        {
            let circuits = self.circuits.read().await;
            for circuit in circuits.iter() {
                let circuit_read = circuit.read().await;
                if circuit_read.is_ready() {
                    if let Some(ref circuit_key) = circuit_read.isolation_key {
                        if circuit_key == &key {
                            debug!(
                                "Reusing circuit {} for isolation key {}",
                                circuit_read.id, key
                            );
                            drop(circuit_read);
                            let mut circuit_write = circuit.write().await;
                            circuit_write.update_last_used();
                            return Ok(circuit.clone());
                        }
                    }
                }
            }
        }

        // 2. Look for a ready unassigned circuit to bind
        // IMPORTANT: We must check AND set under the same write lock to prevent races
        // where two different keys both see the same unassigned circuit
        {
            let circuits = self.circuits.read().await;
            for circuit in circuits.iter() {
                let mut circuit_write = circuit.write().await;
                if circuit_write.is_ready() && circuit_write.isolation_key.is_none() {
                    debug!(
                        "Binding unassigned circuit {} to isolation key {}",
                        circuit_write.id, key
                    );
                    circuit_write.set_isolation_key(key.clone());
                    circuit_write.update_last_used();
                    return Ok(circuit.clone());
                }
            }
        }

        // 3. Check per-key circuit limit before creating new
        {
            let circuits = self.circuits.read().await;
            let circuits_for_key = circuits
                .iter()
                .filter(|c| {
                    if let Ok(circuit_read) = c.try_read() {
                        if let Some(ref circuit_key) = circuit_read.isolation_key {
                            return circuit_key == &key
                                && !circuit_read.is_failed()
                                && !circuit_read.is_closed();
                        }
                    }
                    false
                })
                .count();

            if circuits_for_key >= MAX_CIRCUITS_PER_ISOLATION_KEY {
                // Try to reuse any circuit for this key (even if not ready yet)
                for circuit in circuits.iter() {
                    let circuit_read = circuit.read().await;
                    if let Some(ref circuit_key) = circuit_read.isolation_key {
                        if circuit_key == &key
                            && !circuit_read.is_failed()
                            && !circuit_read.is_closed()
                        {
                            debug!(
                                "At per-key limit, reusing circuit {} for {}",
                                circuit_read.id, key
                            );
                            drop(circuit_read);
                            let mut circuit_write = circuit.write().await;
                            circuit_write.update_last_used();
                            return Ok(circuit.clone());
                        }
                    }
                }
            }
        }

        // 4. Create a new circuit already bound to this key
        // We pass the key to create_circuit_with_isolation so it's bound
        // BEFORE the circuit is added to the list, preventing races
        info!("Creating new circuit for isolation key {}", key);
        let circuit = self.create_circuit_with_isolation(Some(key)).await?;
        {
            let mut circuit_write = circuit.write().await;
            circuit_write.update_last_used();
        }
        Ok(circuit)
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

    /// Synchronous version of update_relays for use when we already have a mutable reference
    pub fn update_relay_list(&mut self, new_relays: Vec<crate::relay::Relay>) {
        if let Ok(mut relay_manager) = self.relay_manager.try_write() {
            relay_manager.update_relays(new_relays);
        }
    }

    /// Get relay information from the most recent ready circuit
    pub async fn get_circuit_relays(&self) -> Option<Vec<CircuitRelayInfo>> {
        let circuits = self.circuits.read().await;
        // Iterate in reverse to get the most recently created circuit
        for circuit in circuits.iter().rev() {
            let circuit_read = circuit.read().await;
            if circuit_read.is_ready() && !circuit_read.relays.is_empty() {
                return Some(
                    circuit_read
                        .relays
                        .iter()
                        .enumerate()
                        .map(|(idx, relay)| {
                            let role = match idx {
                                0 => "Bridge",
                                1 => "Middle",
                                2 => "Exit",
                                _ => "Unknown",
                            };
                            // For Snowflake bridges, the address is typically 0.0.0.0 since
                            // we connect via WebRTC - show something more meaningful
                            let address = if idx == 0
                                && (relay.address == "0.0.0.0"
                                    || relay.address.starts_with("0.0.0.0:"))
                            {
                                "Snowflake (WebRTC)".to_string()
                            } else {
                                relay.address.clone()
                            };
                            CircuitRelayInfo {
                                role: role.to_string(),
                                nickname: relay.nickname.clone(),
                                address,
                                fingerprint: relay.fingerprint.chars().take(16).collect(),
                            }
                        })
                        .collect(),
                );
            }
        }
        None
    }

    /// Preemptively build a spare circuit if conditions are met
    ///
    /// This ensures we have a fresh circuit ready before existing ones expire.
    /// Called periodically or after successful requests.
    pub async fn maybe_prebuild_circuit(&self, max_circuits: usize, age_threshold: Duration) {
        let status = self.get_circuit_status().await;

        // Only prebuild if we have at least 1 ready circuit
        if status.ready_circuits == 0 {
            debug!("Skipping prebuild: no ready circuits");
            return;
        }

        // Limit total circuits
        if status.total_circuits >= max_circuits {
            debug!("Skipping prebuild: at max circuits ({})", max_circuits);
            return;
        }

        // Only if circuits are getting old
        if status.average_circuit_age < age_threshold {
            debug!(
                "Skipping prebuild: circuits not old enough (avg {:?} < threshold {:?})",
                status.average_circuit_age, age_threshold
            );
            return;
        }

        // Try to acquire the prebuild slot (prevents race condition where multiple
        // concurrent requests all trigger prebuilds)
        if self.prebuild_in_progress.swap(true, Ordering::SeqCst) {
            debug!("Skipping prebuild: another prebuild is already in progress");
            return;
        }

        info!(
            "Prebuilding spare circuit (avg age: {:?}, threshold: {:?})",
            status.average_circuit_age, age_threshold
        );

        // Clone self for the spawned task
        let circuit_manager = self.clone();
        let prebuild_flag = self.prebuild_in_progress.clone();

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = circuit_manager.create_circuit().await;
            prebuild_flag.store(false, Ordering::SeqCst);
            match result {
                Ok(circuit) => {
                    let circuit_info = circuit.read().await;
                    info!("Prebuilt circuit {} ready", circuit_info.id);
                }
                Err(e) => {
                    error!("Failed to prebuild circuit: {}", e);
                }
            }
        });

        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            let result = circuit_manager.create_circuit().await;
            prebuild_flag.store(false, Ordering::SeqCst);
            match result {
                Ok(circuit) => {
                    let circuit_info = circuit.read().await;
                    info!("Prebuilt circuit {} ready", circuit_info.id);
                }
                Err(e) => {
                    error!("Failed to prebuild circuit: {}", e);
                }
            }
        });
    }

    /// Clean up failed and old circuits
    pub async fn cleanup_circuits(&self) -> Result<()> {
        let mut circuits = self.circuits.write().await;
        let max_age = Duration::from_secs(60 * 60); // 1 hour
        let max_idle = Duration::from_secs(60 * 10); // 10 minutes

        // Gather circuit info asynchronously first
        let mut to_remove = Vec::new();
        let total_count = circuits.len();
        let mut remaining = total_count;

        for (idx, circuit) in circuits.iter().enumerate() {
            let circuit_read = circuit.read().await;

            // Remove failed circuits
            if circuit_read.is_failed() {
                info!("Removing failed circuit: {}", circuit_read.id);
                to_remove.push(idx);
                remaining -= 1;
                continue;
            }

            // Remove very old circuits
            if circuit_read.age() > max_age {
                info!(
                    "Removing old circuit: {} (age: {:?})",
                    circuit_read.id,
                    circuit_read.age()
                );
                to_remove.push(idx);
                remaining -= 1;
                continue;
            }

            // Remove idle circuits (but keep at least one)
            if circuit_read.time_since_last_use() > max_idle && remaining > 1 {
                info!(
                    "Removing idle circuit: {} (idle: {:?})",
                    circuit_read.id,
                    circuit_read.time_since_last_use()
                );
                to_remove.push(idx);
                remaining -= 1;
            }
        }

        // Remove in reverse order to preserve indices
        for idx in to_remove.into_iter().rev() {
            circuits.remove(idx);
        }

        Ok(())
    }
}

/// Circuit relay information for display
#[derive(Debug, Clone)]
pub struct CircuitRelayInfo {
    pub role: String,
    pub nickname: String,
    pub address: String,
    pub fingerprint: String,
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
    use crate::relay::{flags, Relay};

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

    #[tokio::test]
    async fn test_circuit_creation() {
        let relays = vec![
            create_test_relay("relay1", vec![flags::FAST, flags::STABLE, flags::GUARD]),
            create_test_relay("relay2", vec![flags::FAST, flags::STABLE, flags::V2DIR]),
            create_test_relay("relay3", vec![flags::FAST, flags::STABLE, flags::EXIT]),
        ];

        let relay_manager = Arc::new(RwLock::new(RelayManager::new(relays)));
        // Create empty channel for testing
        let channel = Arc::new(RwLock::new(None));
        let circuit_manager = CircuitManager::new(relay_manager, channel);

        // This will fail because channel is not established
        let result = circuit_manager.create_circuit().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_circuit_status() {
        let mut circuit = Circuit::new("test_circuit".to_string(), None);

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

    #[test]
    fn test_circuit_isolation_key_binding() {
        let mut circuit = Circuit::new("test_circuit".to_string(), None);
        assert!(circuit.isolation_key.is_none());

        let key1 = IsolationKey::from_string("example.com");
        circuit.set_isolation_key(key1.clone());
        assert_eq!(circuit.isolation_key, Some(key1.clone()));

        // Trying to set a different key should be ignored (circuits bind once)
        let key2 = IsolationKey::from_string("other.com");
        circuit.set_isolation_key(key2);
        assert_eq!(circuit.isolation_key, Some(key1));
    }

    #[test]
    fn test_circuit_new_has_no_isolation_key() {
        let circuit = Circuit::new("test".to_string(), None);
        assert!(circuit.isolation_key.is_none());
    }
}
