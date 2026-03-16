# experiments

Only `nuxglit` remains here as a living spike.

The rest of the architecture zoo has been retired on branch history. `nuxglit`
survives because it is the one experiment that still appears likely to inform
the real app.

## Layout

- `nuxglit/`
  In-process Rust plus bare-Qt widgets experiment using `QOpenGLWidget` and
  `QOpenGLPaintDevice` for a native GL-backed canvas inside a normal widget
  hierarchy, with a single fixed-frame handoff and optional live audio capture.

## Quick Run

`cd experiments/nuxglit && NUXGLIT_AUTOSTOP_MS=1500 QT_QPA_PLATFORM=offscreen LIBGL_ALWAYS_SOFTWARE=1 cargo run -- --demo`

## Why Keep It

`nuxglit` is the surviving reference for:

- a native-feeling bare-Qt visualizer
- a Rust-owned hot data path
- a smaller, more disciplined graphical shell than the current `usit`
