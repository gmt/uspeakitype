# usit - Streaming ASR with Live Revision

**uspeakitype** ("you speak, I type") is a streaming speech-to-text overlay for Linux that transcribes audio in real-time with live word-by-word revision, providing both terminal (ANSI) and graphical (Qt) interfaces.

## Features

- **Streaming transcription**: Words appear as you speak, not after pauses
- **Live revision**: Earlier words update as context improves accuracy
- **Dual interfaces**: Terminal (ANSI) mode and a Qt-based graphical shell
- **Spectrogram visualization**: Real-time audio feedback with bars or waterfall display
- **Voice Activity Detection**: Silero VAD for commit detection
- **Moonshine-based**: Fast, efficient ONNX-based ASR, including the newer official `moonshine-tiny-{ar,zh,ja,ko,uk,vi}` flavors
- **Control panel**: Runtime configuration of audio input, gain and AGC.

The current graphical frontend is a Qt Widgets companion shell bridged over stdio; the older WGPU overlay remains available only as a hidden legacy/testing path.

## Model Support

`usit` currently supports:

- `moonshine-base`
- `moonshine-tiny`
- `moonshine-tiny-ar`
- `moonshine-tiny-zh`
- `moonshine-tiny-ja`
- `moonshine-tiny-ko`
- `moonshine-tiny-uk`
- `moonshine-tiny-vi`
- `parakeet-tdt-0.6b-v3`

Recent upstream audit notes live in [`doc/upstream-audit-2026-03.md`](doc/upstream-audit-2026-03.md).

## Input Injection Backends

usit automatically selects the best available input injection backend for your compositor/desktop environment. The selection follows a fallback chain: **input_method** → **fcitx5_bridge** → **wrtype** → **ydotool** → **display-only mode**.

### Backend Comparison

| Backend | Compositor/DE | Requirements | Limitations |
|---------|---------------|--------------|-------------|
| **input_method** | Wayland compositors with zwp_input_method_v2 (wlroots, KDE, GNOME*) | None (built-in) | May conflict with other IMEs (fcitx5, ibus); GNOME support varies |
| **fcitx5_bridge** | KDE Plasma (or anywhere fcitx5 is the active IME) | fcitx5-usit-bridge addon | Requires addon build/install |
| **wrtype** | wlroots-based (Sway, Hyprland, River, etc.) | None (built-in) | wlroots compositors only |
| **ydotool** | Any Linux compositor | `ydotool` package, `ydotoold` daemon, uinput permissions | Requires daemon setup |
| **display-only** | Any | None | No text injection (transcription display only) |

### Feature Degradation

All backends support UTF-8 and emoji when available. The table shows what happens as you move down the fallback chain:

| Feature | input_method | fcitx5_bridge | wrtype | ydotool | display-only |
|---------|--------------|---------------|--------|---------|--------------|
| Text injection | ✓ | ✓ | ✓ | ✓ | ✗ |
| UTF-8 support | ✓ | ✓ | ✓ | ✓ | N/A |
| Emoji support | ✓ | ✓ | ✓ | ✓* | N/A |
| Setup required | None | Addon install | None | Package + daemon | None |

*ydotool emoji support depends on application's input handling

### Backend Setup

#### input_method (Wayland compositors)
No setup required - works out of the box on compositors that support the `zwp_input_method_v2` protocol (wlroots-based compositors like Sway/Hyprland, KDE Plasma, and some GNOME configurations).

**Note**: If another IME (fcitx5, ibus) is already active, `input_method` will gracefully fall back to `fcitx5_bridge`, `wrtype`, or `ydotool`.

#### fcitx5_bridge (KDE / fcitx5 users)
For KDE Plasma users (or anyone using fcitx5 as their input method), the `fcitx5_bridge` backend injects text through fcitx5 via D-Bus using a small addon.

**Development build without root:**
```bash
./script/install-fcitx5-bridge-dev.sh
```

That script:

- builds `fcitx5-usit-bridge`
- writes `~/.local/share/fcitx5/addon/usitbridge.conf`
- points `Library=` at the absolute build artifact path
- reloads `fcitx5` if it is already running

This keeps development on the stock KDE/Plasma launcher path. No custom
`FCITX_ADDON_DIRS`, `.desktop`, or `kwinrc` hacks are required.

**Packaged/system install:**
```bash
cd fcitx5-usit-bridge
cmake -S . -B build -DCMAKE_INSTALL_PREFIX=/usr
cmake --build build
sudo cmake --install build
fcitx5 -r
```

**Dependencies** (Arch Linux):
```bash
sudo pacman -S fcitx5 fcitx5-qt fcitx5-gtk cmake
```

**Dependencies** (Debian/Ubuntu):
```bash
sudo apt install fcitx5 fcitx5-modules fcitx5-modules-dev libfcitx5core-dev cmake
```

**How it works**: The addon exposes a D-Bus interface that usit calls to inject text. For development, `usit` and the helper script keep a user-local addon `.conf` pointed at the built bridge library. For packaged installs, fcitx5 finds the addon through its normal system addon directories.

**Recommended: Disable fcitx5's clipboard addon**

fcitx5 includes a clipboard monitoring addon that can conflict with KDE's clipboard handling (klipper), potentially causing plasmashell crashes in a loop. If you experience repeated plasmashell crashes or see single-character entries flooding klipper's history, disable the fcitx5 clipboard addon:

```bash
# Edit ~/.config/fcitx5/config and set:
# DisabledAddons=clipboard
# in the [Behavior] section

# Or use sed:
sed -i 's/^DisabledAddons=.*/DisabledAddons=clipboard/' ~/.config/fcitx5/config

# Restart fcitx5
pkill fcitx5 && fcitx5 -d
```

This removes fcitx5 from clipboard monitoring, leaving only klipper to manage clipboard state.

#### wrtype (wlroots compositors)
No setup required - works out of the box on Sway, Hyprland, River, and other wlroots-based compositors.

#### ydotool (universal fallback)
```bash
# Arch Linux
sudo pacman -S ydotool

# Debian/Ubuntu
sudo apt install ydotool

# Start the daemon (user service)
systemctl --user enable --now ydotool

# Or manually
ydotoold &
```

**Permissions**: Ensure your user has access to `/dev/uinput` (usually via `input` or `uinput` group).

### CLI Flags

#### `--backend-disable=BACKEND[,BACKEND...]`
Skip specific backends during selection. Useful for testing or working around issues.

```bash
# Skip input_method (try fcitx5_bridge or wrtype)
usit --backend-disable=input_method

# Force ydotool (skip all other backends)
usit --backend-disable=input_method,fcitx5_bridge,wrtype

# Test display-only mode
usit --backend-disable=input_method,fcitx5_bridge,wrtype,ydotool

# Case-insensitive, whitespace-tolerant
usit --backend-disable="Input_Method, Fcitx5_Bridge, WrType, YdoTool"
```

Valid backend names: `input_method`, `fcitx5_bridge`, `wrtype`, `ydotool`

#### `--autostart-ydotoold`
Automatically start the ydotoold daemon if the socket is missing. Useful for systems where the daemon isn't running as a service.

```bash
usit --autostart-ydotoold
```

The daemon will be spawned in the background. usit waits 500ms for the socket to appear, then continues with backend selection.

### Troubleshooting

**"Input injection: unavailable (display-only mode)"**
- All backends failed to initialize
- Check compositor type and install appropriate backend
- Run with `--backend-disable=` to see probe results

**"ydotoold not running"**
- Start daemon: `systemctl --user start ydotool` or `ydotoold &`
- Or use `--autostart-ydotoold` flag
- Check socket exists: `ls -la $XDG_RUNTIME_DIR/.ydotool_socket`

**KDE authorization prompt every time**
- Click "Remember" when granting permission
- Check KDE settings: System Settings → Applications → Launch Feedback

**Text not appearing in application**
- Ensure application window has focus
- Some applications (terminals, password fields) may block injection
- Try different backend with `--backend-disable`

## Quick Start

```bash
# Build and run
cargo build --release
cargo run --release

# Terminal-only mode
cargo run --release -- --ansi

# Demo mode (synthetic audio)
cargo run --release -- --demo
```

## Acknowledgments

usit is basically a dumbed-down version of [**Sonori**](https://github.com/0xPD33/sonori) by 0xPD33, an MIT-licensed local AI-powered speech transcription application for Linux.

Without Sonori's example and code to steal from, usit likely would have been a prohibatively difficult undertaking and basically a non-project.

## License

MIT License - see [LICENSE](LICENSE) file for details.

Copyright (c) 2026 Greg Turner <gmt@be-evil.net>
