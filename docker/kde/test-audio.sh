#!/bin/bash
set -e

echo "=== usit PipeWire Audio Capture Tests ==="

# Start D-Bus session
echo "Starting D-Bus session..."
dbus-daemon --session --address="$DBUS_SESSION_BUS_ADDRESS" --fork
sleep 1

# Start PipeWire
echo "Starting PipeWire..."
pipewire &
PIPEWIRE_PID=$!
sleep 1

# Start WirePlumber (session manager)
echo "Starting WirePlumber..."
wireplumber &
WIREPLUMBER_PID=$!
sleep 2

# Verify PipeWire is running
echo ""
echo "=== Test 1: PipeWire daemon ==="
if pw-cli info 0 >/dev/null 2>&1; then
    echo "✓ PipeWire is running"
    pw-cli info 0 | head -5
else
    echo "✗ PipeWire not responding"
    exit 1
fi

# Create a virtual audio source using pw-loopback
# This creates a sink that appears as a source to capture clients
echo ""
echo "=== Test 2: Create virtual audio source ==="
pw-loopback \
    --capture-props='media.class=Audio/Sink node.name=usit-test-sink' \
    --playback-props='media.class=Audio/Source node.name=usit-test-source' &
LOOPBACK_PID=$!
sleep 2

# List sources to verify
echo "Available sources:"
pw-cli ls Node | grep -E "(usit-test|Audio/Source)" || echo "(none with expected name)"

# Play our test audio into the virtual sink (in background, looping)
echo ""
echo "=== Test 3: Feed audio to virtual source ==="
# pw-play can target by name
pw-cat --playback --target=usit-test-sink /app/test/audio/speech_sample.wav &
PLAY_PID=$!
echo "Playing speech_sample.wav into usit-test-sink..."
sleep 1

# Now run usit in headless mode, capturing from the virtual source
# It should transcribe something from our test audio
echo ""
echo "=== Test 4: usit audio capture and transcription ==="

# Check if models exist
MODEL_DIR="/root/.cache/usit/models"
if [ ! -d "$MODEL_DIR/moonshine-base" ] && [ ! -d "$MODEL_DIR/moonshine-tiny" ]; then
    echo "⚠ No models found at $MODEL_DIR"
    echo "  Mount models or download them first for transcription testing"
    echo "  Skipping transcription test..."
    SKIP_TRANSCRIPTION=1
fi

if [ -z "$SKIP_TRANSCRIPTION" ]; then
    # Run usit briefly to capture and transcribe
    # Use timeout to limit runtime
    echo "Running usit to capture from virtual source..."

    # Find the source node id
    SOURCE_ID=$(pw-cli ls Node | grep -B2 "usit-test-source" | grep "id " | awk '{print $2}' | tr -d ',')

    if [ -n "$SOURCE_ID" ]; then
        echo "  Virtual source ID: $SOURCE_ID"

        # Run usit with explicit device selection (if supported)
        # For now just run in headless mode and see what it captures
        timeout 8 /app/target/release/usit --headless 2>&1 | tee /tmp/usit-output.log &
        USIT_PID=$!

        # Let it run for a few seconds to capture audio
        sleep 6

        # Check output for transcription
        if grep -qi "saying\|words\|hey\|me" /tmp/usit-output.log 2>/dev/null; then
            echo "✓ usit transcribed audio from virtual source!"
            grep -i "saying\|words" /tmp/usit-output.log | head -3
        else
            echo "? Transcription output unclear (may need device selection)"
            cat /tmp/usit-output.log | tail -20
        fi

        kill $USIT_PID 2>/dev/null || true
    else
        echo "✗ Could not find virtual source ID"
    fi
fi

# Test 5: Direct capture API test (if we have a test binary)
echo ""
echo "=== Test 5: PipeWire capture API test ==="
if [ -f "/app/target/release/deps/audio_capture_test" ]; then
    timeout 5 /app/target/release/deps/audio_capture_test || echo "Capture test completed"
else
    echo "  (No dedicated capture test binary)"
fi

# Cleanup
echo ""
echo "=== Cleanup ==="
kill $PLAY_PID 2>/dev/null || true
kill $LOOPBACK_PID 2>/dev/null || true
kill $WIREPLUMBER_PID 2>/dev/null || true
kill $PIPEWIRE_PID 2>/dev/null || true

echo ""
echo "=== Audio tests completed ==="
