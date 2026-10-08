# Qt Quick dictation window

The Qt Quick frontend displays live speech recognition from the Rust engine.
It provides committed and provisional transcript text, a frequency spectrum or
scrolling waterfall, and microphone, model, gain and appearance controls. Its
warm dark window stays above ordinary windows and can be resized.

## Build and run

Requirements: CMake, a C++17 compiler, and Qt 6 Core, Gui, Qml, Quick, Widgets and Test
libraries with their development files. The speech engine's dependencies and
model requirements are described in the repository README.

From the repository root:

```sh
./scripts/build-qt-overlay.sh
cargo run --release -- --qt
```

The helper is built at `target/qt-quick/usit-qt-overlay`. QML resources are
embedded in that executable, so rebuild it after changing these files. Launch
through `usit --qt`: the helper alone expects a live engine on standard input.

For a microphone-free demonstration, use `cargo run -- --qt --demo`. Demo audio
and transcript come from the engine's explicit demo mode; the Qt frontend does
not synthesize speech, spectrum values, or transcript text.

## Controls

- **Listening / Paused** pauses or resumes recognition; shortcut **P**.
- **Injecting** enables typing committed phrases into the focused application;
  shortcut **I**. A working speech model and input backend are required.
- **Waterfall / Bars** changes the audio visualization; shortcut **V**.
- **Controls** opens or closes settings; shortcut **C**.
- **Automatic gain** lets capture manage microphone level. Manual software gain
  is disabled while automatic gain is active.
- **Input** cycles microphones. Selection takes effect on the next launch.
- **Model** opens a list for direct selection. Browse with arrow keys and select
  with Return, or click a model. Browsing does not download intermediate models.
  A missing selected model downloads before activation; **Cancel model download**
  cancels that download while retaining the active model.
- **Colors** and **Window opacity** change appearance.
- **Auto-save** persists explicit setting changes. **Save settings** or
  **Ctrl+S** saves when auto-save is disabled. Algorithmic gain changes do not
  change the saved manual gain.
- The **tray icon** uses the Tux headset artwork. Click it to show or hide the
  window; its menu also controls **Listening**, **Type into focused application**,
  and **Quit usit**. The tooltip reports the active model and listening/typing state.
- Closing the window hides it in the tray while dictation continues. If no tray is
  available, closing ends the session. **Ctrl+Q** or **Quit usit** always stops it.
  If the tray host disappears while the window is hidden, the window reappears.

Click controls or use **Tab / Shift+Tab**, then **Space / Return** to activate.
Sliders use **Left / Right** and **Home / End**. Focused and hovered controls
show help at the bottom of the settings panel. When the window is short, scroll
settings with the mouse; keyboard focus scrolls the active control into view.

The window has no editable text field. After adjusting settings, focus the
application where you want dictation inserted. Desktop focus behavior is owned
by the compositor; this is a Qt tool window, not a Wayland layer-shell surface.

## Frontend checks

```sh
python3 qt_quick_overlay/tests/smoke.py
```

The bounded test uses Qt's offscreen platform with software rendering. It checks
live snapshot bindings, real mouse and keyboard activation, outgoing command
values, malformed and fragmented input, rendering, and automatic exit when the
engine pipe closes. To additionally check the native desktop tray's close/hide,
restore, menu actions and quit lifecycle without capturing audio or injecting text,
run `USIT_QT_LIVE_TRAY_TEST=1 python3 qt_quick_overlay/tests/smoke.py`.
To keep the rendered screenshot, set
`USIT_QT_TEST_SCREENSHOT=/tmp/usit-qt.png`.

## Engine boundary

`qt_quick_overlay/main.cpp` exposes `usitBridge.state` to QML. Each newline JSON
object on stdin replaces that state. User controls call
`usitBridge.send(type, value)`; compact JSON command objects are emitted on
stdout. Diagnostics go to stderr. stdin is nonblocking, and EOF exits the
helper. Rust owns model activation, audio capture, gain, settings persistence,
and text injection; the window only displays their state and sends user intent.
