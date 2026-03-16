## Layout

- `nuxglit/`
  In-process Rust plus bare-Qt widgets experiment using `QOpenGLWidget` and
  `QOpenGLPaintDevice` as native GL-backed canvas inside a normal widget
  hierarchy, with rust code doing the opengl and C++ doing the standard qt
  rituals.

## Quick Run

`cd experiment/nuxglit && NUXGLIT_AUTOSTOP_MS=1500 QT_QPA_PLATFORM=offscreen LIBGL_ALWAYS_SOFTWARE=1 cargo run -- --demo`

## Remove me

`nuxglit` is the POC for a rustacean app with C++ bindings Qt since Qt is
not so agnostic as we might like it to be.

This shows how we can create:

- a native-feeling bare-Qt visualizer
- a Rust-owned hot data path
- a smaller, more disciplined graphical shell

in other words, it is a have-our-cake-and-eat-it-too POC

Once we are having and eating similar cake in the main usit application
this no longer has any utility except as git history and should be
removed from the repo
