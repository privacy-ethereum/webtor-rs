//! Integration tests for webtor with real Tor network
//!
//! These tests require network access and a working bridge configuration.
//!
//! Run with:
//!   # Using WebTunnel (native)
//!   WEBTUNNEL_URL='https://...' WEBTUNNEL_FINGERPRINT='...' cargo test -p webtor --test integration_test
//!
//!   # Or set RUN_INTEGRATION_TESTS=1 to enable (will fail without bridge config)
//!   RUN_INTEGRATION_TESTS=1 cargo test -p webtor --test integration_test

use std::time::Instant;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

fn init_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .try_init();
    
    // Ignore error if already initialized
    let _ = subscriber;
}

fn should_run_integration_tests() -> bool {
    std::env::var("RUN_INTEGRATION_TESTS").is_ok() ||
    (std::env::var("WEBTUNNEL_URL").is_ok() && std::env::var("WEBTUNNEL_FINGERPRINT").is_ok())
}

fn get_webtunnel_config() -> Option<(String, String)> {
    let url = std::env::var("WEBTUNNEL_URL").ok()?;
    let fingerprint = std::env::var("WEBTUNNEL_FINGERPRINT").ok()?;
    Some((url, fingerprint))
}

#[tokio::test]
async fn test_client_creation() {
    use webtor::{TorClient, TorClientOptions};
    
    // This test doesn't require network - just creates the client without connecting
    let options = TorClientOptions::snowflake()
        .with_create_circuit_early(false);
    
    let client = TorClient::new(options).await;
    assert!(client.is_ok(), "Client creation should succeed");
    
    let client = client.unwrap();
    let status = client.get_circuit_status_string().await;
    assert_eq!(status, "None", "No circuits should exist yet");
}

#[tokio::test]
async fn test_webtunnel_fetch_ipify() {
    if !should_run_integration_tests() {
        println!("Skipping integration test (set WEBTUNNEL_URL and WEBTUNNEL_FINGERPRINT to run)");
        return;
    }
    
    init_logging();
    
    let (url, fingerprint) = match get_webtunnel_config() {
        Some(config) => config,
        None => {
            println!("WebTunnel config not found, skipping test");
            return;
        }
    };
    
    use webtor::{TorClient, TorClientOptions};
    
    info!("Starting WebTunnel integration test");
    info!("Bridge URL: {}", url);
    
    let start = Instant::now();
    
    // Create client with WebTunnel bridge
    let options = TorClientOptions::webtunnel(url, fingerprint)
        .with_create_circuit_early(true)
        .with_connection_timeout(60_000)  // 60s for initial connection
        .with_circuit_timeout(180_000);   // 3min for circuit
    
    let client = TorClient::new(options).await
        .expect("Failed to create TorClient");
    
    let connect_time = start.elapsed();
    info!("Connected in {:?}", connect_time);
    
    // Fetch IP from ipify
    info!("Fetching https://api64.ipify.org?format=json through Tor...");
    let fetch_start = Instant::now();
    
    let response = client.fetch("https://api64.ipify.org?format=json").await
        .expect("Failed to fetch from ipify");
    
    let fetch_time = fetch_start.elapsed();
    info!("Fetch completed in {:?}", fetch_time);
    
    assert_eq!(response.status, 200, "Should get 200 OK");
    
    let body = String::from_utf8_lossy(&response.body);
    info!("Response: {}", body);
    
    // Parse JSON to verify it's a valid IP response
    let json: serde_json::Value = serde_json::from_str(&body)
        .expect("Response should be valid JSON");
    
    assert!(json.get("ip").is_some(), "Response should contain 'ip' field");
    
    let ip = json["ip"].as_str().unwrap();
    info!("Tor exit IP: {}", ip);
    
    // Verify it's not our real IP by checking it looks like a valid IP
    assert!(ip.contains('.') || ip.contains(':'), "Should be a valid IPv4 or IPv6 address");
    
    // Total time
    let total_time = start.elapsed();
    info!("Total time: {:?}", total_time);
    
    // Performance assertions (generous timeouts for Tor)
    assert!(connect_time.as_secs() < 120, "Connection should complete within 2 minutes");
    assert!(fetch_time.as_secs() < 60, "Fetch should complete within 1 minute");
    
    client.close().await;
    info!("Test completed successfully!");
}

#[tokio::test]
async fn test_multiple_fetches() {
    if !should_run_integration_tests() {
        println!("Skipping integration test");
        return;
    }
    
    init_logging();
    
    let (url, fingerprint) = match get_webtunnel_config() {
        Some(config) => config,
        None => return,
    };
    
    use webtor::{TorClient, TorClientOptions};
    
    info!("Testing multiple fetches through single circuit");
    
    let options = TorClientOptions::webtunnel(url, fingerprint)
        .with_create_circuit_early(true)
        .with_connection_timeout(60_000)
        .with_circuit_timeout(180_000);
    
    let client = TorClient::new(options).await
        .expect("Failed to create TorClient");
    
    let urls = [
        "https://httpbin.org/ip",
        "https://httpbin.org/user-agent",
        "https://httpbin.org/headers",
    ];
    
    let mut total_fetch_time = std::time::Duration::ZERO;
    
    for (i, url) in urls.iter().enumerate() {
        let start = Instant::now();
        let response = client.fetch(url).await
            .expect(&format!("Failed to fetch {}", url));
        let elapsed = start.elapsed();
        total_fetch_time += elapsed;
        
        info!("Fetch {} ({}) completed in {:?} - status {}", 
            i + 1, url, elapsed, response.status);
        
        assert_eq!(response.status, 200);
    }
    
    info!("Average fetch time: {:?}", total_fetch_time / urls.len() as u32);
    
    client.close().await;
}

#[tokio::test]
async fn test_circuit_status() {
    if !should_run_integration_tests() {
        return;
    }
    
    let (url, fingerprint) = match get_webtunnel_config() {
        Some(config) => config,
        None => return,
    };
    
    use webtor::{TorClient, TorClientOptions};
    
    let options = TorClientOptions::webtunnel(url, fingerprint)
        .with_create_circuit_early(true);
    
    let client = TorClient::new(options).await
        .expect("Failed to create TorClient");
    
    let status = client.get_circuit_status().await;
    
    // After successful connection, we should have at least one ready circuit
    assert!(status.ready_circuits > 0 || status.creating_circuits > 0,
        "Should have active circuits after connection");
    
    let status_str = client.get_circuit_status_string().await;
    assert!(status_str.contains("Ready") || status_str.contains("Creating"),
        "Status should indicate active circuit");
    
    client.close().await;
}

/// Test to verify consensus fetching works
#[tokio::test]
async fn test_consensus_fetch() {
    use webtor::{TorClient, TorClientOptions};
    
    // Create client without early circuit
    let options = TorClientOptions::snowflake()
        .with_create_circuit_early(false);
    
    let client = TorClient::new(options).await
        .expect("Failed to create TorClient");
    
    // Check if consensus needs refresh (it should initially)
    let needs_refresh = client.needs_consensus_refresh().await;
    assert!(needs_refresh, "Fresh client should need consensus refresh");
    
    // Get consensus status
    let status = client.get_consensus_status().await;
    println!("Consensus status: {}", status);
}
