# crazyideas

Disposable architecture spikes for answering "what if we changed the shell,
the bridge, or both?" without wrecking the real app.

These experiments are intentionally isolated from the main `cargo build`
graph. Each subdirectory has its own build instructions and makes different
tradeoffs around:

- who owns the main executable
- where audio lives
- how UI state crosses the Rust/C++ boundary
- how much Qt/Rust object-model pain we accept

## Layout

- `common/`
  Shared notes about wire formats and evaluation criteria.
- `nusit/`
  C++ shell as the main executable, with a Rust helper over a simple wire
  protocol and a trivial level meter.
- `nucit/`
  C++ shell plus audio I/O and level meter on the C++ side, with a narrower
  Rust worker.
- `shared_memory_bridge/`
  Qt shell and Rust helper communicating through a shared memory region instead
  of newline-delimited JSON over stdio.
- `nuxxit/`
  In-process Rust experiment using CXX-Qt.
- `hellnuxit/`
  In-process Rust experiment using raw C++ interop instead of a higher-level
  Qt bridge.

## Quick Runs

- `nusit`
  `cd experiments/nusit && ./build.sh && QT_QPA_PLATFORM=offscreen timeout 3s ./shell/build/nusit`
- `nucit`
  `cd experiments/nucit && ./build.sh && QT_QPA_PLATFORM=offscreen timeout 3s ./shell/build/nucit`
- `shared_memory_bridge`
  `QT_QPA_PLATFORM=offscreen ./experiments/shared_memory_bridge/run_demo.sh --auto-quit-ms 1500`
- `nuxxit`
  `cd experiments/nuxxit && QT_QPA_PLATFORM=offscreen timeout 3s env CARGO_HOME=/tmp/usit-cargo-home cargo run`
- `hellnuxit`
  `cd experiments/hellnuxit && HELLNUXIT_AUTOSTOP_MS=1200 QT_QPA_PLATFORM=offscreen env CARGO_HOME=/tmp/usit-cargo-home cargo run`

## What "done enough" means here

Each spike should answer at least these questions:

1. What is the process model?
2. What is the control/state protocol?
3. Where does audio live?
4. Where does the minimal visualizer live?
5. How awkward is the build/run loop in practice?

The code here is expected to be throwaway. The goal is to feel the direction,
not to prematurely bless one of these as production architecture.
