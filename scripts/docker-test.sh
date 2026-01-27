#!/bin/bash
set -e

echo "=== usit Docker Visual Tests ==="
echo "Environment:"
echo "  XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
echo "  WLR_BACKENDS=$WLR_BACKENDS"
echo "  WLR_RENDERER=$WLR_RENDERER"

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
    exit 1
fi
export WAYLAND_DISPLAY="$WAYLAND_SOCKET"
echo "  WAYLAND_DISPLAY=$WAYLAND_DISPLAY"

# Discover Sway IPC socket
SWAY_SOCKET=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.* 2>/dev/null | head -1)
if [ -n "$SWAY_SOCKET" ]; then
    export SWAYSOCK="$SWAY_SOCKET"
    echo "  SWAYSOCK=$SWAYSOCK"
    
    # Configure output
    swaymsg output HEADLESS-1 resolution 1920x1080 scale 1 background "#000000" solid_color || true
fi

# Verify grim works
echo "Verifying screenshot capture..."
if grim /tmp/test-screenshot.png; then
    echo "  grim: OK"
    rm /tmp/test-screenshot.png
else
    echo "  grim: FAILED (screenshots may not work)"
fi

# Create output directory
mkdir -p /app/test-output

# Run the actual command
echo ""
echo "=== Running: $@ ==="
"$@"
EXIT_CODE=$?

# Cleanup
kill $SWAY_PID 2>/dev/null || true

exit $EXIT_CODE
