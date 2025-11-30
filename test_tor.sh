#!/bin/bash

# Test Tor connection through WebTunnel bridge
#
# Usage:
#   ./test_tor.sh                                    # Show instructions
#   ./test_tor.sh <webtunnel_url> <fingerprint>      # Run test with bridge
#
# Get bridges from: https://bridges.torproject.org/options
# Select "webtunnel" as transport type and solve captcha.
#
# Example bridge line:
#   webtunnel [2001:db8::1]:443 FINGERPRINT url=https://example.com/secret
#
# Extract URL and fingerprint and run:
#   ./test_tor.sh "https://example.com/secret" "FINGERPRINT"

set -e

if [ $# -lt 2 ]; then
    echo "=== Tor WebTunnel Test ==="
    echo ""
    echo "This script tests the webtor library against a real Tor relay."
    echo ""
    echo "To get WebTunnel bridges:"
    echo "1. Go to https://bridges.torproject.org/options"
    echo "2. Select 'webtunnel' as transport"
    echo "3. Solve the captcha"
    echo "4. You'll get a bridge line like:"
    echo "   webtunnel [2001:db8::1]:443 ABC123... url=https://example.com/path"
    echo ""
    echo "Usage:"
    echo "  $0 <webtunnel_url> <fingerprint>"
    echo ""
    echo "Example:"
    echo "  $0 'https://example.com/secret-path' 'ABC123DEF456...'"
    echo ""
    exit 0
fi

WEBTUNNEL_URL="$1"
WEBTUNNEL_FINGERPRINT="$2"

echo "=== Testing Tor Connection ==="
echo "WebTunnel URL: $WEBTUNNEL_URL"
echo "Fingerprint: $WEBTUNNEL_FINGERPRINT"
echo ""

# Run the integration test
export WEBTUNNEL_URL
export WEBTUNNEL_FINGERPRINT
export RUST_LOG=info

echo "Running integration test..."
cargo test -p webtor --test integration_test test_webtunnel_fetch_ipify -- --nocapture

echo ""
echo "Running benchmark..."
cargo bench -p webtor
