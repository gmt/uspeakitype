# Barbara - Streaming ASR with Live Revision

**Named for:** Greek "barbaros" - one who babbles unintelligibly ("bar bar bar"). Our job: make the babbling intelligible.

## Quick Reference

```bash
cargo check              # Type check without building
cargo build              # Debug build
cargo build --release    # Release build
cargo run                # Run debug binary
cargo run -- --headless  # Terminal-only mode (future)
cargo clippy             # Lint (no config - uses defaults)
cargo fmt                # Format (no config - uses defaults)
cargo test               # Run all tests
cargo test test_name     # Run single test
cargo test -- --nocapture  # Show println! output
```

## Architecture

```
src/
├── main.rs           # CLI entry (clap), orchestrates modules
├── audio/
│   ├── mod.rs        # Re-exports AudioCapture, SileroVad
│   ├── capture.rs    # PortAudio 16kHz mono capture
│   └── vad.rs        # Silero VAD for commit detection
├── backend/
│   ├── mod.rs        # Re-exports MoonshineStreamer
│   └── moonshine.rs  # ONNX streaming inference
└── ui/
    ├── mod.rs        # TranscriptState + re-exports
    ├── app.rs        # winit + Wayland layer shell
    └── renderer.rs   # WGPU + glyphon text rendering
```

## Core Concept: Streaming vs Batch ASR

Batch (Whisper): Wait for silence → transcribe → show all at once
Streaming (Barbara): Show words as spoken → revise earlier guesses → commit on silence

VAD role: **Commit detection**, not batching. We transcribe continuously; VAD signals when to finalize.

## Code Style

### Formatting
- **No rustfmt.toml** - use `cargo fmt` defaults
- **No clippy.toml** - use `cargo clippy` defaults
- 4 spaces, no tabs
- Max line length ~100 (soft limit)

### Naming
- `snake_case` for functions, methods, variables, modules
- `PascalCase` for types, traits, enums
- `SCREAMING_SNAKE` for constants
- Descriptive names; avoid abbreviations except common ones (vad, asr, ui)

### Imports
```rust
// Order: std, external crates, crate internals
use std::path::Path;

use anyhow::Result;
use clap::Parser;

use crate::audio::AudioCapture;
```

### Module Organization
- One `mod.rs` per directory with `pub mod` declarations
- `pub use` for convenient re-exports from `mod.rs`
- Module doc comments with `//!` at top of file

```rust
//! Silero VAD - Voice Activity Detection
//!
//! Key insight: VAD is for COMMIT DETECTION, not batching.
```

### Error Handling
- Use `anyhow::Result<T>` for fallible functions
- Use `?` for propagation
- Provide context with `.context("what failed")`

```rust
pub fn new(model_path: &Path) -> anyhow::Result<Self> {
    let session = Session::builder()
        .commit_from_file(model_path)
        .context("loading moonshine model")?;
    Ok(Self { session })
}
```

### Structs and Enums
```rust
pub struct MoonshineStreamer {
    // TODO: encoder, decoder, tokenizer
}

pub enum StreamEvent {
    /// Partial transcription (may be revised)
    Partial(String),
    /// Committed transcription (final for this phrase)
    Commit(String),
}
```

### Documentation
- Doc comments on public items: `///` for items, `//!` for modules
- Keep it brief; focus on *why*, not *what*
- Use `TODO:` comments for unimplemented parts

### Unimplemented Code
- Use `todo!("Port from sonori")` with context
- Keep skeleton structs/functions for architecture clarity

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `ort` | ONNX Runtime for ML inference (Moonshine, Silero) |
| `pipewire` | PipeWire audio capture (16kHz mono) |
| `wgpu` | GPU rendering for overlay |
| `glyphon` | Text rendering on WGPU |
| `winit` | Windowing (forked for Wayland layer shell) |
| `anyhow` | Error handling |
| `clap` | CLI argument parsing |
| `tokio` | Async runtime |

## Porting from Sonori

When implementing TODOs, reference sonori source:
- `sonori/src/audio_capture.rs` → `src/audio/capture.rs`
- `sonori/src/silero_audio_processor.rs` → `src/audio/vad.rs`
- `sonori/src/backend/moonshine/` → `src/backend/moonshine.rs`
- `sonori/src/ui/app.rs` → `src/ui/app.rs`
- `sonori/src/ui/text_renderer.rs` + `window.rs` → `src/ui/renderer.rs`

**Key simplifications from sonori:**
- No 29s chunking logic
- No multi-backend abstraction (Moonshine only)
- No buttons, scrollbar
- Simpler UI state: just `partial` and `committed` text
- Spectrogram included (simplified bar visualization)

## Testing

```bash
cargo test                        # All tests
cargo test moonshine              # Tests matching "moonshine"
cargo test --lib                  # Library tests only
cargo test --doc                  # Doc tests only
```

No tests exist yet. When adding:
- Unit tests in same file with `#[cfg(test)]` module
- Integration tests in `tests/` directory
- Use `#[test]` attribute

## Common Patterns

### Result handling with anyhow
```rust
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    do_something()?;
    Ok(())
}
```

### State struct pattern
```rust
pub struct TranscriptState {
    pub committed: String,
    pub partial: String,
}

impl TranscriptState {
    pub fn new() -> Self { ... }
    pub fn set_partial(&mut self, text: String) { ... }
    pub fn commit(&mut self) { ... }
}
```

### Re-export pattern in mod.rs
```rust
pub mod capture;
pub mod vad;

pub use capture::AudioCapture;
pub use vad::SileroVad;
```

## Phase 1 TODO (Current)

- [x] Port audio capture (PipeWire, 16kHz mono)
- [ ] Port Moonshine inference
- [x] Port minimal UI (layer shell + spectrogram + text)
- [ ] Wire streaming loop
- [x] Partial vs committed text rendering (two-tone)
