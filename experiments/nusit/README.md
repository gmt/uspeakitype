# nusit

`nusit` is the “C++ shell first” sketch.

The Qt executable is the thing a user launches. It owns the window, level meter,
and simple controls. A Rust helper process provides the live state over a tiny
line-oriented JSON protocol.

This is intentionally narrow and disposable:

- no real audio capture
- no model loading
- no input integration
- fake transcript/meter updates generated on the Rust side

The point is to feel the ergonomics of:

- C++ as the primary shell/integration language
- Rust as an auxiliary state engine
- a boring, inspectable wire protocol

See [../common/protocols.md](../common/protocols.md) for the shared crazyideas
baseline.

## Layout

- `rust_helper/` - Rust helper that emits snapshots and accepts commands
- `shell/` - Qt Widgets shell executable
- `build.sh` - convenience builder for both halves

## Wire Protocol

The Qt shell spawns the Rust helper and communicates over newline-delimited JSON.

Helper -> shell snapshots:

```json
{"level":0.42,"paused":false,"injection_enabled":true,"status":"Listening","transcript":"rough draft words appear here"}
```

Shell -> helper commands:

```json
{"type":"toggle_pause"}
{"type":"toggle_injection"}
{"type":"quit"}
```

Supported common-ish controls in this spike:

- `pause` / `resume` via `toggle_pause`
- `quit`

Explicitly not implemented here:

- `toggle_visualizer_mode`
- `set_gain`
- `source_next`
- `model_next`

## Build

```bash
./build.sh
```

This builds:

- `rust_helper/target/debug/nusit-rust-helper`
- `shell/build/nusit`

## Run

```bash
QT_QPA_PLATFORM=offscreen ./shell/build/nusit
```

Or with a visible desktop session:

```bash
./shell/build/nusit
```

The shell looks for the helper at:

```text
../rust_helper/target/debug/nusit-rust-helper
```

relative to the shell executable, unless you pass a custom helper path as the
first CLI argument.
