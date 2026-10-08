# Test environments

The default suite covers configuration, signal processing, model/cache validation,
mock injection, TUI layouts and image comparison. To keep backend probes away from
a running desktop, use an empty private runtime directory and no session bus:

```sh
test_runtime=$(mktemp -d)
env -u WAYLAND_DISPLAY -u DISPLAY -u USIT_CANONICAL_TEST_ENV \
  XDG_RUNTIME_DIR="$test_runtime" \
  DBUS_SESSION_BUS_ADDRESS="unix:path=$test_runtime/no-bus" \
  cargo test --all-targets
rmdir "$test_runtime"
cargo test --doc
```

TUI tests need a PTY and report a runtime skip if one cannot be allocated. Their
children run in demo mode with synthetic audio. Screenshot and input-method tests
need the separate environments below; a default-suite pass does not certify them.

## External requirements and skip audit

| Tests | Default behavior | Required environment |
| --- | --- | --- |
| `transcription` (2) | Explicitly ignored | Cached `moonshine-base` and `moonshine-tiny` ONNX files, plus the checked-in speech WAV. Run `cargo test --test transcription -- --ignored --nocapture`; missing/broken models fail an explicit run. |
| `audio_capture` (4) | Explicitly ignored | Isolated PipeWire daemon, virtual source continuously playing `tests/audio/speech_sample.wav`; full pipeline also needs `moonshine-tiny`. Set `USIT_TEST_SOURCE` to that virtual source. Never run against the desktop microphone. An explicit run fails on absent samples/models. |
| `input_method_tests` (5), `injection_flow::test_input_method_injector_creation` | Runtime skip outside canonical environment | Container-owned Sway with input-method-v2. Set `USIT_CANONICAL_TEST_ENV=1` only inside that isolated compositor. Run serially. |
| `visual_tests` screenshot cases | Runtime skip without supported compositor; noncanonical rendering differences are diagnostic | Container-owned Sway, grim, software Vulkan, fixed fonts and checked-in golden images. Canonical failures are fatal. Pure image comparison tests always run. |
| `tui_size_matrix`, `tui_harness` | Runtime skip if no PTY | Linux PTY allocation; no real audio or display required. |

The six explicitly ignored tests require external services or large model files;
none are ignored to conceal an assertion failure. Runtime skips mark capabilities
that the pure suite cannot supply. Run the applicable integration environment to
claim that capability works.

## Isolated compositor tests

```sh
docker compose run --build --rm visual-tests
docker compose run --build --rm ime-tests
docker compose run --build --rm audio-tests
```

These commands use a rootless Docker daemon and container-owned sockets. Neither
mounts host Wayland, D-Bus or PipeWire sockets. The IME tests are enabled tests;
passing `--ignored` would select zero tests. See `visual/README.md` for screenshot
comparison details. `audio-tests` mounts the model cache read-only, creates its own PipeWire virtual
source and loops the speech fixture into that source. It runs all four Rust
audio tests with `USIT_TEST_SOURCE` set; missing capture or transcription fails. The PTT capture test also verifies that
idle and manual pause admit no worker audio, a held key admits real frames, and
release delivers its finalization event before returning to idle.
