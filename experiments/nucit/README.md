# nucit

`nucit` pushes the fake “audio I/O” loop and level meter into the C++ side.

The Qt shell owns:

- the level meter
- a timer that pretends to be the audio callback
- sending lightweight frame summaries to Rust

The Rust worker is narrower than `nusit`: it does not originate the meter. It
only receives frame summaries, tracks a little worker state, and emits a text
snapshot back.

This is a sketch of the question:

> what if Qt/C++ owns the integration and realtime-ish UI loop, and Rust is just
> an analysis worker?

See [../common/protocols.md](../common/protocols.md) for the shared crazyideas
baseline.

## Layout

- `rust_worker/` - Rust worker for frame-summary ingestion
- `shell/` - Qt Widgets shell and fake audio/meter loop
- `build.sh` - convenience builder

## Wire Protocol

Shell -> worker commands:

```json
{"type":"audio_frame","level":0.31}
{"type":"toggle_pause"}
{"type":"quit"}
```

Worker -> shell snapshots:

```json
{"paused":false,"frames_seen":128,"analysis":"worker sees local audio frames from C++","advice":"C++ owns the meter; Rust only owns interpretation"}
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

## Run

```bash
QT_QPA_PLATFORM=offscreen ./shell/build/nucit
```

By default the shell looks for:

```text
../rust_worker/target/debug/nucit-rust-worker
```

relative to its own executable.
