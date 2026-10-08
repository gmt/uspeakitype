#!/bin/bash
set -euo pipefail

# Every socket and audio node belongs to this container. No hardware is mounted.
if [[ -z ${DBUS_SESSION_BUS_ADDRESS:-} ]]; then
    exec dbus-run-session -- "$0" "$@"
fi
pids=()
cleanup() {
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait || true
}
trap cleanup EXIT
pipewire > /tmp/pipewire.log 2>&1 &
pids+=("$!")
wireplumber > /tmp/wireplumber.log 2>&1 &
pids+=("$!")
for attempt in {1..50}; do
    if pw-cli info 0 >/dev/null 2>&1; then break; fi
    sleep .1
done
pw-cli info 0 >/dev/null
pw-loopback \
    --capture-props='media.class=Audio/Sink node.name=usit-test-sink' \
    --playback-props='media.class=Audio/Source node.name=usit-test-source' \
    > /tmp/loopback.log 2>&1 &
pids+=("$!")
sleep 2
# Loop the fixture while tests attach and detach, always targeting the virtual sink.
(while true; do pw-cat --playback --target=usit-test-sink /app/tests/audio/speech_sample.wav; done) \
    > /tmp/fixture.log 2>&1 &
pids+=("$!")
export USIT_TEST_SOURCE=usit-test-source
timeout 120 cargo test --release --test audio_capture -- --ignored --nocapture --test-threads=1
