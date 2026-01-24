# Barbara - Streaming ASR with Live Revision

**Named for:** Greek "barbaros" - one who babbles unintelligibly ("bar bar bar"). Our job: make the babbling intelligible.

Barbara is a streaming speech-to-text overlay for Linux that transcribes audio in real-time with live word-by-word revision, providing both terminal (ANSI) and graphical (WGPU) interfaces.

## Features

- **Streaming transcription**: Words appear as you speak, not after pauses
- **Live revision**: Earlier words update as context improves accuracy
- **Dual interfaces**: Terminal (ANSI) mode and graphical overlay (Wayland layer shell)
- **Spectrogram visualization**: Real-time audio feedback with bars or waterfall display
- **Voice Activity Detection**: Silero VAD for commit detection
- **Moonshine-based**: Fast, efficient ONNX-based ASR
- **Control panel**: Runtime configuration of audio input, gain and AGC.

## Input Injection Backends

Barbara automatically selects the best available input injection backend for your compositor/desktop environment. The selection follows a fallback chain: **wrtype** → **kwtype** → **ydotool** → **display-only mode**.

### Backend Comparison

| Backend | Compositor/DE | Requirements | Limitations |
|---------|---------------|--------------|-------------|
| **wrtype** | wlroots-based (Sway, Hyprland, River, etc.) | None (built-in) | wlroots compositors only |
| **kwtype** | KDE Plasma | `kwtype` package | First use requires KDE authorization |
| **ydotool** | Any Linux compositor | `ydotool` package, `ydotoold` daemon, uinput permissions | Requires daemon setup |
| **display-only** | Any | None | No text injection (transcription display only) |

### Feature Degradation

All backends support UTF-8 and emoji when available. The table shows what happens as you move down the fallback chain:

| Feature | wrtype | kwtype | ydotool | display-only |
|---------|--------|--------|---------|--------------|
| Text injection | ✓ | ✓ | ✓ | ✗ |
| UTF-8 support | ✓ | ✓ | ✓ | N/A |
| Emoji support | ✓ | ✓ | ✓* | N/A |
| Setup required | None | Package install | Package + daemon | None |

*ydotool emoji support depends on application's input handling

### Backend Setup

#### wrtype (wlroots compositors)
No setup required - works out of the box on Sway, Hyprland, River, and other wlroots-based compositors.

#### kwtype (KDE Plasma)
```bash
# Arch Linux
yay -S kwtype

# Or build from source
git clone https://github.com/sporif/kwtype
cd kwtype && mkdir build && cd build
cmake .. && make && sudo make install
```

**First use**: KDE will show an authorization dialog - click "Allow" to grant permission.

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
# Force kwtype on KDE (skip wrtype)
barbara --backend-disable=wrtype

# Test display-only mode
barbara --backend-disable=wrtype,kwtype,ydotool

# Case-insensitive, whitespace-tolerant
barbara --backend-disable="WrType, KwType"
```

Valid backend names: `wrtype`, `kwtype`, `ydotool`

#### `--autostart-ydotoold`
Automatically start the ydotoold daemon if the socket is missing. Useful for systems where the daemon isn't running as a service.

```bash
barbara --autostart-ydotoold
```

The daemon will be spawned in the background. Barbara waits 500ms for the socket to appear, then continues with backend selection.

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

Barbara is basically a dumbed-down version of [**Sonori**](https://github.com/0xPD33/sonori) by 0xPD33, an MIT-licensed local AI-powered speech transcription application for Linux.

Without Sonori's example and code to steal from, Barbara likely would have been a prohibatively difficult undertaking and basically a non-project.

## License

MIT License - see [LICENSE](LICENSE) file for details.

Copyright (c) 2026 Greg Turner <gmt@be-evil.net>
