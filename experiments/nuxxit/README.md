# nuxxit

`nuxxit` is the smallest in-process sketch of "what if we accepted the Qt
object model and stayed mostly Rust anyway?"

It is intentionally tiny:

- Rust main executable
- CXX-Qt-generated QObject
- QML/Qt Quick window
- fake single-model meter data
- no input integration
- no control panel
- no terminal surface

This spike exists to answer: how much ceremony does CXX-Qt buy us, and how
pleasant is that ceremony compared with a subprocess bridge?

See also:

- [experiments/common/protocols.md](/home/greg/src/usit/experiments/common/protocols.md)
- [doc/crazyideas.md](/home/greg/src/usit/doc/crazyideas.md)

## Process model

Single process:

1. Rust `main()` creates the Qt application and QML engine.
2. A CXX-Qt-generated QObject exposes a few properties and an invokable `tick()`
   method to QML.
3. QML drives a small timer that asks Rust to advance the fake level meter.

There is no external helper process and no wire protocol; the interop boundary
is generated C++/Rust glue inside the same address space.

## Control/state model

This spike deliberately avoids a serious control surface. State is just:

- `level`
- `peak`
- `status`
- `model_label`
- `waterfallish`

QML calls `tick()` on a short timer, Rust mutates the QObject properties, and
the scene redraws.

## Build

```bash
cd experiments/nuxxit
env CARGO_HOME=/tmp/usit-cargo-home cargo run
```

For headless/smoke checks:

```bash
cd experiments/nuxxit
QT_QPA_PLATFORM=offscreen env CARGO_HOME=/tmp/usit-cargo-home cargo run
```

## Expected rough edges

- The visualizer is intentionally fake.
- The QML timer is doing the driving, which is nice for a toy and not yet a
  serious architecture claim.
- CXX-Qt introduces a real codegen/build layer; this spike is meant to make
  that friction tangible.
