#![no_main]

use libfuzzer_sys::fuzz_target;
use url::Url;
use webtor::isolation::{IsolationKey, StreamIsolationPolicy};

fuzz_target!(|data: &[u8]| {
    // Try to parse the input as a URL string
    if let Ok(url_str) = std::str::from_utf8(data) {
        // Try various URL schemes
        for prefix in ["https://", "http://"] {
            let full_url = format!("{}{}", prefix, url_str);
            if let Ok(url) = Url::parse(&full_url) {
                // Test all isolation policies - should never panic
                let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerDomain);
                let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerSubdomain);
                let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerOrigin);
                let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::None);
            }
        }

        // Also try parsing directly
        if let Ok(url) = Url::parse(url_str) {
            let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerDomain);
            let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerSubdomain);
            let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerOrigin);
            let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::None);
        }
    }
});
