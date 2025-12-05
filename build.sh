#!/bin/bash

set -e

echo "🚀 Building Webtor-rs project..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Parse arguments
BUILD_MODE="--release"

while [[ $# -gt 0 ]]; do
    case $1 in
        --dev)
            BUILD_MODE="--dev"
            shift
            ;;
        *)
            shift
            ;;
    esac
done

# Check if wasm-pack is installed
if ! command -v wasm-pack &> /dev/null; then
    print_error "wasm-pack is not installed. Please install it first:"
    print_error "curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh"
    exit 1
fi

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    print_error "Rust/Cargo is not installed. Please install Rust first:"
    print_error "https://rustup.rs/"
    exit 1
fi

print_status "Build mode: $BUILD_MODE"

print_status "Building webtor-wasm (WebAssembly bindings)..."
cd webtor-wasm
wasm-pack build --target web --out-dir pkg $BUILD_MODE
if [ $? -ne 0 ]; then
    print_error "Failed to build webtor-wasm"
    exit 1
fi
cd ..

print_status "Building webtor-demo (Demo webpage)..."
cd webtor-demo
wasm-pack build --target web --out-dir pkg $BUILD_MODE
if [ $? -ne 0 ]; then
    print_error "Failed to build webtor-demo"
    exit 1
fi
cd ..

print_status "Copying demo files..."
mkdir -p webtor-demo/static/pkg
cp -r webtor-demo/pkg/* webtor-demo/static/pkg/

# Run wasm-opt if available (for additional size optimization)
optimize_wasm() {
    local path="$1"
    if [ -f "$path" ] && command -v wasm-opt &> /dev/null; then
        print_status "Running wasm-opt on $(basename "$path")..."
        wasm-opt -Oz --strip-dwarf --strip-producers \
            -o "${path}.opt" "$path"
        mv "${path}.opt" "$path"
    fi
}

# Print WASM sizes (uncompressed and gzipped)
print_wasm_size() {
    local path="$1"
    if [ -f "$path" ]; then
        local size=$(ls -lh "$path" | awk '{print $5}')
        local gz_size=$(gzip -c -9 "$path" | wc -c | awk '{printf "%.2f MB", $1/1024/1024}')
        print_status "$(basename "$path"): $size (uncompressed), $gz_size (gzipped)"
    fi
}

# Optimize WASM binaries if wasm-opt is available
if [ "$BUILD_MODE" = "--release" ]; then
    optimize_wasm webtor-wasm/pkg/webtor_wasm_bg.wasm
    optimize_wasm webtor-demo/pkg/webtor_demo_bg.wasm
fi

# Show WASM sizes
echo ""
print_wasm_size webtor-wasm/pkg/webtor_wasm_bg.wasm
print_wasm_size webtor-demo/pkg/webtor_demo_bg.wasm

echo ""
print_status "Build completed successfully!"
echo ""
print_status "Usage:"
print_status "  ./build.sh          # Production build (optimized, slower compile)"
print_status "  ./build.sh --dev    # Development build (fast compile, no optimization)"
echo ""
print_status "To run the demo:"
print_status "  cd webtor-demo/static && python3 -m http.server 8000"
print_status "  Open http://localhost:8000 in your browser"
