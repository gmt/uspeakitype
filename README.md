# usit - Streaming ASR with Live Revision

**uspeakitype** ("you speak, I type") is a streaming speech-to-text overlay for Linux that transcribes audio in real-time with live word-by-word revision, providing both terminal (ANSI) and graphical (WGPU) interfaces.

## Features

- **Streaming transcription**: Words appear as you speak, not after pauses
- **Live revision**: Earlier words update as context improves accuracy
- **Dual interfaces**: Terminal (ANSI) mode and graphical overlay (Wayland layer shell)
- **Spectrogram visualization**: Real-time audio feedback with bars or waterfall display
- **Voice Activity Detection**: Silero VAD for commit detection
- **Moonshine-based**: Fast, efficient ONNX-based ASR
- **Control panel**: Runtime configuration of audio input, gain and AGC.

## Input Injection Backends

usit automatically selects the best available input injection backend for your compositor/desktop environment. The selection follows a fallback chain: **input_method** → **wrtype** → **ydotool** → **display-only mode**.

### Backend Comparison

| Backend | Compositor/DE | Requirements | Limitations |
|---------|---------------|--------------|-------------|
| **input_method** | Wayland compositors with zwp_input_method_v2 (wlroots, KDE, GNOME*) | None (built-in) | May conflict with other IMEs (fcitx5, ibus); GNOME support varies |
| **wrtype** | wlroots-based (Sway, Hyprland, River, etc.) | None (built-in) | wlroots compositors only |
| **ydotool** | Any Linux compositor | `ydotool` package, `ydotoold` daemon, uinput permissions | Requires daemon setup |
| **display-only** | Any | None | No text injection (transcription display only) |

### Feature Degradation

All backends support UTF-8 and emoji when available. The table shows what happens as you move down the fallback chain:

| Feature | input_method | wrtype | ydotool | display-only |
|---------|--------------|--------|---------|--------------|
| Text injection | ✓ | ✓ | ✓ | ✗ |
| UTF-8 support | ✓ | ✓ | ✓ | N/A |
| Emoji support | ✓ | ✓ | ✓* | N/A |
| Setup required | None | None | Package + daemon | None |

*ydotool emoji support depends on application's input handling

### Backend Setup

#### input_method (Wayland compositors)
No setup required - works out of the box on compositors that support the `zwp_input_method_v2` protocol (wlroots-based compositors like Sway/Hyprland, KDE Plasma, and some GNOME configurations).

**Note**: If another IME (fcitx5, ibus) is already active, `input_method` will gracefully fall back to `wrtype` or `ydotool`.

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
# Skip input_method (use wrtype)
usit --backend-disable=input_method

# Force ydotool (skip input_method and wrtype)
usit --backend-disable=input_method,wrtype

# Test display-only mode
usit --backend-disable=input_method,wrtype,ydotool

# Case-insensitive, whitespace-tolerant
usit --backend-disable="Input_Method, WrType, YdoTool"
```

Valid backend names: `input_method`, `wrtype`, `ydotool`

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
