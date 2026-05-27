#!/bin/bash
# Build script for peri on ARM (TLT153-MiniEVM / Allwinner T507)
# Target: arm-unknown-linux-gnueabihf (32-bit ARM Cortex-A hard-float)
#
# Toolchain: gcc-linaro-11.3.1-2022.06-x86_64_arm-linux-gnueabihf
# Download from: TLT153-MiniEVM SDK / 4-软件资料/Linux/Tools/
# Extract to: /opt/gcc-linaro-11.3.1-2022.06-x86_64_arm-linux-gnueabihf/
#
# Usage:
#   ./build-arm.sh          # dev build
#   ./build-arm.sh release  # release build

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

TOOLCHAIN_ROOT="${TOOLCHAIN_ROOT:-/opt/gcc-linaro-11.3.1-2022.06-x86_64_arm-linux-gnueabihf}"
TOOLCHAIN_BIN="$TOOLCHAIN_ROOT/bin"
TARGET="arm-unknown-linux-gnueabihf"

# Add toolchain to PATH
export PATH="$TOOLCHAIN_BIN:$PATH"

# Use stable toolchain to avoid nightly issues
export RUSTC="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc"
export CARGO="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo"

# Ensure ARM target is installed
"$CARGO" +stable target add "$TARGET" 2>/dev/null || true

MODE="${1:-dev}"

if [ "$MODE" = "release" ]; then
    echo "=== Building peri for ARM (release) ==="
    "$CARGO" build --target "$TARGET" --release
    echo ""
    echo "=== Output ==="
    ls -lh "$SCRIPT_DIR/target/$TARGET/release/peri"
    echo ""
    echo "=== File info ==="
    file "$SCRIPT_DIR/target/$TARGET/release/peri"
else
    echo "=== Building peri for ARM (dev) ==="
    "$CARGO" build --target "$TARGET"
    echo ""
    echo "=== Output ==="
    ls -lh "$SCRIPT_DIR/target/$TARGET/debug/peri"
    echo ""
    echo "=== File info ==="
    file "$SCRIPT_DIR/target/$TARGET/debug/peri"
fi

echo ""
echo "=== Build complete ==="
echo "Target: $TARGET"
echo "Toolchain: $TOOLCHAIN_ROOT"
echo ""
echo "To deploy: scp target/$TARGET/release/peri root@<board-ip>:/usr/local/bin/"
