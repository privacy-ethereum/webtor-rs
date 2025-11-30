#!/bin/bash
# Fetch fresh Tor network consensus and microdescriptors for embedding in WASM
# This script is run by GitHub Actions daily

set -e

OUTPUT_DIR="${1:-webtor/src/cached}"
mkdir -p "$OUTPUT_DIR"

# Directory authorities to try (in order of reliability)
AUTHORITIES=(
    "193.23.244.244:80"   # dannenberg
    "131.188.40.189:80"   # gabelmoo
    "204.13.164.118:80"   # bastet
    "199.58.81.140:80"    # longclaw
    "171.25.193.9:443"    # maatuska
    "86.59.21.38:80"      # tor26
)

echo "🔄 Fetching Tor network consensus..."

# Try each authority until one succeeds
CONSENSUS_FETCHED=false
for AUTH in "${AUTHORITIES[@]}"; do
    HOST="${AUTH%:*}"
    PORT="${AUTH#*:}"
    URL="http://${HOST}:${PORT}/tor/status-vote/current/consensus-microdesc"
    
    echo "   Trying $HOST:$PORT..."
    
    if curl -s --connect-timeout 10 --max-time 60 -o "$OUTPUT_DIR/consensus.txt" "$URL"; then
        # Verify it looks like a consensus
        if head -1 "$OUTPUT_DIR/consensus.txt" | grep -q "network-status-version"; then
            echo "✅ Fetched consensus from $HOST"
            CONSENSUS_FETCHED=true
            break
        else
            echo "   Invalid response from $HOST"
            rm -f "$OUTPUT_DIR/consensus.txt"
        fi
    else
        echo "   Failed to connect to $HOST"
    fi
done

if [ "$CONSENSUS_FETCHED" = false ]; then
    echo "❌ Failed to fetch consensus from any directory authority"
    exit 1
fi

# Get consensus size
CONSENSUS_SIZE=$(wc -c < "$OUTPUT_DIR/consensus.txt" | tr -d ' ')
RELAY_COUNT=$(grep -c "^r " "$OUTPUT_DIR/consensus.txt" || echo "0")
echo "   Consensus size: $CONSENSUS_SIZE bytes, $RELAY_COUNT relays"

# Now fetch microdescriptors for the first ~500 relays (we only need a subset)
echo "🔄 Fetching microdescriptors..."

# Extract microdescriptor digests from consensus (m lines)
# Format: m <digest1>,<digest2>,...
# We need to fetch: /tor/micro/d/<digest1>-<digest2>-...

# Get first 500 microdescriptor digests
DIGESTS=$(grep "^m " "$OUTPUT_DIR/consensus.txt" | head -500 | sed 's/^m //' | tr ',' '\n' | head -500)
DIGEST_COUNT=$(echo "$DIGESTS" | wc -l | tr -d ' ')
echo "   Found $DIGEST_COUNT microdescriptor digests"

# Fetch in batches of 90 (URL length limit)
BATCH_SIZE=90
BATCH_NUM=0
> "$OUTPUT_DIR/microdescriptors.txt"

echo "$DIGESTS" | while IFS= read -r digest; do
    BATCH+=("$digest")
    
    if [ ${#BATCH[@]} -ge $BATCH_SIZE ]; then
        BATCH_NUM=$((BATCH_NUM + 1))
        DIGEST_PATH=$(IFS=-; echo "${BATCH[*]}")
        
        for AUTH in "${AUTHORITIES[@]}"; do
            HOST="${AUTH%:*}"
            PORT="${AUTH#*:}"
            URL="http://${HOST}:${PORT}/tor/micro/d/${DIGEST_PATH}"
            
            if curl -s --connect-timeout 10 --max-time 30 "$URL" >> "$OUTPUT_DIR/microdescriptors.txt" 2>/dev/null; then
                echo "   Batch $BATCH_NUM fetched from $HOST"
                break
            fi
        done
        
        BATCH=()
    fi
done

# Fetch any remaining digests
if [ ${#BATCH[@]} -gt 0 ]; then
    BATCH_NUM=$((BATCH_NUM + 1))
    DIGEST_PATH=$(IFS=-; echo "${BATCH[*]}")
    
    for AUTH in "${AUTHORITIES[@]}"; do
        HOST="${AUTH%:*}"
        PORT="${AUTH#*:}"
        URL="http://${HOST}:${PORT}/tor/micro/d/${DIGEST_PATH}"
        
        if curl -s --connect-timeout 10 --max-time 30 "$URL" >> "$OUTPUT_DIR/microdescriptors.txt" 2>/dev/null; then
            echo "   Final batch fetched from $HOST"
            break
        fi
    done
fi

MICRO_SIZE=$(wc -c < "$OUTPUT_DIR/microdescriptors.txt" | tr -d ' ')
MICRO_COUNT=$(grep -c "^onion-key" "$OUTPUT_DIR/microdescriptors.txt" || echo "0")
echo "✅ Fetched $MICRO_COUNT microdescriptors ($MICRO_SIZE bytes)"

# Create timestamp file
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$OUTPUT_DIR/fetched_at.txt"

# Compress with Brotli for smaller WASM binary (better than gzip)
echo "🗜️  Compressing with Brotli..."

# Check if brotli is available
if command -v brotli &> /dev/null; then
    brotli -9 -k -f "$OUTPUT_DIR/consensus.txt"
    brotli -9 -k -f "$OUTPUT_DIR/microdescriptors.txt"
else
    echo "⚠️  brotli not found, installing..."
    if command -v apt-get &> /dev/null; then
        sudo apt-get install -y brotli
    elif command -v brew &> /dev/null; then
        brew install brotli
    else
        echo "❌ Please install brotli: apt-get install brotli OR brew install brotli"
        exit 1
    fi
    brotli -9 -k -f "$OUTPUT_DIR/consensus.txt"
    brotli -9 -k -f "$OUTPUT_DIR/microdescriptors.txt"
fi

COMPRESSED_CONSENSUS=$(wc -c < "$OUTPUT_DIR/consensus.txt.br" | tr -d ' ')
COMPRESSED_MICRO=$(wc -c < "$OUTPUT_DIR/microdescriptors.txt.br" | tr -d ' ')
echo "   Compressed consensus: $COMPRESSED_CONSENSUS bytes"
echo "   Compressed microdescriptors: $COMPRESSED_MICRO bytes"

echo ""
echo "✅ Done! Cached files in $OUTPUT_DIR/"
ls -la "$OUTPUT_DIR/"
