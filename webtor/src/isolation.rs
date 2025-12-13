//! Stream isolation for domain-based circuit separation
//!
//! This module implements stream isolation to prevent cross-site correlation
//! at exit relays. Different domains use different Tor circuits, following
//! Tor Browser's first-party isolation strategy.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::{Hash, Hasher};
use url::Url;

/// Stream isolation policy determining how requests are grouped into circuits
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamIsolationPolicy {
    /// Isolate by first-party domain (eTLD+1 approximation)
    /// e.g., foo.example.com and bar.example.com share a circuit
    PerDomain,
    /// Isolate by full hostname including subdomains
    /// e.g., foo.example.com and bar.example.com use different circuits
    PerSubdomain,
    /// Isolate by full origin (scheme + host + port)
    /// e.g., http://example.com and https://example.com use different circuits
    PerOrigin,
    /// No isolation - all requests share circuits (legacy behavior)
    None,
}

impl Default for StreamIsolationPolicy {
    fn default() -> Self {
        StreamIsolationPolicy::PerDomain
    }
}

/// Isolation key that uniquely identifies a stream isolation group
#[derive(Clone, Eq)]
pub struct IsolationKey(pub String);

impl PartialEq for IsolationKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Hash for IsolationKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl fmt::Debug for IsolationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IsolationKey({})", self.0)
    }
}

impl fmt::Display for IsolationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl IsolationKey {
    /// Create an isolation key from a URL based on the given policy
    pub fn from_url(url: &Url, policy: StreamIsolationPolicy) -> Option<Self> {
        match policy {
            StreamIsolationPolicy::None => None,
            _ => {
                let host = url.host_str().unwrap_or("");
                let port = url.port_or_known_default().unwrap_or(0);

                let key = match policy {
                    StreamIsolationPolicy::PerOrigin => {
                        format!("{}://{}:{}", url.scheme(), host, port)
                    }
                    StreamIsolationPolicy::PerSubdomain => host.to_string(),
                    StreamIsolationPolicy::PerDomain => extract_domain(host),
                    StreamIsolationPolicy::None => unreachable!(),
                };

                Some(IsolationKey(key))
            }
        }
    }

    /// Create an isolation key from a raw string (for testing)
    pub fn from_string(s: impl Into<String>) -> Self {
        IsolationKey(s.into())
    }
}

/// Extract the registrable domain (eTLD+1) from a hostname using the Public Suffix List
///
/// Uses Mozilla's Public Suffix List via the `psl` crate for accurate domain extraction.
/// This handles all known TLDs including multi-part suffixes like co.uk, com.au, etc.
fn extract_domain(host: &str) -> String {
    if host.is_empty() {
        return String::new();
    }

    // Handle IP addresses - return as-is
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_string();
    }

    // Handle IPv6 addresses in brackets
    if host.starts_with('[') && host.ends_with(']') {
        return host.to_string();
    }

    // Use PSL to extract the registrable domain (eTLD+1)
    // The psl crate works with bytes and handles lowercase internally
    match psl::domain(host.as_bytes()) {
        Some(domain) => {
            // Convert the domain back to a string
            // The Domain type implements PartialEq<str> and can be converted
            std::str::from_utf8(domain.as_bytes())
                .unwrap_or(host)
                .to_string()
        }
        None => {
            // PSL couldn't find a registrable domain - this happens for:
            // - TLDs themselves (e.g., "com", "co.uk")
            // - Private/unknown TLDs
            // Fall back to returning the host as-is
            host.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_per_subdomain_uses_full_host() {
        let url = Url::parse("https://foo.bar.example.com:443/path").unwrap();
        let key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerSubdomain).unwrap();
        assert_eq!(key.0, "foo.bar.example.com");
    }

    #[test]
    fn test_per_domain_uses_registrable_domain() {
        let url = Url::parse("https://foo.bar.example.com").unwrap();
        let key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerDomain).unwrap();
        assert_eq!(key.0, "example.com");
    }

    #[test]
    fn test_per_origin_includes_scheme_and_port() {
        let url = Url::parse("https://example.com:4443/path").unwrap();
        let key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerOrigin).unwrap();
        assert_eq!(key.0, "https://example.com:4443");
    }

    #[test]
    fn test_per_origin_with_default_port() {
        let url = Url::parse("https://example.com/path").unwrap();
        let key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerOrigin).unwrap();
        assert_eq!(key.0, "https://example.com:443");
    }

    #[test]
    fn test_http_and_https_same_domain() {
        let http_url = Url::parse("http://example.com/").unwrap();
        let https_url = Url::parse("https://example.com/").unwrap();

        let http_key = IsolationKey::from_url(&http_url, StreamIsolationPolicy::PerDomain).unwrap();
        let https_key =
            IsolationKey::from_url(&https_url, StreamIsolationPolicy::PerDomain).unwrap();
        assert_eq!(http_key, https_key);

        let http_key = IsolationKey::from_url(&http_url, StreamIsolationPolicy::PerOrigin).unwrap();
        let https_key =
            IsolationKey::from_url(&https_url, StreamIsolationPolicy::PerOrigin).unwrap();
        assert_ne!(http_key, https_key);
    }

    #[test]
    fn test_ip_address_handling() {
        let url = Url::parse("http://192.168.1.1:8080/").unwrap();
        let key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerDomain).unwrap();
        assert_eq!(key.0, "192.168.1.1");
    }

    #[test]
    fn test_onion_address() {
        let url = Url::parse("http://exampleonion123456.onion/").unwrap();
        let key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerDomain).unwrap();
        assert_eq!(key.0, "exampleonion123456.onion");
    }

    #[test]
    fn test_two_part_tld() {
        let url = Url::parse("https://www.example.co.uk/").unwrap();
        let key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerDomain).unwrap();
        assert_eq!(key.0, "example.co.uk");
    }

    #[test]
    fn test_no_isolation_returns_none() {
        let url = Url::parse("https://example.com/").unwrap();
        let key = IsolationKey::from_url(&url, StreamIsolationPolicy::None);
        assert!(key.is_none());
    }

    #[test]
    fn test_isolation_key_equality() {
        let key1 = IsolationKey::from_string("example.com");
        let key2 = IsolationKey::from_string("example.com");
        let key3 = IsolationKey::from_string("other.com");

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_subdomains_same_domain_per_domain() {
        let url1 = Url::parse("https://foo.example.com/").unwrap();
        let url2 = Url::parse("https://bar.example.com/").unwrap();

        let key1 = IsolationKey::from_url(&url1, StreamIsolationPolicy::PerDomain).unwrap();
        let key2 = IsolationKey::from_url(&url2, StreamIsolationPolicy::PerDomain).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_subdomains_different_per_subdomain() {
        let url1 = Url::parse("https://foo.example.com/").unwrap();
        let url2 = Url::parse("https://bar.example.com/").unwrap();

        let key1 = IsolationKey::from_url(&url1, StreamIsolationPolicy::PerSubdomain).unwrap();
        let key2 = IsolationKey::from_url(&url2, StreamIsolationPolicy::PerSubdomain).unwrap();
        assert_ne!(key1, key2);
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn isolation_key_from_url_never_panics(
            scheme in "(http|https)",
            host in "[a-z]{1,10}(\\.[a-z]{1,10}){0,3}",
            port in 1u16..65535,
            path in "/[a-z]{0,20}",
        ) {
            let url_str = format!("{}://{}:{}{}", scheme, host, port, path);
            if let Ok(url) = Url::parse(&url_str) {
                let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerDomain);
                let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerSubdomain);
                let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::PerOrigin);
                let _ = IsolationKey::from_url(&url, StreamIsolationPolicy::None);
            }
        }

        #[test]
        fn per_domain_always_shorter_or_equal_to_per_subdomain(
            host in "[a-z]{1,5}(\\.[a-z]{1,5}){1,4}",
        ) {
            let url_str = format!("https://{}/", host);
            if let Ok(url) = Url::parse(&url_str) {
                let domain_key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerDomain).unwrap();
                let subdomain_key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerSubdomain).unwrap();
                prop_assert!(domain_key.0.len() <= subdomain_key.0.len());
            }
        }

        #[test]
        fn per_origin_includes_scheme(
            scheme in "(http|https)",
            host in "[a-z]{1,10}\\.[a-z]{1,5}",
        ) {
            let url_str = format!("{}://{}/", scheme, host);
            if let Ok(url) = Url::parse(&url_str) {
                let key = IsolationKey::from_url(&url, StreamIsolationPolicy::PerOrigin).unwrap();
                prop_assert!(key.0.starts_with(&scheme));
            }
        }

        #[test]
        fn same_url_same_key(url_str in "https://[a-z]{1,5}\\.[a-z]{1,5}/[a-z]{0,10}") {
            if let Ok(url) = Url::parse(&url_str) {
                for policy in [
                    StreamIsolationPolicy::PerDomain,
                    StreamIsolationPolicy::PerSubdomain,
                    StreamIsolationPolicy::PerOrigin,
                ] {
                    let key1 = IsolationKey::from_url(&url, policy);
                    let key2 = IsolationKey::from_url(&url, policy);
                    prop_assert_eq!(key1, key2);
                }
            }
        }
    }
}
