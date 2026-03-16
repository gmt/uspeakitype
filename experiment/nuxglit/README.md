# nuxglit

`nuxglit` is a bare-Qt widgets spike for the question:

- can a native Qt visual hierarchy host an OpenGL-backed canvas cleanly,
- while Rust stays in charge of hot data generation,
- without turning the canvas path into stringly IPC theater?

This experiment uses:

- Rust as the producer for either live PipeWire audio or a demo signal
- a tiny raw C ABI bridge
- `QOpenGLWidget` as the native Qt canvas host
- `QOpenGLPaintDevice` so the canvas can be painted with `QPainter` on top of the
  current OpenGL context

It is intentionally small and disposable. The point is to learn whether this seam feels
promising, not to establish a permanent frontend.

## Run

```bash
cargo run
```

Optional auto-exit:

```bash
NUXGLIT_AUTOSTOP_MS=1500 cargo run
```

Demo fallback:

```bash
cargo run -- --demo
```

Specific source:

```bash
cargo run -- --source "Built-in Audio Analog Stereo"
```

## What it demonstrates

- plain Qt Widgets can host a GL-backed canvas in a normal hierarchy
- Rust can feed the hot path without a text protocol
- the hot numeric path can collapse into one fixed frame struct instead of
  multiple per-frame calls
- the same shell can run against a real audio capture path or a synthetic demo
- `QOpenGLPaintDevice` is a plausible truffle patch if the future shell stays bare Qt

## What it does not demonstrate

- zero-copy shared buffers
- real audio transport
- KDE integration
- Qt Quick / scenegraph embedding
