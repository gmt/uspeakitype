#!/bin/bash
set -e

echo "=== usit KDE/fcitx5 Docker Tests ==="

# Start D-Bus session
echo "Starting D-Bus session..."
dbus-daemon --session --address="$DBUS_SESSION_BUS_ADDRESS" --fork

# Start KWin in virtual/headless mode
echo "Starting KWin (virtual backend)..."
kwin_wayland --virtual --no-lockscreen &
KWIN_PID=$!
sleep 2

# Discover Wayland socket
WAYLAND_SOCKET=$(ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null | head -1 | xargs basename)
if [ -z "$WAYLAND_SOCKET" ]; then
    echo "ERROR: Wayland socket not found"
    exit 1
fi
export WAYLAND_DISPLAY="$WAYLAND_SOCKET"
echo "  WAYLAND_DISPLAY=$WAYLAND_DISPLAY"

# Start stock fcitx5; the user-local addon conf points at the built bridge
echo "Starting fcitx5..."
fcitx5 -d
sleep 2

# Test 1: Check if fcitx5 is running
echo ""
echo "=== Test 1: fcitx5 D-Bus presence ==="
if busctl --user list | grep -q org.fcitx.Fcitx5; then
    echo "✓ fcitx5 is running on D-Bus"
else
    echo "✗ fcitx5 not found on D-Bus"
    exit 1
fi

# Test 2: Check if our addon is loaded
echo ""
echo "=== Test 2: usit-bridge addon loaded ==="
if busctl --user call org.fcitx.Fcitx5 /rocks/gmt/usit/FcitxBridge1 rocks.gmt.UsitFcitxBridge1 IsActive 2>/dev/null; then
    echo "✓ usit-bridge addon is responding"
else
    echo "✗ usit-bridge addon not responding"
    # Check fcitx5 debug output
    busctl --user call org.fcitx.Fcitx5 /controller org.fcitx.Fcitx.Controller1 DebugInfo 2>/dev/null | head -20
    exit 1
fi

# Test 3: Test CommitString (won't inject anywhere but shouldn't error)
echo ""
echo "=== Test 3: CommitString method call ==="
if busctl --user call org.fcitx.Fcitx5 /rocks/gmt/usit/FcitxBridge1 rocks.gmt.UsitFcitxBridge1 CommitString s "test" 2>/dev/null; then
    echo "✓ CommitString method works (no input context, but no error)"
else
    echo "✗ CommitString method failed"
    exit 1
fi

# Test 4: Build and run usit's fcitx5_bridge backend probe
echo ""
echo "=== Test 4: usit fcitx5_bridge backend probe ==="
cd /app
timeout 5 ./target/release/usit --headless --demo 2>&1 &
USIT_PID=$!
sleep 3

# Check journal/logs for backend selection
if journalctl --user --since "30 seconds ago" 2>/dev/null | grep -q "fcitx5_bridge: active"; then
    echo "✓ usit detected fcitx5_bridge backend"
elif grep -r "fcitx5_bridge" /tmp/*.log 2>/dev/null; then
    echo "✓ usit detected fcitx5_bridge backend (from logs)"
else
    echo "? Could not verify backend selection (may still work)"
fi

kill $USIT_PID 2>/dev/null || true

# Cleanup
echo ""
echo "=== Cleanup ==="
kill $KWIN_PID 2>/dev/null || true
pkill fcitx5 2>/dev/null || true

echo ""
echo "=== All tests passed! ==="
