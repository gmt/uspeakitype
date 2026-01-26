#!/bin/bash
# tests/visual/scripts/capture_goldens.sh
#
# Captures golden reference images for visual regression testing.
# MUST be run from a clean environment (no existing Wayland sessions).
# 
# Usage:
#   - Switch to a TTY (Ctrl+Alt+F2) or use a fresh container
#   - Run: ./tests/visual/scripts/capture_goldens.sh
#
# Requirements:
#   - sway (wlroots compositor)
#   - grim (screenshot tool)
#   - cargo (to build barbara)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/../../.."

# PRECONDITION: No existing Wayland sessions
if ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null | grep -q .; then
    echo "ERROR: Existing Wayland sockets found. Run from clean TTY or container."
    echo "Found sockets:"
    ls -la "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null || true
    exit 1
fi

# Check for required tools
if ! command -v sway &>/dev/null; then
    echo "ERROR: sway not found. Install with: paru -S sway"
    exit 1
fi

if ! command -v grim &>/dev/null; then
    echo "ERROR: grim not found. Install with: paru -S grim"
    exit 1
fi

echo "=== Golden Image Capture Script ==="
echo "Script dir: $SCRIPT_DIR"
echo "Repo root: $REPO_ROOT"

# 1. Record existing sockets (should be empty)
BEFORE_SOCKETS=$(ls -1 "$XDG_RUNTIME_DIR" 2>/dev/null || true)

# 2. Start headless sway (socket discovery, not pre-set)
echo "Starting headless sway..."
export WLR_BACKENDS=headless
export WLR_HEADLESS_OUTPUTS=1
sway -c /dev/null &
SWAY_PID=$!
sleep 2

# 3. Discover the newly created sockets (post-start discovery)
AFTER_SOCKETS=$(ls -1 "$XDG_RUNTIME_DIR" 2>/dev/null || true)
NEW_WAYLAND=$(comm -13 <(echo "$BEFORE_SOCKETS" | sort) <(echo "$AFTER_SOCKETS" | sort) | grep '^wayland-' | head -1)
NEW_SWAYSOCK=$(comm -13 <(echo "$BEFORE_SOCKETS" | sort) <(echo "$AFTER_SOCKETS" | sort) | grep '^sway-ipc' | head -1)

if [[ -z "$NEW_WAYLAND" ]]; then
    echo "ERROR: No Wayland socket created by sway"
    kill $SWAY_PID 2>/dev/null || true
    exit 1
fi

export WAYLAND_DISPLAY="$NEW_WAYLAND"
export SWAYSOCK="$XDG_RUNTIME_DIR/$NEW_SWAYSOCK"
echo "Discovered WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
echo "Discovered SWAYSOCK=$SWAYSOCK"

# Verify sockets exist
if [[ ! -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]]; then
    echo "ERROR: Wayland socket missing at $XDG_RUNTIME_DIR/$WAYLAND_DISPLAY"
    kill $SWAY_PID 2>/dev/null || true
    exit 1
fi

if [[ ! -S "$SWAYSOCK" ]]; then
    echo "ERROR: SWAYSOCK missing at $SWAYSOCK"
    kill $SWAY_PID 2>/dev/null || true
    exit 1
fi

# 4. Configure output with EXPLICIT black background
echo "Configuring output..."
swaymsg output HEADLESS-1 resolution 1920x1080 scale 1 background "#000000" solid_color

# 5. Verify configuration
if ! swaymsg -t get_outputs | grep -q '"width": 1920'; then
    echo "ERROR: Resolution not set correctly"
    kill $SWAY_PID 2>/dev/null || true
    exit 1
fi
echo "Output configured: 1920x1080, scale 1, black background"

# 6. Isolate from user config
export XDG_CONFIG_HOME=$(mktemp -d)
echo "Using temp config dir: $XDG_CONFIG_HOME"

# 7. Mark as canonical environment
export BARBARA_CANONICAL_TEST_ENV=1

# 8. Create golden directory
mkdir -p "$SCRIPT_DIR/../golden"

# 9. Build Barbara BEFORE capturing (avoid compilation affecting timing)
echo "Building Barbara (release)..."
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml"
BARBARA_BIN="$REPO_ROOT/target/release/barbara"

if [[ ! -x "$BARBARA_BIN" ]]; then
    echo "ERROR: Barbara binary not found at $BARBARA_BIN"
    kill $SWAY_PID 2>/dev/null || true
    exit 1
fi
echo "Barbara binary: $BARBARA_BIN"

# Cleanup function
cleanup() {
    echo "Cleaning up..."
    kill $BARBARA_PID 2>/dev/null || true
    kill $SWAY_PID 2>/dev/null || true
    rm -rf "$XDG_CONFIG_HOME"
}
trap cleanup EXIT

# 10. Launch Barbara with isolated config
echo "Launching Barbara in demo mode..."
"$BARBARA_BIN" --demo &
BARBARA_PID=$!

# 11. Capture at each milestone
# Demo timeline:
#   t=2.0s: set_partial "Listening..."
#   t=4.0s: set_partial "Hello world"
#   t=5.0s: commit() -> "Hello world" committed
#   t=6.0s: set_partial "this is streaming"
#   t=7.0s: commit() + set_partial "transcription"
#
# Capture timestamps (with margin):
#   t=3.0s: Gray "Listening..." (partial only)
#   t=5.5s: White "Hello world" (committed, no partial)
#   t=7.5s: Two-tone (committed + partial)

echo "Waiting for t=3.0s (partial: 'Listening...')..."
sleep 3.0
grim "$SCRIPT_DIR/../golden/demo_partial_listening.png"
echo "Captured demo_partial_listening.png"

echo "Waiting for t=5.5s (committed: 'Hello world')..."
sleep 2.5
grim "$SCRIPT_DIR/../golden/demo_committed_hello.png"
echo "Captured demo_committed_hello.png"

echo "Waiting for t=7.5s (two-tone)..."
sleep 2.0
grim "$SCRIPT_DIR/../golden/demo_twotone_streaming.png"
echo "Captured demo_twotone_streaming.png"

echo ""
echo "=== Done! ==="
echo "Golden images saved to: $SCRIPT_DIR/../golden/"
ls -la "$SCRIPT_DIR/../golden/"
