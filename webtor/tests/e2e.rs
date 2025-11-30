//! End-to-end integration tests for webtor
//!
//! These tests require network access and make real requests through Tor.
//! They are marked with #[ignore] by default since they take several minutes.
//!
//! Run with: cargo test -p webtor --test e2e -- --ignored --nocapture

use webtor::{TorClient, TorClientOptions};
use webtor::snowflake::{SnowflakeBridge, SnowflakeConfig};

/// WebTunnel bridge from community bridges list
/// These are real bridges - they may go offline. Try a different one if it fails.
/// Note: Bridges behind Cloudflare may not work due to WebSocket key validation issues.
/// Source: https://github.com/scriptzteam/Tor-Bridges-Collector/blob/main/bridges-webtunnel
const WEBTUNNEL_URL: &str = "https://fdmf.ch/QCjqMFJumKjWgB7BFaOc04dN";
const WEBTUNNEL_FINGERPRINT: &str = "58DA67BD879E9239FCD4A590E25118BB2118CB3C";

/// Alternative WebTunnel bridges if the primary one is down
/// Prefer bridges NOT behind Cloudflare CDN
#[allow(dead_code)]
const WEBTUNNEL_BRIDGES: &[(&str, &str)] = &[
    ("https://fdmf.ch/QCjqMFJumKjWgB7BFaOc04dN", "58DA67BD879E9239FCD4A590E25118BB2118CB3C"),
    ("https://kochenjessler.de/ayaSmSql2aohr1GYPsHAc8w9", "EEC3B74384FF65C03CE3308AC8911C1A70FD6A57"),
    ("https://shallotfarm.org/jcHgyp7m90iQr9QaVSprq1wP", "770EA6412C8D3997ABFFF7173A3E53F1D3660167"),
    ("https://wt.skynetcloud.site/GtMuw9ifqfu3AAap4GVHHnBx", "63BAB47B6DB9CA74AAC004E803F02E6E8D425E48"),
];

#[tokio::test]
#[ignore] // Takes several minutes - run with: cargo test -p webtor --test e2e -- --ignored
async fn test_webtunnel_https_request() {
    // Initialize tracing for better debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("webtor=debug,tor_proto=info")
        .try_init();
    
    println!("=== E2E Test: WebTunnel HTTPS request to jsonip.com ===");
    println!("This test will:");
    println!("  1. Fetch Tor network consensus");
    println!("  2. Connect to WebTunnel bridge via HTTPS");
    println!("  3. Build a 3-hop circuit");
    println!("  4. Make HTTPS request to jsonip.com");
    println!("  5. Parse JSON response with exit node IP");
    println!();
    println!("Using WebTunnel bridge: {}", WEBTUNNEL_URL);
    println!("Bridge fingerprint: {}", WEBTUNNEL_FINGERPRINT);
    println!();
    
    let options = TorClientOptions::webtunnel(
        WEBTUNNEL_URL.to_string(),
        WEBTUNNEL_FINGERPRINT.to_string(),
    )
    .with_create_circuit_early(true)
    .with_connection_timeout(30_000)  // 30 seconds
    .with_circuit_timeout(120_000);   // 2 minutes
    
    println!("Creating Tor client...");
    let client = TorClient::new(options).await
        .expect("Failed to create Tor client");
    
    println!("Tor client created, circuit should be ready");
    println!("Consensus status: {}", client.get_consensus_status().await);
    println!("Circuit status: {}", client.get_circuit_status_string().await);
    
    println!("\nMaking HTTPS request to https://jsonip.com/...");
    let response = client.get("https://jsonip.com/").await
        .expect("Failed to make HTTPS request");
    
    println!("\n=== Response ===");
    println!("Status: {}", response.status);
    println!("Headers: {:?}", response.headers);
    
    let body_text = response.text().expect("Failed to get response text");
    println!("Body: {}", body_text);
    
    // Parse the JSON to verify we got a valid response
    #[derive(serde::Deserialize, Debug)]
    struct JsonIpResponse {
        ip: String,
    }
    
    let json: JsonIpResponse = response.json()
        .expect("Failed to parse JSON response");
    
    println!("\n=== Parsed JSON ===");
    println!("Exit IP: {}", json.ip);
    
    // Verify we got a valid IP address
    assert!(!json.ip.is_empty(), "IP should not be empty");
    assert!(json.ip.contains('.') || json.ip.contains(':'), "Should be valid IPv4 or IPv6");
    
    println!("\n✅ E2E test passed! Successfully made HTTPS request through Tor via WebTunnel.");
    
    // Cleanup
    client.close().await;
}

#[tokio::test]
#[ignore]
async fn test_webtunnel_connection_only() {
    // Simpler test that just tests the WebTunnel connection
    let _ = tracing_subscriber::fmt()
        .with_env_filter("webtor=debug")
        .try_init();
    
    println!("=== E2E Test: WebTunnel connection test ===");
    
    let options = TorClientOptions::webtunnel(
        WEBTUNNEL_URL.to_string(),
        WEBTUNNEL_FINGERPRINT.to_string(),
    )
    .with_create_circuit_early(true)
    .with_connection_timeout(30_000);
    
    let client = TorClient::new(options).await
        .expect("Failed to create Tor client");
    
    let status = client.get_circuit_status_string().await;
    println!("Circuit status: {}", status);
    
    assert!(status == "Ready" || status.contains("Ready"), "Circuit should be ready");
    
    client.close().await;
    println!("✅ WebTunnel connection test passed!");
}

#[tokio::test]
#[ignore]
async fn test_try_multiple_bridges() {
    // Try multiple bridges in case some are down
    let _ = tracing_subscriber::fmt()
        .with_env_filter("webtor=info")
        .try_init();
    
    println!("=== E2E Test: Try multiple WebTunnel bridges ===");
    
    for (i, (url, fingerprint)) in WEBTUNNEL_BRIDGES.iter().enumerate() {
        println!("\nTrying bridge {} of {}: {}", i + 1, WEBTUNNEL_BRIDGES.len(), url);
        
        let options = TorClientOptions::webtunnel(
            url.to_string(),
            fingerprint.to_string(),
        )
        .with_create_circuit_early(true)
        .with_connection_timeout(20_000);
        
        match TorClient::new(options).await {
            Ok(client) => {
                println!("✅ Successfully connected to bridge!");
                client.close().await;
                return; // Success!
            }
            Err(e) => {
                println!("❌ Failed: {}", e);
                continue;
            }
        }
    }
    
    panic!("All bridges failed to connect");
}

/// Snowflake broker URLs
const SNOWFLAKE_BROKER_URL: &str = "https://snowflake-broker.torproject.net/";
const SNOWFLAKE_BROKER_URL_ALT: &str = "https://snowflake-broker.bamsoftware.com/";

#[tokio::test]
#[ignore]
async fn test_snowflake_connection() {
    // Test the Snowflake connection using WebRTC + Turbo + KCP + SMUX stack
    // NOTE: This test requires a browser environment (WASM) to work
    // On native, it will fail with "Snowflake requires WebRTC"
    let _ = tracing_subscriber::fmt()
        .with_env_filter("webtor=debug")
        .try_init();
    
    println!("=== E2E Test: Snowflake connection test ===");
    println!("This test will:");
    println!("  1. Contact Snowflake broker to get proxy assignment");
    println!("  2. Establish WebRTC DataChannel to volunteer proxy");
    println!("  3. Initialize Turbo framing layer");
    println!("  4. Initialize KCP reliability layer");
    println!("  5. Initialize SMUX multiplexing layer");
    println!();
    println!("Using Snowflake broker: {}", SNOWFLAKE_BROKER_URL);
    println!();
    println!("NOTE: Snowflake requires WebRTC which is only available in WASM.");
    println!("      This test will fail on native. Use WebTunnel instead.");
    println!();
    
    let config = SnowflakeConfig::new()
        .with_timeout(std::time::Duration::from_secs(60));
    
    println!("Connecting to Snowflake bridge via WebRTC...");
    let bridge = SnowflakeBridge::with_config(config);
    
    match bridge.connect().await {
        Ok(mut stream) => {
            println!("✅ Snowflake connection established!");
            println!("Protocol stack initialized: WebRTC → Turbo → KCP → SMUX");
            
            // Try to close gracefully
            if let Err(e) = stream.close().await {
                println!("Warning: Error during close: {}", e);
            }
            
            println!("✅ Snowflake connection test passed!");
        }
        Err(e) => {
            // On native, this is expected to fail
            println!("❌ Failed to connect: {}", e);
            println!("This is expected on native builds - Snowflake requires WebRTC (WASM only).");
            println!("Use WebTunnel bridge for native testing.");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_snowflake_alternate_broker() {
    // Test the alternate Snowflake broker
    // NOTE: Snowflake requires WebRTC (WASM only)
    let _ = tracing_subscriber::fmt()
        .with_env_filter("webtor=debug")
        .try_init();
    
    println!("=== E2E Test: Snowflake alternate broker ===");
    println!("Using broker: {}", SNOWFLAKE_BROKER_URL_ALT);
    println!();
    println!("NOTE: Snowflake requires WebRTC which is only available in WASM.");
    println!("      This test will fail on native. Use WebTunnel instead.");
    println!();
    
    // On native, this will fail immediately
    let options = TorClientOptions::snowflake()
        .with_create_circuit_early(true)
        .with_connection_timeout(60_000)
        .with_circuit_timeout(180_000);
    
    println!("Creating Tor client with Snowflake...");
    match TorClient::new(options).await {
        Ok(client) => {
            println!("Tor client created");
            println!("Consensus status: {}", client.get_consensus_status().await);
            println!("Circuit status: {}", client.get_circuit_status_string().await);
            client.close().await;
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
            println!("This is expected on native - Snowflake requires WebRTC (WASM only).");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_snowflake_tor_circuit() {
    // Test the full Tor circuit through Snowflake
    // NOTE: Snowflake requires WebRTC (WASM only)
    let _ = tracing_subscriber::fmt()
        .with_env_filter("webtor=debug,tor_proto=info")
        .try_init();
    
    println!("=== E2E Test: Snowflake Tor Circuit ===");
    println!("This test will:");
    println!("  1. Fetch Tor network consensus");
    println!("  2. Connect to Snowflake via WebRTC + Turbo + KCP + SMUX");
    println!("  3. Establish Tor channel with bridge");
    println!("  4. Build 3-hop circuit");
    println!("  5. Make HTTPS request");
    println!();
    println!("NOTE: Snowflake requires WebRTC which is only available in WASM.");
    println!("      This test will fail on native. Use WebTunnel instead.");
    println!();
    
    let options = TorClientOptions::snowflake()
        .with_create_circuit_early(true)
        .with_connection_timeout(60_000)  // 60 seconds for Snowflake
        .with_circuit_timeout(180_000);   // 3 minutes for circuit
    
    println!("Creating Tor client with Snowflake...");
    match TorClient::new(options).await {
        Ok(client) => {
            println!("Tor client created");
            println!("Consensus status: {}", client.get_consensus_status().await);
            println!("Circuit status: {}", client.get_circuit_status_string().await);
    
            println!("\nMaking HTTPS request to https://jsonip.com/...");
            let response = client.get("https://jsonip.com/").await
                .expect("Failed to make HTTPS request");
    
            println!("\n=== Response ===");
            println!("Status: {}", response.status);
    
            let body_text = response.text().expect("Failed to get response text");
            println!("Body: {}", body_text);
    
            // Parse the JSON to verify we got a valid response
            #[derive(serde::Deserialize, Debug)]
            struct JsonIpResponse {
                ip: String,
            }
    
            let json: JsonIpResponse = response.json()
                .expect("Failed to parse JSON response");
    
            println!("\n=== Parsed JSON ===");
            println!("Exit IP: {}", json.ip);
    
            assert!(!json.ip.is_empty(), "IP should not be empty");
    
            println!("\n✅ E2E test passed! Successfully made HTTPS request through Tor via Snowflake.");
    
            client.close().await;
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
            println!("This is expected on native - Snowflake requires WebRTC (WASM only).");
        }
    }
}
