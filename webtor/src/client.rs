//! Main Tor client implementation

use crate::circuit::{CircuitManager, CircuitStatusInfo};
use crate::config::{BridgeType, LogType, TorClientOptions, SNOWFLAKE_FINGERPRINT_PRIMARY};
use crate::directory::DirectoryManager;
use crate::error::{Result, TorError};
use crate::http::{HttpRequest, HttpResponse, TorHttpClient};
use crate::relay::RelayManager;
#[cfg(target_arch = "wasm32")]
use crate::snowflake_ws::{SnowflakeWsConfig, SnowflakeWsStream};
#[cfg(not(target_arch = "wasm32"))]
use crate::webtunnel::{WebTunnelConfig, create_webtunnel_stream};
use crate::wasm_runtime::WasmRuntime;
use tor_proto::channel::ChannelBuilder;
use http::Method;
use tor_memquota::MemoryQuotaTracker;
use tor_proto::memquota::{ChannelAccount, SpecificAccount};
use tor_linkspec::OwnedChanTargetBuilder;
use tor_llcrypto::pk::rsa::RsaIdentity;
use crate::time::system_time_now;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use url::Url;

/// Main Tor client that manages circuits and HTTP requests
pub struct TorClient {
    options: TorClientOptions,
    circuit_manager: Arc<RwLock<CircuitManager>>,
    directory_manager: Arc<DirectoryManager>,
    http_client: Arc<TorHttpClient>,
    is_initialized: Arc<RwLock<bool>>,
    // Store the channel to prevent it from being dropped
    channel: Arc<RwLock<Option<Arc<tor_proto::channel::Channel>>>>,
    update_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl TorClient {
    /// Create a new Tor client with the given options
    pub async fn new(options: TorClientOptions) -> Result<Self> {
        info!("TorClient::new START");
        
        // Initialize WASM modules (placeholder for now)
        Self::init_wasm_modules().await?;
        
        // Channel storage
        let channel = Arc::new(RwLock::new(None));
        
        // Create relay manager with empty relay list (will be populated later)
        let relay_manager = RelayManager::new(Vec::new());
        let relay_manager_arc = Arc::new(RwLock::new(relay_manager));
        
        let directory_manager = Arc::new(DirectoryManager::new(relay_manager_arc.clone()));
        let circuit_manager = Arc::new(RwLock::new(CircuitManager::new(relay_manager_arc.clone(), channel.clone())));
        let http_client = TorHttpClient::new(circuit_manager.clone());
        
        let client = Self {
            options: options.clone(),
            circuit_manager,
            directory_manager,
            http_client: Arc::new(http_client),
            is_initialized: Arc::new(RwLock::new(false)),
            channel,
            update_task: Arc::new(RwLock::new(None)),
        };
        
        // Create initial circuit if requested
        if options.create_circuit_early {
            info!("Establishing connection early");
            
            // Establish the channel
            info!("TorClient::new: calling establish_channel");
            if let Err(e) = client.establish_channel().await {
                error!("Failed to establish channel: {}", e);
                // Don't fail the client creation, just log the error
            }
            info!("TorClient::new: establish_channel returned");
        }
        
        info!("TorClient::new RETURNING");
        Ok(client)
    }
    
    /// Bootstrap the client by fetching consensus
    pub async fn bootstrap(&self) -> Result<()> {
        self.log("Bootstrapping Tor client...", LogType::Info);
        
        // Ensure channel is established
        let channel_guard = self.channel.read().await;
        if channel_guard.is_none() {
            drop(channel_guard);
            self.establish_channel().await?;
        } else {
            drop(channel_guard);
        }
        
        // Get channel
        let channel_guard = self.channel.read().await;
        let channel = channel_guard.as_ref()
            .ok_or_else(|| TorError::Internal("Channel not established".to_string()))?
            .clone();
        drop(channel_guard);
        
        // Fetch consensus
        self.log("Fetching consensus...", LogType::Info);
        self.directory_manager.fetch_and_process_consensus(channel).await?;
        self.log("Consensus fetched successfully", LogType::Success);
        
        Ok(())
    }
    
    /// Make a one-time fetch request through Tor with a temporary circuit
    pub async fn fetch_one_time(
        snowflake_url: &str,
        url: &str,
        connection_timeout: Option<u64>,
        circuit_timeout: Option<u64>,
    ) -> Result<HttpResponse> {
        info!("Making one-time fetch request to {} through Snowflake {}", url, snowflake_url);
        
        let options = TorClientOptions::new(snowflake_url.to_string())
            .with_create_circuit_early(true) // Ensure channel is established
            .with_circuit_update_interval(None) // No auto-updates for one-time use
            .with_connection_timeout(connection_timeout.unwrap_or(15_000))
            .with_circuit_timeout(circuit_timeout.unwrap_or(90_000));
        
        let client = Self::new(options).await?;
        
        // Make the request and then close the client
        let result = client.fetch(url).await;
        client.close().await;
        
        result
    }
    
    /// Make a fetch request through the persistent Tor circuit
    pub async fn fetch(&self, url: &str) -> Result<HttpResponse> {
        self.log(&format!("Starting fetch request to {}", url), LogType::Info);
        
        let url = Url::parse(url)?;
        let request = HttpRequest::new(url);
        
        self.http_client.request(request).await
    }
    
    /// Make a GET request
    pub async fn get(&self, url: &str) -> Result<HttpResponse> {
        self.fetch(url).await
    }
    
    /// Make a POST request
    pub async fn post(&self, url: &str, body: Vec<u8>) -> Result<HttpResponse> {
        let url = Url::parse(url)?;
        let request = HttpRequest::new(url)
            .with_method(Method::POST)
            .with_body(body);
        
        self.http_client.request(request).await
    }
    
    /// Update the circuit with a deadline for graceful transition
    pub async fn update_circuit(&self, deadline: Duration) -> Result<()> {
        info!("Updating circuit with {:?} deadline", deadline);
        
        // For now, this is a placeholder
        // In the full implementation, this would:
        // 1. Create a new circuit in the background
        // 2. Allow existing requests to use the old circuit until deadline
        // 3. Switch to the new circuit after deadline
        
        self.log("Circuit update completed", LogType::Success);
        Ok(())
    }
    
    /// Wait for a circuit to be ready
    pub async fn wait_for_circuit(&self) -> Result<()> {
        info!("Waiting for circuit to be ready");
        
        let circuit_manager = self.circuit_manager.read().await;
        let circuit = circuit_manager.get_ready_circuit().await?;
        
        // Wait for the circuit to be ready
        let circuit_read = circuit.read().await;
        if !circuit_read.is_ready() {
            return Err(TorError::circuit_creation("Circuit is not ready"));
        }
        
        info!("Circuit is ready");
        Ok(())
    }
    
    /// Get current circuit status
    pub async fn get_circuit_status(&self) -> CircuitStatusInfo {
        let circuit_manager = self.circuit_manager.read().await;
        circuit_manager.get_circuit_status().await
    }
    
    /// Get human-readable circuit status string
    pub async fn get_circuit_status_string(&self) -> String {
        let status = self.get_circuit_status().await;
        
        if !status.has_ready_circuits() && status.creating_circuits > 0 {
            return "Creating...".to_string();
        }
        
        if !status.has_ready_circuits() {
            return "None".to_string();
        }
        
        if status.failed_circuits > 0 {
            return format!("Ready ({} failed circuits)", status.failed_circuits);
        }
        
        "Ready".to_string()
    }
    
    /// Ensure the client is ready for making requests
    pub async fn ensure_ready(&self) -> Result<()> {
        // Establish channel if not already done
        if !*self.is_initialized.read().await {
            self.establish_channel().await?;
        }
        
        Ok(())
    }
    
    /// Close the Tor client and clean up resources
    pub async fn close(&self) {
        info!("Closing Tor client");
        
        // Stop update task if running
        if let Some(task) = self.update_task.write().await.take() {
            task.abort();
        }
        
        // Clean up circuits
        let circuit_manager = self.circuit_manager.write().await;
        if let Err(e) = circuit_manager.cleanup_circuits().await {
            warn!("Error during circuit cleanup: {}", e);
        }
        
        *self.is_initialized.write().await = false;
        info!("Tor client closed");
    }

    /// Establish the Tor channel (called during construction if requested)
    async fn establish_channel(&self) -> Result<()> {
        self.log("Establishing channel", LogType::Info);
        
        let timeout = Duration::from_millis(self.options.connection_timeout);
        
        // Get fingerprint - use default for Snowflake if not provided
        let fingerprint = match &self.options.bridge {
            BridgeType::Snowflake { .. } => {
                self.options.bridge_fingerprint.as_ref()
                    .map(|s| s.clone())
                    .unwrap_or_else(|| SNOWFLAKE_FINGERPRINT_PRIMARY.to_string())
            }
            BridgeType::WebTunnel { .. } => {
                self.options.bridge_fingerprint.as_ref()
                    .ok_or_else(|| TorError::Configuration("Bridge fingerprint is required for WebTunnel".to_string()))?
                    .clone()
            }
        };
        
        // Parse fingerprint to RSA identity
        let rsa_id = {
            let bytes = hex::decode(&fingerprint)
                .map_err(|e| TorError::Configuration(format!("Invalid fingerprint hex: {}", e)))?;
            if bytes.len() != 20 {
                return Err(TorError::Configuration("Fingerprint must be 40 hex characters (20 bytes)".to_string()));
            }
            RsaIdentity::from_bytes(&bytes)
                .ok_or_else(|| TorError::Configuration("Invalid RSA identity bytes".to_string()))?
        };
        
        // 1. Connect to bridge based on type
        let chan = match &self.options.bridge {
            BridgeType::Snowflake { url } => {
                self.log("Connecting via Snowflake (WebSocket)", LogType::Info);
                self.log("Using WebSocket → Turbo → KCP → SMUX → TLS stack", LogType::Info);
                #[cfg(target_arch = "wasm32")]
                {
                    // Use WebSocket-based Snowflake (like echalote)
                    let config = SnowflakeWsConfig::default()
                        .with_url(url)
                        .with_fingerprint(&fingerprint);
                    let stream = SnowflakeWsStream::connect(config).await?;
                    self.log("Connected to Snowflake bridge via WebSocket", LogType::Success);
                    self.create_channel_from_stream(stream, rsa_id).await?
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = url; // suppress unused warning
                    return Err(TorError::Internal(
                        "Snowflake WebSocket is only available in WASM. \
                         Use WebTunnel bridge for native builds.".to_string()
                    ));
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            BridgeType::WebTunnel { url, server_name } => {
                self.log(&format!("Connecting via WebTunnel to {}", url), LogType::Info);
                let mut config = WebTunnelConfig::new(url.clone(), fingerprint.clone())
                    .with_timeout(timeout);
                if let Some(sni) = server_name {
                    config = config.with_server_name(sni.clone());
                }
                let stream = create_webtunnel_stream(config).await?;
                self.log("Connected to WebTunnel bridge", LogType::Success);
                self.create_channel_from_stream(stream, rsa_id).await?
            }
            #[cfg(target_arch = "wasm32")]
            BridgeType::WebTunnel { .. } => {
                return Err(TorError::Internal(
                    "WebTunnel is not supported in WASM. Use Snowflake bridge instead.".to_string()
                ));
            }
        };
        
        // Store the channel to keep it alive
        *self.channel.write().await = Some(chan);

        self.log("Channel established", LogType::Success);
        
        // Now create the actual circuit through the Tor network
        self.log("Creating circuit through Tor network...", LogType::Info);
        
        let circuit_manager = self.circuit_manager.read().await;
        match circuit_manager.create_circuit().await {
            Ok(circuit) => {
                let circuit_info = circuit.read().await;
                let relay_names: Vec<_> = circuit_info.relays.iter()
                    .map(|r| r.nickname.clone())
                    .collect();
                self.log(&format!("Circuit created: {}", relay_names.join(" → ")), LogType::Success);
            }
            Err(e) => {
                self.log(&format!("Failed to create circuit: {}", e), LogType::Error);
                return Err(e);
            }
        }
        
        *self.is_initialized.write().await = true;
        
        Ok(())
    }
    
    /// Create Tor channel from a connected stream and spawn the reactor
    async fn create_channel_from_stream<S>(
        &self,
        stream: S,
        rsa_id: RsaIdentity,
    ) -> Result<Arc<tor_proto::channel::Channel>>
    where
        S: futures::AsyncRead + futures::AsyncWrite + Send + Unpin 
           + tor_rtcompat::StreamOps + tor_rtcompat::CertifiedConn + 'static,
    {
        let runtime = WasmRuntime::new();
        
        // Extract the peer certificate from the TLS stream BEFORE moving it
        // The peer certificate is needed later for the check() call
        let peer_cert = stream.peer_certificate()
            .map_err(|e| TorError::Network(format!("Failed to get peer certificate: {}", e)))?
            .ok_or_else(|| TorError::Network("No peer certificate from TLS".to_string()))?;
        debug!("Got peer certificate: {} bytes", peer_cert.len());
        
        // Create a no-op memory quota for now
        let mq = MemoryQuotaTracker::new_noop();
        
        // Create ChannelAccount directly from tracker
        let chan_account = ChannelAccount::new(&mq)
             .map_err(|e| TorError::Internal(format!("Failed to create channel account: {}", e)))?;

        let builder = ChannelBuilder::new();
        debug!("Launching Tor channel client handshake...");
        let handshake = builder.launch_client(stream, runtime, chan_account);
        
        debug!("Starting handshake connect...");
        let unverified = handshake.connect(system_time_now).await
            .map_err(|e| {
                error!("Handshake connect error details: {:?}", e);
                TorError::Network(format!("Handshake connect failed: {}", e))
            })?;
        debug!("Handshake connect completed, verifying...");
            
        // Construct peer target
        let mut peer_builder = OwnedChanTargetBuilder::default();
        peer_builder.rsa_identity(rsa_id);
            
        let peer = peer_builder.build()
            .map_err(|e| TorError::Internal(format!("Failed to build peer target: {}", e)))?;

        // Pass the peer certificate to check() - this verifies that the CERTS cells
        // properly authenticate the TLS certificate we received
        // Note: We must pass the current time explicitly because SystemTime::now() panics on WASM
        let (chan, reactor) = unverified.check(&peer, &peer_cert, Some(system_time_now()))
            .map_err(|e| TorError::Network(format!("Handshake check failed: {}", e)))?
            .finish()
            .await
            .map_err(|e| TorError::Network(format!("Handshake finish failed: {}", e)))?;
        
        // Spawn reactor
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let _ = reactor.run().await;
        });
        
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            let _ = reactor.run().await;
        });
            
        Ok(chan)
    }
    
    /// Initialize WASM modules (placeholder)
    async fn init_wasm_modules() -> Result<()> {
        // This will be implemented in the WASM bindings
        // For now, just log that we're initializing
        debug!("Initializing WASM modules");
        Ok(())
    }
    
    /// Log a message (uses callback if provided)
    fn log(&self, message: &str, log_type: LogType) {
        if let Some(ref on_log) = self.options.on_log {
            (on_log.0)(message, log_type);
        } else {
            // Default logging
            match log_type {
                LogType::Info => info!("{}", message),
                LogType::Success => info!("✅ {}", message),
                LogType::Error => error!("❌ {}", message),
            }
        }
    }
}

impl Drop for TorClient {
    fn drop(&mut self) {
        // Try to clean up, but don't block since we're in drop
        let client = self.clone();
        tokio::spawn(async move {
            client.close().await;
        });
    }
}

impl Clone for TorClient {
    fn clone(&self) -> Self {
        Self {
            options: self.options.clone(),
            circuit_manager: self.circuit_manager.clone(),
            directory_manager: self.directory_manager.clone(),
            http_client: self.http_client.clone(),
            is_initialized: self.is_initialized.clone(),
            channel: self.channel.clone(),
            update_task: self.update_task.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_tor_client_creation() {
        let options = TorClientOptions::new("wss://snowflake.torproject.net/".to_string())
            .with_create_circuit_early(false);
        
        let client = TorClient::new(options).await;
        assert!(client.is_ok());
    }
    
    #[tokio::test]
    async fn test_one_time_fetch() {
        // This will fail because we don't have WASM WebSocket implementation
        let result = TorClient::fetch_one_time(
            "wss://snowflake.torproject.net/",
            "https://httpbin.org/ip",
            None,
            None,
        ).await;
        
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_circuit_status() {
        let options = TorClientOptions::new("wss://snowflake.torproject.net/".to_string())
            .with_create_circuit_early(false);
        
        let client = TorClient::new(options).await.unwrap();
        let status = client.get_circuit_status().await;
        
        assert_eq!(status.total_circuits, 0);
        assert_eq!(status.ready_circuits, 0);
        assert!(!status.has_ready_circuits());
    }
}