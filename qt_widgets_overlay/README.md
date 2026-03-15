# Qt Widgets Overlay

This is the real graphical frontend companion for `usit`.

It is intentionally built as a separate Qt Widgets process instead of being embedded directly into the Rust build:

- Rust owns the speech/input/config state
- this Qt app owns shell layout and user interaction
- snapshots come in on `stdin`
- control commands go back out on `stdout`

Build manually with:

```bash
./scripts/build-qt-overlay.sh
```

The main `usit` binary now attempts to build and launch this shell automatically for the default graphical path.
