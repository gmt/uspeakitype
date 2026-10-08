# usit — streaming dictation for Linux

**uspeakitype** ("you speak, I type") captures a PipeWire microphone, transcribes
speech locally, and can insert completed phrases into the focused application.
Provisional words update while you speak; voice activity detection decides when
to commit a phrase.

The same Rust engine serves three interfaces: a Qt Quick window (`--qt`), an
ANSI terminal (`--ansi`), and a WGPU Wayland overlay (the default). The Qt Quick
window displays the live transcript and spectrum and provides model, gain,
listening, injection, and appearance controls.

## Build and run

Requirements include Rust/Cargo, a C/C++ compiler, CMake, pkg-config, Clang for
bindgen, PipeWire development files, and the Wayland/XKB libraries used by WGPU.
The Qt frontend additionally requires Qt 6 Core, Gui, Qml, Quick and Test
libraries and development files. Models and Silero VAD use ONNX Runtime.

```sh
cargo build --release
./scripts/build-qt-overlay.sh
./target/release/usit --qt --no-inject

# Other interfaces and a microphone-free demonstration
./target/release/usit --ansi --no-inject
./target/release/usit --demo
./target/release/usit --qt --demo
```

`--no-inject` starts with typing disabled; the **Injecting** control enables it.
Demo mode never injects text. Focus the destination application after adjusting
controls. The Qt window is a normal tool window, not a layer-shell surface.
Closing it stops the Rust session and microphone capture.

For a user-local command and application menu entry, run
`./scripts/install-local-launcher.sh` after building. The launcher runs the
release executable with `--qt` and forwards additional arguments. It references
this checkout, which must remain at its current location.

## Audio, models, and settings

```sh
./target/release/usit --list-sources
./target/release/usit --qt --source SOURCE_NAME --auto-gain --no-inject
./target/release/usit --qt --model parakeet-tdt-0.6b-v3 --no-inject
```

Supported model identifiers are `moonshine-base`, `moonshine-tiny`,
`moonshine-tiny-{ar,zh,ja,ko,uk,vi}`, and `parakeet-tdt-0.6b-v3`. Model availability
and download success depend on the configured upstream files. A failed model
switch retains the working model. If no model can load, graphical and terminal
interfaces continue as audio visualizers with an error; input registration is
disabled. Model files live under `~/.cache/usit/models/` and are integrity checked.

The Qt model list selects a model directly; browsing does not initiate downloads.
Input-device changes take effect on the next launch. Automatic gain disables
manual gain control; its adjustments do not overwrite the saved manual gain.

Settings live in `~/.config/usit/usit.toml`. Runtime controls take precedence over
explicit command-line values, then configuration, then defaults. Auto-save
persists explicit control changes. With auto-save disabled, use **Save settings**
or **Ctrl+S** in Qt, or **S** in ANSI/WGPU. Command-line overrides alone do not
rewrite the configuration. See [Qt controls](examples/qt_quick_overlay/README.md)
for the complete keyboard and mouse interface.

## Typing through KDE and Fcitx 5

For a Plasma desktop using Fcitx 5, build the small usit addon:

```sh
./scripts/install-fcitx-bridge.sh
```

It requires Fcitx 5 development headers, CMake and a C++ compiler. The script
installs the library under `~/.local/lib/fcitx5/` and writes an addon descriptor
under the user data directory with its absolute library path. Restart Fcitx
through the desktop session, or log out and back in, to load it. usit does not
restart or replace the desktop's input-method process.

Fcitx must already be configured as the desktop input method. Plasma's Fcitx
settings page also requires the Fcitx configuration tool with its KDE module
(on Gentoo: `app-i18n/fcitx-configtool` with `kcm` enabled).

The addon commits text only to Fcitx's currently focused input context. With
`--qt` or `--ansi`, usit tries Fcitx first, followed by wrtype and ydotool. The
WGPU overlay additionally tries `zwp_input_method_v2` when the compositor offers
it. Protocol and application support vary; a visible transcript does not by
itself establish that injection is available.

`--backend-disable=input_method,fcitx5_bridge,wrtype,ydotool` disables every
injection backend. Individual names can be disabled to select a fallback.
`--autostart-ydotoold` requests `systemctl --user start ydotool.service` when its
socket is absent; it requires an existing user service and uinput permissions.
Without a usable backend, transcription remains visible and diagnostics explain
the failed probe.

## Validation and recognition comparisons

```sh
cargo check --locked
cargo test --all-targets
cargo test --doc
python3 qt_quick_overlay/tests/smoke.py
```

[Testing guidance](tests/README.md) describes safe isolation from the live desktop,
Docker compositor and audio tests, and the requirements for ignored integration
tests. Do not run input-delivery tests against an ordinary desktop session.

To compare cached models on the same recording:

```sh
cargo run --release --bin wav_stream -- \
  --input speech.wav --model parakeet-tdt-0.6b-v3 \
  --model-dir ~/.cache/usit/models/parakeet-tdt-0.6b-v3 --whole-file
```

Transcripts go to stdout and timing to stderr. The recording must be mono at
16 kHz. `--whole-file` uses each backend's existing decoder behavior, including
Parakeet's internal windows. Silence and phrase context affect accuracy, so use
the same recording and segmentation when comparing models.

## Acknowledgments

usit is basically a dumbed-down version of [**Sonori**](https://github.com/0xPD33/sonori) by 0xPD33, an MIT-licensed local AI-powered speech transcription application for Linux.

Without Sonori's example and code to steal from, usit likely would have been a prohibatively difficult undertaking and basically a non-project.

## License

MIT License - see [LICENSE](LICENSE) file for details.

Copyright (c) 2026 Greg Turner <gmt@be-evil.net>
