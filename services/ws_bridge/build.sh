#!/usr/bin/env bash
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Building wsb (Rust) ==="

# Check for cargo / rustc
if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust compiler not found. Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
fi

echo "Building release binary..."
cargo build --release

echo ""
echo "Build complete! Binary located at:"
echo "$SCRIPT_DIR/target/release/wsb"
