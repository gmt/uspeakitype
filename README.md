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
The Qt frontend additionally requires Qt 6 Core, Gui, Qml, Quick, Widgets, DBus and Test
libraries and development files, plus KF6GlobalAccel and LayerShellQt. Models and
Silero VAD use ONNX Runtime.

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
controls. The Qt transcript is a translucent layer-shell display on Wayland. Showing it
leaves keyboard focus in the destination app. Controls open a separate settings
window; only an explicit settings action requests focus.
Closing it hides the window in the tray and keeps dictation running. Click the
Tux tray icon to restore it, or use its listening and typing toggles. **Quit usit**
in the tray menu or **Ctrl+Q** stops the engine and microphone. Without a system
tray, closing the window exits.

In hold-to-talk mode the transcript follows dictation: pressing the hold key
shows it, and it hides into the tray 1.5 seconds after the phrase is typed.
Text that was not typed, for example with typing off, stays visible.

Only one untagged `--qt` session runs at a time. Launching usit again, from the
application menu or a launcher shortcut, shows the running session's transcript
and exits without loading another model. Use `--tag NAME` to run a deliberately
separate session.

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

## Global push-to-talk

On KDE Plasma, enable **Hold to talk** in Settings or the tray menu. Hold **F9**
to dictate and release it to finish the phrase. The shortcut works with the Qt
window hidden and with ANSI or WGPU selected. Typing still requires **Injecting**
to be enabled and a focused destination field.

Choose a different key or combination with **Hold key** in Settings, opened
from **Controls** or the tray menu. ANSI and WGPU provide F8/F9/F10 presets; CLI and configuration accept a
single Qt portable key sequence, including combinations such as `Ctrl+Alt+Space`.
Existing shortcuts belonging to other applications are not replaced; a conflict
appears as a setting error. Only one usit session may own push-to-talk at a time.

The hold key may also be usit's own KDE launcher shortcut (System Settings →
Shortcuts → usit). KDE launchers fire only on press, so they cannot carry
hold-to-talk themselves. Instead, the first press launches usit, and while
hold-to-talk is armed usit borrows the key from its launcher. Disabling
hold-to-talk, choosing another hold key or exiting gives the key back, so it
launches usit again. The launcher's saved binding is never rewritten. If usit is
killed before it can give the key back, the next usit session restores the
launcher. A launcher counts as usit's when its desktop file is `usit.desktop` or
its `Exec` runs a program named `usit`.

```sh
usit --push-to-talk --ptt-shortcut F9
usit --push-to-talk=false
```

The corresponding TOML fields are `push_to_talk = true` and `ptt_shortcut = "F9"`.
New configurations default to continuous listening. While push-to-talk is idle,
the PipeWire stream remains connected, but its buffers are discarded before
recognition. Release finalizes the admitted phrase without waiting for silence.
Boundaries are applied at audio callbacks, not sample-exact timestamps. Manual
pause or changing modes discards an unfinished phrase. A lost shortcut helper
closes the held gate and reports an error; restart usit to reconnect a crashed
helper. The helper attempts to register again when KDE’s shortcut service returns.

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
