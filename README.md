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
