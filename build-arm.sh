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
#   ./build-arm.sh release  # release build (default)
#   ./build-arm.sh pkg      # build release and package deploy tarball

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

MODE="${1:-release}"

build_release() {
    echo "=== Building peri for ARM (release) ==="
    "$CARGO" build --target "$TARGET" --release
    echo ""
    echo "=== Output ==="
    ls -lh "$SCRIPT_DIR/target/$TARGET/release/peri"
    echo ""
    echo "=== File info ==="
    file "$SCRIPT_DIR/target/$TARGET/release/peri"
}

build_dev() {
    echo "=== Building peri for ARM (dev) ==="
    "$CARGO" build --target "$TARGET"
    echo ""
    echo "=== Output ==="
    ls -lh "$SCRIPT_DIR/target/$TARGET/debug/peri"
    echo ""
    echo "=== File info ==="
    file "$SCRIPT_DIR/target/$TARGET/debug/peri"
}

package_deploy() {
    echo ""
    echo "=== Creating deploy package ==="

    PERI_BIN="$SCRIPT_DIR/target/$TARGET/release/peri"
    if [ ! -f "$PERI_BIN" ]; then
        error "Release binary not found, run: $0 release"
        exit 1
    fi

    PKG_DIR="$SCRIPT_DIR/deploy/peri-tlt153-v0.1.0"
    mkdir -p "$PKG_DIR/lib"

    # Copy binary
    cp "$PERI_BIN" "$PKG_DIR/peri"

    # Copy libs from toolchain
    TOOLCHAIN_LIBS="$TOOLCHAIN_ROOT/arm-linux-gnueabihf/libc/lib"
    cp -v "$TOOLCHAIN_LIBS/libc.so.6" "$PKG_DIR/lib/" || true
    cp -v "$TOOLCHAIN_LIBS/libm.so.6" "$PKG_DIR/lib/" || true
    cp -v "$TOOLCHAIN_LIBS/libgcc_s.so.1" "$PKG_DIR/lib/" || true

    # Create tarball
    DATE=$(date +%Y%m%d)
    PKG_TAR="$SCRIPT_DIR/deploy/peri-tlt153-v0.1.0-$DATE.tar.gz"
    cd "$SCRIPT_DIR/deploy"
    tar -czf "$PKG_TAR" \
        --exclude='deploy.sh' \
        --exclude='*.sh' \
        --exclude='README.md' \
        --exclude='env.example' \
        'peri-tlt153-v0.1.0'
    cd "$SCRIPT_DIR"

    echo ""
    echo "=== Deploy package created ==="
    ls -lh "$PKG_TAR"
    echo ""
    echo "Package contents:"
    tar -tzf "$PKG_TAR" | head -10
    echo ""
    echo "To deploy:"
    echo "  tar -xzf $PKG_TAR -C /"
    echo "  # Then on target board:"
    echo "  LD_LIBRARY_PATH=/opt/peri/lib /opt/peri/peri --print '你好'"
}

case "$MODE" in
    release)
        build_release
        echo ""
        echo "=== Build complete ==="
        echo "Target: $TARGET"
        echo "Toolchain: $TOOLCHAIN_ROOT"
        echo ""
        echo "To deploy: scp target/$TARGET/release/peri root@<board-ip>:/usr/local/bin/"
        ;;
    dev)
        build_dev
        echo ""
        echo "=== Build complete ==="
        echo "Target: $TARGET"
        echo "Toolchain: $TOOLCHAIN_ROOT"
        ;;
    pkg)
        build_release
        package_deploy
        ;;
    *)
        echo "Unknown mode: $MODE"
        echo "Usage: $0 [release|dev|pkg]"
        exit 1
        ;;
esac
