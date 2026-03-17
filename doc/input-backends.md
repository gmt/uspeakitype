# Input Backends

This note captures the implementation contracts behind `usit`'s text injection path.

## Selection Order

`usit` probes backends in this order:

1. `input_method`
2. `fcitx5_bridge`
3. `wrtype`
4. `ydotool`
5. display-only mode

The first backend that initializes successfully becomes the injector for the session.

## Probe Semantics

Each backend probe ends in one of three states:

- `active`: backend initialized and can be selected
- `unavailable`: backend exists in theory but failed to initialize in the current environment
- `skipped`: backend was disabled by CLI or intentionally bypassed

If all probes fail, `usit` stays in display-only mode instead of pretending text injection works.

## TUI Exception

`input_method` is skipped in ANSI/TUI mode.

Reason: the Wayland input-method protocol is bound to a compositor-managed input-method object, and the current implementation expects the graphical path to own that lifecycle. TUI mode therefore falls through to `fcitx5_bridge`, `wrtype`, `ydotool`, or display-only behavior.

## `input_method` Contract

The `input_method` backend is the most compositor-native path and has the strictest behavior:

- It uses Smithay Client Toolkit reexports only; the implementation intentionally avoids a direct `wayland-client` dependency.
- Initialization proves protocol availability, not that a text field is currently focused.
- Startup performs a bounded three-roundtrip probe.
- If the compositor reports the input method as unavailable, the backend fails fast with an `another IME active` style error.
- Actual injection still requires runtime activation from the compositor; otherwise `inject()` returns `input method not activated`.
- Surrounding-text state is cached when the compositor provides it so later features can inspect cursor and context.
- Drop order matters: the input-method object is destroyed before the final Wayland flush to avoid leaving stale compositor-side IME state behind.

The practical distinction is important:

- `active` during probing means the protocol path initialized
- `activated` during injection means the currently focused field is ready to accept committed text

## `fcitx5_bridge`

`fcitx5_bridge` is the preferred fallback when fcitx5 is the active input-method framework. It speaks to the local addon over D-Bus and is a better KDE/fcitx5 story than dropping straight to keystroke synthesis.

Development posture:

- build the addon with `script/install-fcitx5-bridge-dev.sh`
- keep Plasma/KWin on the stock `Fcitx 5 Wayland Launcher`
- register the addon by writing a user-local `usitbridge.conf` whose `Library=` points at the absolute build artifact
- install a user-local `fcitx5-wayland-launcher.desktop` override so KDE's Virtual Keyboard list shows the `usit` icon without changing the launcher semantics

Production posture:

- install the addon into fcitx5's normal system addon directories
- avoid custom launcher wrappers or `FCITX_ADDON_DIRS` overrides

## `wrtype`

`wrtype` is the lightweight wlroots-oriented fallback. It is fast and simple, but only available on compositors that support the expected wlroots behavior.

## `ydotool`

`ydotool` is the universal escape hatch.

Socket discovery follows a strict three-step search:

1. `YDOTOOL_SOCKET`
2. `$XDG_RUNTIME_DIR/.ydotool_socket`
3. `/tmp/.ydotool_socket`

Initialization requires both:

- a working `ydotool` binary in `PATH`
- a reachable `ydotoold` socket

When `--autostart-ydotoold` is set, `usit` starts `ydotoold` before backend selection if no socket is found, waits briefly, and then resumes the normal probe chain.

## CLI Controls

`--backend-disable` accepts a comma-separated, case-insensitive list of backend names. The recognized names are:

- `input_method`
- `fcitx5_bridge`
- `wrtype`
- `ydotool`

Skipping every backend is a supported way to force display-only mode.

## Safety Rule

No injection backend is registered when transcription is unavailable.

This is a deliberate guardrail: if no model is active, the injector thread consumes messages but never claims a live input path.
