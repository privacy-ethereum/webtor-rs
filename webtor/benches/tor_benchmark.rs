//! Performance benchmarks for webtor
//!
//! Run with: cargo bench -p webtor
//!
//! Note: These are integration benchmarks that require network access.
//! For reliable results, run multiple times and use a stable network.

use std::time::{Duration, Instant};

mod benchmark_utils {
    use super::*;

    pub struct BenchmarkResult {
        pub name: String,
        pub duration: Duration,
        pub iterations: u32,
        pub success: bool,
        pub details: String,
    }

    impl BenchmarkResult {
        pub fn print(&self) {
            let status = if self.success { "[OK]" } else { "[FAIL]" };
            println!(
                "{} {} - {:?} ({} iterations)",
                status, self.name, self.duration, self.iterations
            );
            if !self.details.is_empty() {
                println!("   Details: {}", self.details);
            }
        }

        pub fn avg_duration(&self) -> Duration {
            if self.iterations == 0 {
                return Duration::ZERO;
            }
            self.duration / self.iterations
        }
    }

    pub async fn time_async<F, Fut, T>(name: &str, iterations: u32, f: F) -> BenchmarkResult
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error>>>,
    {
        let start = Instant::now();
        let mut success = true;
        let mut details = String::new();

        for i in 0..iterations {
            match f().await {
                Ok(_) => {}
                Err(e) => {
                    success = false;
                    details = format!("Failed on iteration {}: {}", i + 1, e);
                    break;
                }
            }
        }

        BenchmarkResult {
            name: name.to_string(),
            duration: start.elapsed(),
            iterations,
            success,
            details,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("=== Webtor Performance Benchmarks ===\n");
    println!("Note: These benchmarks require network access and a WebTunnel bridge.");
    println!("Set WEBTUNNEL_URL and WEBTUNNEL_FINGERPRINT environment variables.\n");

    run_benchmarks().await;
}

async fn run_benchmarks() {
    use webtor::{TorClient, TorClientOptions};

    // Check for WebTunnel configuration
    let webtunnel_url = std::env::var("WEBTUNNEL_URL").ok();
    let webtunnel_fingerprint = std::env::var("WEBTUNNEL_FINGERPRINT").ok();

    if webtunnel_url.is_none() || webtunnel_fingerprint.is_none() {
        println!("⚠️  WebTunnel configuration not found.");
        println!("   Set WEBTUNNEL_URL and WEBTUNNEL_FINGERPRINT to run full benchmarks.");
        println!("   Example:");
        println!("   export WEBTUNNEL_URL='https://example.com/secret-path'");
        println!("   export WEBTUNNEL_FINGERPRINT='AABBCCDD...'");
        println!("\n   Running limited benchmarks only...\n");

        // Run crypto benchmarks only
        run_crypto_benchmarks().await;
        return;
    }

    let url = webtunnel_url.unwrap();
    let fingerprint = webtunnel_fingerprint.unwrap();

    println!("Using WebTunnel bridge: {}", url);
    println!();

    // Benchmark 1: Client creation (no connection)
    println!("--- Benchmark: Client Creation ---");
    let result = benchmark_utils::time_async("TorClient::new (no early connect)", 5, || async {
        let options = TorClientOptions::webtunnel(url.clone(), fingerprint.clone())
            .with_create_circuit_early(false);
        TorClient::new(options)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    })
    .await;
    result.print();
    println!("   Average: {:?}", result.avg_duration());
    println!();

    // Benchmark 2: Full connection establishment
    println!("--- Benchmark: Full Connection ---");
    let start = Instant::now();
    let options = TorClientOptions::webtunnel(url.clone(), fingerprint.clone())
        .with_create_circuit_early(true)
        .with_connection_timeout(30_000)
        .with_circuit_timeout(120_000);

    match TorClient::new(options).await {
        Ok(client) => {
            let connect_time = start.elapsed();
            println!(" Connection established in {:?}", connect_time);

            // Benchmark 3: HTTP fetch through Tor
            println!("\n--- Benchmark: HTTP Fetch ---");
            let fetch_start = Instant::now();
            match client.fetch("https://api64.ipify.org?format=json").await {
                Ok(response) => {
                    let fetch_time = fetch_start.elapsed();
                    println!(" Fetch completed in {:?}", fetch_time);
                    println!("   Status: {}", response.status);
                    println!("   Body: {}", String::from_utf8_lossy(&response.body));

                    // Benchmark 4: Multiple fetches (connection reuse)
                    println!("\n--- Benchmark: Multiple Fetches (connection reuse) ---");
                    let multi_start = Instant::now();
                    let mut success_count = 0;
                    for i in 0..3 {
                        let iter_start = Instant::now();
                        match client.fetch("https://httpbin.org/ip").await {
                            Ok(resp) => {
                                println!(
                                    "   Fetch {} completed in {:?} - {}",
                                    i + 1,
                                    iter_start.elapsed(),
                                    resp.status
                                );
                                success_count += 1;
                            }
                            Err(e) => {
                                println!("   Fetch {} failed: {}", i + 1, e);
                            }
                        }
                    }
                    let multi_time = multi_start.elapsed();
                    println!(
                        "   Total: {:?} ({}/3 successful)",
                        multi_time, success_count
                    );
                }
                Err(e) => {
                    println!(" Fetch failed: {}", e);
                }
            }

            client.close().await;
        }
        Err(e) => {
            println!(" Connection failed: {}", e);
        }
    }

    // Run crypto benchmarks
    println!();
    run_crypto_benchmarks().await;
}

async fn run_crypto_benchmarks() {
    use std::time::Instant;

    println!("--- Benchmark: Cryptographic Operations ---");

    // X25519 key generation
    {
        let iterations = 1000;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = x25519_dalek::EphemeralSecret::random_from_rng(rand::rngs::OsRng);
        }
        let elapsed = start.elapsed();
        println!(
            "   X25519 key generation: {:?} / {} iterations ({:?} avg)",
            elapsed,
            iterations,
            elapsed / iterations
        );
    }

    // ChaCha20-Poly1305 encryption
    {
        use chacha20poly1305::{
            aead::{Aead, KeyInit},
            ChaCha20Poly1305, Key, Nonce,
        };

        let iterations = 10000;
        let key = Key::from_slice(&[0u8; 32]);
        let cipher = ChaCha20Poly1305::new(key);
        let nonce = Nonce::from_slice(&[0u8; 12]);
        let plaintext = vec![0u8; 1024]; // 1KB payload

        let start = Instant::now();
        for _ in 0..iterations {
            let _ = cipher.encrypt(nonce, plaintext.as_slice());
        }
        let elapsed = start.elapsed();
        let throughput = (iterations as f64 * 1024.0) / elapsed.as_secs_f64() / 1_000_000.0;
        println!(
            "   ChaCha20-Poly1305 encrypt (1KB): {:?} / {} iterations ({:.2} MB/s)",
            elapsed, iterations, throughput
        );
    }

    // SHA-256
    {
        use sha2::{Digest, Sha256};

        let iterations = 100000;
        let data = vec![0u8; 64];

        let start = Instant::now();
        for _ in 0..iterations {
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let _ = hasher.finalize();
        }
        let elapsed = start.elapsed();
        println!(
            "   SHA-256 (64 bytes): {:?} / {} iterations ({:?} avg)",
            elapsed,
            iterations,
            elapsed / iterations
        );
    }

    println!();
}
