# hellnuxit

`hellnuxit` is the "fine, what if we just did the hard thing?" spike.

It keeps a Rust `main()` but uses raw C++ interop for the Qt shell instead of a
higher-level bridge framework. The Qt side is deliberately tiny: one window,
one level meter, one line of status text.

This sketch is meant to answer:

- how ugly is the build loop if Cargo owns the app and compiles Qt C++ by hand?
- how annoying is manual ABI design compared with CXX-Qt?
- how much state can we move across the boundary before it stops being cute?

See also:

- [experiments/common/protocols.md](/home/greg/src/usit/experiments/common/protocols.md)
- [doc/crazyideas.md](/home/greg/src/usit/doc/crazyideas.md)

## Process model

Single process:

1. Rust starts first and spawns a tiny background loop producing fake meter data.
2. Rust calls raw `extern "C"` functions implemented in C++.
3. The C++ side owns the Qt event loop and a minimal meter window.

There is no separate helper process and no QObject/QML bridge layer.

## Boundary shape

The boundary is intentionally manual and tiny:

- `hellnuxit_set_level(float)`
- `hellnuxit_set_status(const char*)`
- `hellnuxit_request_quit()`
- `hellnuxit_run()`

This is not meant to be a good production ABI. It is meant to make the manual
interop burden tangible.

## Build

```bash
cd experiments/hellnuxit
env CARGO_HOME=/tmp/usit-cargo-home cargo run
```

For a bounded smoke run:

```bash
cd experiments/hellnuxit
HELLNUXIT_AUTOSTOP_MS=1200 QT_QPA_PLATFORM=offscreen \
  env CARGO_HOME=/tmp/usit-cargo-home cargo run
```

## Expected rough edges

- The UI is QWidget-based and intentionally primitive.
- Strings cross the ABI as UTF-8 `char*` and are copied immediately on the C++
  side.
- This build style is honest about how quickly "just a thin C++ shim" grows a
  bespoke build system.
