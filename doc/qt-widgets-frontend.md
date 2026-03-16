# Qt Widgets Frontend

`usit` now treats the graphical shell as a separate Qt Widgets process that talks to the Rust core over JSON lines on stdio.

## Current Shape

- Rust still owns:
  - audio capture
  - ASR/model lifecycle
  - injection backends
  - runtime config persistence
  - ANSI mode
- Qt Widgets now owns:
  - the default graphical shell
  - layout and control chrome
  - spectrogram-style visualization from Rust snapshots
  - control events sent back to Rust

## Why This Shape

It gives us a real Qt-based UI without forcing the Rust crate itself to become a Qt build or FFI project immediately.

The seam is deliberately simple:

- Rust writes state snapshots to the Qt process on `stdin`
- Qt writes control commands back to Rust on `stdout`

## Build And Run

The companion app lives in [`oldcrap/qt_widgets_overlay`](../oldcrap/qt_widgets_overlay) and is built by [`script/build-qt-overlay.sh`](../script/build-qt-overlay.sh).

Graphical `usit` now tries to build and launch that shell automatically.

## Legacy Path

The old WGPU overlay is still available behind the hidden `--wgpu-legacy` flag so visual regression tests and rollback stay possible during the migration.

That path should be treated as transitional.
