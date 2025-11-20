//! Directory management and consensus fetching

use crate::error::{Result, TorError};
use crate::relay::{Relay, RelayManager};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use tor_proto::channel::Channel;
use tor_proto::client::circuit::TimeoutEstimator;
use futures::{AsyncReadExt, AsyncWriteExt};

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
        // Note: This assumes Microdescriptor consensus
        // We need to handle the parsing carefully as tor-netdoc is strict
        
        // specific parsing logic to be added
        // For now, just logging
        debug!("Consensus body preview: {:.100}...", consensus_str);
        
        Ok(0)
    }
}
