# Shared Memory Bridge

Disposable sketch of the current Qt/Rust split with a shared memory region
instead of newline-delimited JSON over stdio.

The point is not to be clever. The point is to get a tactile sense for:

- a fixed-layout cross-language state block
- polling vs push tradeoffs
- how ugly command return-paths feel without a stream
- whether the data model wants to stay POD-friendly

This slice follows the shared branch scaffold in
[experiments/README.md](/home/greg/src/usit/experiments/README.md) and the
common protocol guidance in
[experiments/common/protocols.md](/home/greg/src/usit/experiments/common/protocols.md).

## Layout

- `rust_helper/`: Rust process that owns fake engine state and writes snapshots
- `qt_shell/`: Qt Widgets shell that polls the shared memory block and writes
  commands back into it
- `protocol.md`: binary layout and command semantics
- `build.sh`: build both sides
- `run_demo.sh`: launch both sides against the same shared memory name

## Build

```bash
./experiments/shared_memory_bridge/build.sh
```

This builds:

- `experiments/shared_memory_bridge/rust_helper/target/debug/usit-shm-helper`
- `experiments/shared_memory_bridge/qt_shell/build/usit-shm-shell`

## Run

```bash
./experiments/shared_memory_bridge/run_demo.sh
```

Useful options:

```bash
QT_QPA_PLATFORM=offscreen \
./experiments/shared_memory_bridge/run_demo.sh --auto-quit-ms 1500
```

Pass a stable shared memory name if you want to run the helper and shell
manually:

```bash
./experiments/shared_memory_bridge/rust_helper/target/debug/usit-shm-helper --shm-name /usit-shm-demo
./experiments/shared_memory_bridge/qt_shell/build/usit-shm-shell --shm-name /usit-shm-demo
```

## Rough conclusions to look for

- snapshots are easy when the state stays fixed-size and boring
- commands are more awkward than the stdio version because we need a shared
  “mailbox” discipline
- text fields and variable-length data immediately make the layout feel more
  rigid than the JSON bridge

That is probably the real value of this experiment.
