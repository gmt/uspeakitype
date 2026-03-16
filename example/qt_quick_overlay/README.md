# Qt Quick Overlay Concept

This is a standalone Qt Quick mockup for a possible `usit` overlay shell.

It is intentionally **not** wired into the Rust build. The goal is to explore
what happens if we let Qt Quick own the container chrome while keeping the
actual spectrogram/transcript semantics recognizable.

## What It Tries To Test

- a clearer separation between spectrogram surface and transcript panel
- a compact status/header strip instead of bespoke text painting
- a control drawer that feels like UI rather than debug geometry
- whether a Qt-hosted shell could reduce the amount of hand-rolled overlay code

## What It Is Not

- not a framework migration
- not a real Wayland/KWin integration path
- not proof that Qt Quick should replace the current renderer
- not connected to live audio or live transcript state

## Spectrogram Bridge Idea

The key seam is the `SpectrogramViewport` item.

In a real experiment, that one item could be backed by one of three things:

1. a native child surface owned by the existing Rust/WGPU renderer
2. a texture provider fed by a Rust-side renderer
3. a fresh Qt Quick / ShaderEffect reimplementation

This mockup keeps that seam visible on purpose.

## Running

If you have Qt Quick tooling installed:

```bash
qml example/qt_quick_overlay/Main.qml
```

Or:

```bash
qmlscene example/qt_quick_overlay/Main.qml
```
