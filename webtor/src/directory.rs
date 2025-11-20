//! Directory management and consensus fetching

use crate::error::{Result, TorError};
use crate::relay::{Relay, RelayManager};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use tor_proto::channel::Channel;
use tor_proto::client::circuit::TimeoutEstimator;
use futures::{AsyncReadExt, AsyncWriteExt};
use tor_netdoc::doc::netstatus::MdConsensus;
use tor_checkable::Timebound;

/// Directory manager for handling network documents
pub struct DirectoryManager {
    pub relay_manager: Arc<RwLock<RelayManager>>,
}

impl DirectoryManager {
    pub fn new(relay_manager: Arc<RwLock<RelayManager>>) -> Self {
        Self { relay_manager }
    }

    /// Fetch consensus from the directory cache (bridge)
    pub async fn fetch_consensus(&self, channel: Arc<Channel>) -> Result<()> {
        info!("Fetching consensus from bridge...");

        // 1. Create 1-hop circuit (tunnel)
        let (pending_tunnel, reactor) = channel.new_tunnel(
            Arc::new(crate::circuit::SimpleTimeoutEstimator) as Arc<dyn TimeoutEstimator>
        )
        .await
        .map_err(|e| TorError::Internal(format!("Failed to create pending tunnel for dir: {}", e)))?;

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = reactor.run().await {
                error!("Dir circuit reactor finished with error: {}", e);
            }
        });
        
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            if let Err(e) = reactor.run().await {
                error!("Dir circuit reactor finished with error: {}", e);
            }
        });

        let params = crate::circuit::make_circ_params()?;
        let tunnel = pending_tunnel.create_firsthop_fast(params)
            .await
            .map_err(|e| TorError::Internal(format!("Failed to create dir circuit: {}", e)))?;
            
        // 2. Open directory stream
        // Note: We need to wrap tunnel in Arc because begin_dir_stream expects &Arc<Self>
        let tunnel_arc = Arc::new(tunnel);
        let mut stream = tunnel_arc.begin_dir_stream()
            .await
            .map_err(|e| TorError::Internal(format!("Failed to begin dir stream: {}", e)))?;
            
        // 3. Send HTTP GET request for microdescriptor consensus
        // TODO: Support compression (.z)
        let path = "/tor/status-vote/current/consensus-microdesc";
        let request = format!(
            "GET {} HTTP/1.0\r\n\
             Host: directory\r\n\
             Connection: close\r\n\
             \r\n",
            path
        );
        
        stream.write_all(request.as_bytes()).await
            .map_err(|e| TorError::Network(format!("Failed to write dir request: {}", e)))?;
        stream.flush().await
            .map_err(|e| TorError::Network(format!("Failed to flush dir request: {}", e)))?;
            
        // 4. Read response
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await
            .map_err(|e| TorError::Network(format!("Failed to read dir response: {}", e)))?;
            
        info!("Received consensus response: {} bytes", response.len());
        
        // 5. Process response (skip headers for now)
        // Simple header skipping
        let body_start = response.windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| i + 4)
            .unwrap_or(0);
            
        let body = &response[body_start..];
        let body_str = String::from_utf8_lossy(body);
        
        self.process_consensus(&body_str).await?;
        
        Ok(())
    }
    
    /// Parse a consensus document and update the relay manager
    pub async fn process_consensus(&self, consensus_str: &str) -> Result<usize> {
        info!("Processing consensus document (length: {})", consensus_str.len());
        
        // Parse the consensus
        // We assume it's a Microdescriptor consensus
        let (_, _, unvalidated) = MdConsensus::parse(consensus_str)
            .map_err(|e| TorError::serialization(format!("Failed to parse consensus: {}", e)))?;
            
        // Check timeliness (or assume timely for bootstrapping if needed)
        // For now, we just check against system time.
        // If the consensus is not valid, we log a warning but proceed if possible
        // using dangerously_assume_timely() if check fails, for testing/bootstrap.
        
        let consensus = match unvalidated.clone().check_valid_at(&SystemTime::now()) {
            Ok(c) => c,
            Err(e) => {
                warn!("Consensus timeliness check failed: {}. Proceeding anyway for bootstrapping.", e);
                unvalidated.dangerously_assume_timely()
            }
        };
            
        // Access the inner consensus object
        // UnvalidatedConsensus wraps the actual consensus document
        let inner_consensus = &consensus.consensus;
        
        info!("Parsed consensus with {} relays", inner_consensus.relays().len());
        
        let mut relays = Vec::new();
        
        for router in inner_consensus.relays().iter() {
            // Extract basic info
            let nickname = router.nickname().to_string();
            let fingerprint = hex::encode(router.rsa_identity().as_bytes());
            
            // We need at least one address
            let address = if let Some(addr) = router.addrs().next() {
                addr.ip().to_string()
            } else {
                continue;
            };
            
            let or_port = router.addrs().next().map(|a| a.port()).unwrap_or(0);
            
            // Flags
            let mut flags = std::collections::HashSet::new();
            if router.is_flagged_fast() { flags.insert("Fast".to_string()); }
            if router.is_flagged_stable() { flags.insert("Stable".to_string()); }
            // Valid/Authority might not be exposed on MdRouterStatus or named differently
            // if router.is_flagged_valid() { flags.insert("Valid".to_string()); }
            if router.is_flagged_guard() { flags.insert("Guard".to_string()); }
            if router.is_flagged_exit() { flags.insert("Exit".to_string()); }
            if router.is_flagged_bad_exit() { flags.insert("BadExit".to_string()); }
            // if router.is_flagged_authority() { flags.insert("Authority".to_string()); }
            if router.is_flagged_hsdir() { flags.insert("HSDir".to_string()); }
            if router.is_flagged_v2dir() { flags.insert("V2Dir".to_string()); }
            
            // We need Ed25519 identity and ntor key for circuit creation
            // Note: MdConsensus routers might not have all keys directly accessible 
            // in the same way as full descriptors, but let's try to extract what we can.
            
            let ntor_onion_key = hex::encode(router.ntor_onion_key().as_bytes());

            let mut relay = Relay::new(
                fingerprint,
                nickname,
                address,
                or_port,
                flags,
                ntor_onion_key,
            );

            if let Some(ed_id) = router.ed_identity() {
                relay.ed25519_identity = Some(hex::encode(ed_id.as_bytes()));
            }

            relay.consensus_weight = router.weight().weight() as u32;
            if let Some(version) = router.version() {
                relay.version = version.to_string();
            }
            relay.microdescriptor_hash = hex::encode(router.md_digest());
            
            relays.push(relay);
        }
        
        let count = relays.len();
        
        {
            let mut manager = self.relay_manager.write().await;
            manager.update_relays(relays);
        }
        
        info!("Updated RelayManager with {} relays", count);
        
        Ok(count)
    }
}
