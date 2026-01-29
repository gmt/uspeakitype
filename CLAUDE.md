# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**usit** (uspeakitype - "you speak, I type") is a streaming speech-to-text overlay for Linux. Unlike batch ASR systems (Whisper), usit streams words as you speak and revises them in real-time as context improves accuracy. It provides both terminal (ANSI/Ratatui) and graphical (Wayland layer shell/WGPU) interfaces.

## Build Commands

```bash
cargo check              # Type check without building
cargo build              # Debug build
cargo build --release    # Release build
cargo run                # Run debug binary
cargo run -- --ansi      # Terminal UI mode
cargo run -- --demo      # Demo with synthetic audio (no mic)
cargo run -- --headless  # Text-only output
cargo clippy             # Lint (uses defaults, no clippy.toml)
cargo fmt                # Format (uses defaults, no rustfmt.toml)
cargo test               # Run all tests
cargo test test_name     # Run single test by name
cargo test -- --nocapture  # Show println! output
```

### Docker Tests

```bash
docker compose run visual-tests cargo test --release --test visual_tests -- --ignored --nocapture
docker compose run kde-tests      # KDE/fcitx5 injection tests
docker compose run audio-tests    # PipeWire audio capture tests
docker compose run kde-shell      # Interactive debugging shell
```

## Architecture

```
src/
├── main.rs              # CLI (clap), signal handling, orchestrates threads
├── audio/
│   ├── capture.rs       # PipeWire 16kHz mono capture, AGC
│   └── vad.rs           # Silero VAD for commit detection
├── backend/
│   ├── moonshine.rs     # ONNX streaming ASR (default)
│   └── nemo_transducer.rs  # Parakeet TDT multilingual backend
├── input/
│   ├── input_method.rs  # Wayland zwp_input_method_v2 (primary)
│   ├── fcitx5_bridge.rs # D-Bus to fcitx5 addon (KDE)
│   ├── wrtype.rs        # wlroots-only backend
│   ├── ydotool.rs       # Universal fallback
│   └── mod.rs           # Backend selection with fallback chain
├── ui/
│   ├── app.rs           # Winit + Wayland layer shell
│   ├── renderer.rs      # WGPU + glyphon text rendering
│   ├── terminal.rs      # Ratatui TUI (--ansi mode)
│   └── control_panel.rs # Settings UI (gain, AGC, model selector)
├── streaming.rs         # VAD + incremental transcription coordinator
├── config.rs            # TOML config (~/.config/usit/usit.toml)
└── download.rs          # Model downloading with progress
```

### Core Data Flow

1. **Audio capture** (capture.rs): PipeWire → 16kHz mono samples → optional AGC → channel
2. **Streaming loop** (streaming.rs): Samples → VAD → partial transcriptions; VAD silence→speech transition commits
3. **Transcription** (backend/): ONNX inference produces partial text, revised as context grows
4. **Text injection** (input/): Committed text → Wayland IME or fallback → target app
5. **UI** (ui/): Spectrogram + two-tone text (committed=white, partial=gray)

### Key Shared State

`AudioState` (in `ui/mod.rs`) is shared via `Arc<RwLock<>>`:
- `committed`: Finalized transcription
- `partial`: Live text that may revise
- `is_speaking`, `is_paused`, `auto_gain_enabled`, `current_gain`, etc.

### Text Injection Fallback Chain

`input_method` → `fcitx5_bridge` → `wrtype` → `ydotool` → display-only. Use `--backend-disable=X,Y` to skip backends.

## Key Design Principles

1. **Dual UX requirement**: Every control must work in both TUI and WGPU modes. No frob may exist in only one surface.

2. **Config precedence** (high to low): GUI → CLI → config file → defaults

3. **AGC vs manual gain**: When AGC is active, the gain slider becomes disabled (AGC owns it). Algorithm-driven changes never persist to config; only explicit user choices do.

4. **Graceful shutdown**: Signal handler sets `running: Arc<AtomicBool>` to false; all threads check this flag; injector thread joins on drop to ensure Wayland IME cleanup.

5. **Model hot-swap**: Can switch Moonshine/Parakeet during runtime via control panel channel.

## Testing Infrastructure

| Type | Location | Run with |
|------|----------|----------|
| Unit tests | `src/**/*.rs` (#[cfg(test)]) | `cargo test` |
| Integration | `tests/` | `cargo test` |
| Visual tests | `tests/visual_tests.rs` | Docker visual-tests |
| TUI matrix | `tests/tui_size_matrix.rs` | `cargo test tui_size` |
| Injection flow | `tests/injection_flow.rs` | `cargo test --test injection_flow` |
| Transcription | `tests/transcription.rs` | `cargo test --test transcription -- --ignored` |
| Audio capture | `tests/audio_capture.rs` | Docker audio-tests |
| KDE/fcitx5 | `docker/kde/test-kde.sh` | Docker kde-tests |

Docker provides headless Wayland (Sway/KWin) with llvmpipe software GPU rendering and PipeWire audio. Claims that "Wayland testing requires a display" or "WGPU needs GPU access" are invalid - Docker handles this.

## Versioning

- Bump patch version when a significant unit of work is complete (e.g., 0.2.7 → 0.2.8)
- Version lives in `Cargo.toml`
- Tag after committing: `./scripts/tag-version.sh`
- Push with tags: `git push` (followTags config handles it)

## Important Files

- `AGENTS.md`: Comprehensive developer reference (code style, anti-patterns, test patterns, policies)
- `docs/audio-input-policy.md`: Audio device selection, gain control, AGC behavior
- `docs/nemo-tdt-onnx-workflow.md`: Parakeet model export workflow
