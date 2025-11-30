#!/bin/bash
set -e

echo "🔄 Checking for cached consensus..."
cd ..

# Fetch consensus if missing or older than 12 hours
if [ ! -f webtor/src/cached/consensus.txt.gz ] || \
   [ $(find webtor/src/cached/consensus.txt.gz -mmin +720 2>/dev/null | wc -l) -gt 0 ]; then
    echo "📡 Fetching fresh Tor consensus..."
    chmod +x scripts/fetch-consensus.sh
    ./scripts/fetch-consensus.sh webtor/src/cached
else
    echo "✅ Using existing cached consensus"
fi

echo "🔨 Building webtor-wasm..."
wasm-pack build webtor-wasm --target web --out-dir ../example/pkg

# Pre-compress WASM for servers that support pre-compressed assets
echo "🗜️  Pre-compressing WASM for HTTP delivery..."
if command -v brotli &> /dev/null; then
    brotli -9 -k -f example/pkg/webtor_wasm_bg.wasm
    WASM_SIZE=$(wc -c < example/pkg/webtor_wasm_bg.wasm | tr -d ' ')
    BR_SIZE=$(wc -c < example/pkg/webtor_wasm_bg.wasm.br | tr -d ' ')
    echo "   WASM: $WASM_SIZE bytes → Brotli: $BR_SIZE bytes ($(echo "scale=0; $BR_SIZE * 100 / $WASM_SIZE" | bc)%)"
fi

echo "📦 Installing npm dependencies..."
cd example
npm install

echo "✅ Build complete! Run 'npm run dev' to start the development server."
