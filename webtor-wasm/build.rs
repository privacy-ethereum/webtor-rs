//! Build script for webtor-wasm
//!
//! Extracts version information from dependencies at build time
//! to avoid hard-coded version strings that may drift.

use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../webtor/Cargo.toml");
    println!("cargo:rerun-if-changed=../subtle-tls/Cargo.toml");
    println!("cargo:rerun-if-changed=../vendor/arti/crates/arti/Cargo.toml");
    println!("cargo:rerun-if-changed=../vendor/arti/crates/tor-proto/Cargo.toml");

    // Extract subtle-tls version
    let subtle_tls_version = extract_version("../subtle-tls/Cargo.toml").unwrap_or("0.1.0");
    println!("cargo:rustc-env=SUBTLE_TLS_VERSION={}", subtle_tls_version);

    // Extract tor-proto version
    let tor_proto_version =
        extract_version("../vendor/arti/crates/tor-proto/Cargo.toml").unwrap_or("0.37.0");
    println!("cargo:rustc-env=TOR_PROTO_VERSION={}", tor_proto_version);

    // Prefer reading Arti's version directly from vendored sources to avoid drift.
    // Fallback to derivation if the file isn't present.
    let arti_version = extract_version("../vendor/arti/crates/arti/Cargo.toml")
        .map(|v| v.to_string())
        .unwrap_or_else(|| derive_arti_version(tor_proto_version));
    println!("cargo:rustc-env=ARTI_VERSION={}", arti_version);
}

fn extract_version(cargo_toml_path: &str) -> Option<&'static str> {
    let path = Path::new(cargo_toml_path);
    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(path).ok()?;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("version") && line.contains('=') {
            // Parse: version = "X.Y.Z"
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                let version = parts[1].trim().trim_matches('"').trim_matches('\'');
                // Leak the string to get 'static lifetime (acceptable in build script)
                return Some(Box::leak(version.to_string().into_boxed_str()));
            }
        }
    }

    None
}

fn derive_arti_version(tor_proto_version: &str) -> String {
    // tor-proto 0.37.0 corresponds to arti 1.8.0
    // The pattern is: arti_minor = tor_proto_minor - 29
    let parts: Vec<&str> = tor_proto_version.split('.').collect();
    if parts.len() >= 2 {
        if let Ok(minor) = parts[1].parse::<i32>() {
            let arti_minor = minor - 29;
            if arti_minor >= 0 {
                return format!("1.{}.0", arti_minor);
            }
        }
    }
    "1.8.0".to_string() // fallback
}
