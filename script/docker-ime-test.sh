#!/bin/bash
set -e

echo "=== usit Docker Input Method Tests ==="

# Check for Cargo.lock staleness
if [ -n "$HOST_CARGO_LOCK_HASH" ] && [ -f /app/.cargo-lock-hash ]; then
    IMAGE_HASH=$(cat /app/.cargo-lock-hash)
    if [ "$HOST_CARGO_LOCK_HASH" != "$IMAGE_HASH" ]; then
        echo ""
        echo "WARNING: Docker image has stale dependencies!"
        echo "  Host Cargo.lock:  ${HOST_CARGO_LOCK_HASH:0:16}..."
        echo "  Image Cargo.lock: ${IMAGE_HASH:0:16}..."
        echo "  Run: docker compose build ime-tests"
        echo ""
        if [ "$USIT_STRICT_SYNC" = "1" ]; then
            echo "ERROR: USIT_STRICT_SYNC=1, aborting due to stale image"
            exit 1
        fi
    else
        echo "Cargo.lock: in sync"
    fi
fi

echo "Environment:"
echo "  XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
echo "  WLR_BACKENDS=$WLR_BACKENDS"
echo "  WLR_RENDERER=$WLR_RENDERER"
echo "  USIT_CANONICAL_TEST_ENV=$USIT_CANONICAL_TEST_ENV"

# Set canonical test environment variable
export USIT_CANONICAL_TEST_ENV=1

# Start headless Sway
echo "Starting headless Sway..."
sway -d 2>/dev/null &
SWAY_PID=$!

# Wait for Sway to initialize
sleep 2

# Discover Wayland socket
WAYLAND_SOCKET=$(ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null | head -1 | xargs basename)
if [ -z "$WAYLAND_SOCKET" ]; then
    echo "ERROR: Wayland socket not found in $XDG_RUNTIME_DIR"
    kill $SWAY_PID 2>/dev/null || true
    exit 1
fi
export WAYLAND_DISPLAY="$WAYLAND_SOCKET"
echo "  WAYLAND_DISPLAY=$WAYLAND_DISPLAY"

# Create output directory
mkdir -p /app/test-output

# Run the actual command
echo ""
echo "=== Running: cargo test --release --test input_method_tests -- --ignored --nocapture ==="
cargo test --release --test input_method_tests -- --ignored --nocapture
EXIT_CODE=$?

# Cleanup
kill $SWAY_PID 2>/dev/null || true

exit $EXIT_CODE
