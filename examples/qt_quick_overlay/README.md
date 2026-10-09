# Qt Quick dictation display and settings

The Qt Quick frontend displays live speech recognition from the Rust engine.
Its warm, translucent transcript display stays above ordinary windows without
taking keyboard focus. A separate Settings window provides microphone, model,
gain, hold-to-talk and appearance controls. The system tray provides quick
access while the transcript is hidden.

## Build and run

Requirements: CMake, a C++17 compiler, Qt 6 Core, Gui, Qml, Quick, Widgets, DBus
and Test development libraries, KDE Frameworks 6 GlobalAccel, and LayerShellQt.
KDE Plasma's global shortcut service supplies press/release events for
hold-to-talk. The engine's audio and model requirements are in the repository
README.

From the repository root:

```sh
./scripts/build-qt-overlay.sh
cargo run --release -- --qt
```

The helper is built at `target/qt-quick/usit-qt-overlay`. QML resources are
embedded in that executable, so rebuild after changing these files. Launch
through `usit --qt`: the helper expects a live engine on standard input.

For a microphone-free demonstration, use `cargo run -- --qt --demo`. Demo audio
and transcript come from the engine's explicit demo mode; the Qt frontend does
not synthesize speech, spectrum values, or transcript text.

## Focus and tray behavior

On Wayland, the transcript is a LayerShellQt overlay with keyboard interaction
disabled. It has no editable fields and never requests activation when shown or
restored from the tray. It leaves the application receiving your dictation
focused. On X11, its window flags prevent keyboard focus.

**Controls** on the display, or **Settings…** in the tray, explicitly opens a
normal focusable Settings window. Closing Settings leaves dictation running.
After changing settings, return focus to the application receiving dictation.
Clicking the tray icon toggles only the passive transcript display. **Hide** or
closing the display hides it when a tray is available; without a tray it quits
so an inaccessible session is not left running. **Quit usit** always ends the
session. If the tray disappears while the display is hidden, the display
returns without requesting focus.

## Controls

- **Hold to talk** recognizes speech while the configured global key is held.
  **Hold key** opens a shortcut editor accepting one key or key combination,
  such as `F9` or `Ctrl+Alt+Space`. The tray and transcript show whether usit is
  waiting for the key, dictating, or unable to register the key.
- **Listening / Paused** pauses or resumes continuous recognition; shortcut
  **P** in Settings. This control is disabled in hold-to-talk mode.
- **Injecting** types committed phrases into the focused application;
  shortcut **I** in Settings. A working model and input backend are required.
- **Waterfall / Bars** changes the audio visualization; shortcut **V**.
- **Automatic gain** lets capture manage microphone level. Manual software gain
  is disabled while automatic gain is active.
- **Input** cycles microphones. Selection takes effect on the next launch.
- **Model** opens a direct-selection list. Browse with arrow keys and select
  with Return, or click a model. Browsing does not download intermediate models.
  A missing selected model downloads before activation; **Cancel model download**
  cancels it while retaining the active model.
- **Colors** and **Window opacity** change the transcript's appearance.
- **Auto-save** persists explicit changes. **Save settings** or **Ctrl+S** saves
  when auto-save is disabled. Automatic gain adjustments do not change saved
  manual gain.
- **Ctrl+Q** in Settings or **Quit usit** in the tray ends the session.

Settings supports **Tab / Shift+Tab**, then **Space / Return** to activate.
Sliders use **Left / Right** and **Home / End**. Focused and hovered controls
show help at the bottom of the panel. Scrolling and keyboard focus keep controls
accessible in a short window.

The hold-key listener is independent of the graphical frontend, so the Rust
engine can use it with terminal and WGPU presentations too. Only one usit
session may own hold-to-talk on a session bus. An occupied shortcut produces a
readable error; usit does not take another application's binding. The exception
is usit's own launcher (`usit.desktop`, or any desktop entry whose `Exec`
program is `usit`). KDE launchers fire only on press, so while hold-to-talk is
armed the helper parks that launcher entry. kglobalacceld drops its grab but
keeps the configured binding, and the helper binds the key itself. Disabling,
rebinding or helper exit wakes the launcher. A helper that starts while no
session owns `org.uspeakitype.PushToTalk` wakes any usit launcher left parked
by a killed helper and removes its stale hold binding. Disabling hold-to-talk,
disconnecting the engine, or losing KDE's shortcut service releases a held key.
The helper unregisters its binding on shutdown.

## Frontend checks

```sh
python3 qt_quick_overlay/tests/smoke.py
python3 qt_quick_overlay/tests/hotkey.py
python3 qt_quick_overlay/tests/hotkey_launcher.py
```

The smoke test uses Qt's offscreen platform with software rendering. It checks
snapshot bindings, nonfocus/transparency flags, separate Settings lifecycle,
real mouse and keyboard activation, commands, malformed and fragmented input,
rendering, launch activation restoring a hidden transcript, hold-to-talk
auto-hide after typing, and exit when the engine pipe closes. To keep the display screenshot,
set `USIT_QT_TEST_SCREENSHOT=/tmp/usit-qt.png`. Set
`USIT_QT_LIVE_TRAY_TEST=1` only when deliberately testing the current desktop's
tray; the default smoke test stays offscreen.

The hotkey test uses simulated registration and activation to verify repeat
suppression and release on disable, service loss and stdin EOF. It also checks
the production helper's missing-service error with an unreachable session bus.
It does not register a shortcut on the current desktop. Actual compositor
focus and global-key delivery require a KDE/Wayland integration check.

The launcher test starts a private session bus and a standalone `kglobalacceld`
with temporary XDG directories and a usit launcher bound to F9. It checks that
the launcher is parked while hold-to-talk is armed and wakes on disable,
rebinding and exit. It also checks that another application's key is refused,
a killed helper's orphaned loan is recovered, and the saved launcher bindings
are left unchanged. It prints a skip when `kglobalacceld`, `dbus-daemon`,
`gdbus` or `kbuildsycoca6` is missing.

## Engine boundary

`qt_quick_overlay/bridge.h` exposes `usitBridge.state` to QML. Each newline JSON
object on stdin replaces that state. Controls call
`usitBridge.send(type, value)`; compact JSON commands go to stdout. Diagnostics
go to stderr. stdin is nonblocking, and EOF exits the helper. Rust owns model
activation, capture, gain, persistence, and text injection.

Snapshots carry an `activation` counter. An untagged `usit --qt` holds a lock and
socket under `$XDG_RUNTIME_DIR/usit/`; a later launch increments the running
session's counter and exits. The helper shows the hidden transcript when the
counter rises, without taking focus, like a tray click.

Snapshots also carry an `injected` counter of phrases typed into their
targets. In hold-to-talk mode the helper shows the transcript on key-down and
hides it into the tray shortly after `injected` rises; without a tray it stays.

The same executable accepts `--hotkey-only`, which creates neither QML windows
nor a tray icon. Its input snapshots contain `push_to_talk` and `ptt_shortcut`.
It emits `ptt_press`, `ptt_release`, and `hotkey_status` with `available`,
`shortcut`, and nullable `error` fields. Only this process registers the stable
KDE action `usit / hold_to_dictate`; the graphical process never registers it.
