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

## Anti-patterns (Forbidden)

| Anti-pattern | Rationale | Reference |
|--------------|-----------|-----------|
| Multi-backend abstraction | Moonshine-only by design; no backend trait/plugin system | `src/backend/mod.rs:1-13` |
| Single-surface frobs | Every control MUST work in both TUI and WGPU; if terminal can't represent it, reconsider | UI/Config Principles |
| 29s chunking logic | Removed from sonori; continuous streaming model instead | Porting from Sonori |
| Persistent AGC adjustments | Algorithm-driven gain changes don't save to config; only explicit user choices persist | UI/Config Principles |
| AGC vs manual gain conflict | When AGC active, gain slider becomes disabled or "aggressiveness" control; no fighting user | UI/Config Principles |

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

### Test Patterns

| Pattern | Purpose | Location |
|---------|---------|----------|
| Synthetic signal generation | AGC convergence tests | `src/audio/capture.rs:645-865` |
| Toy ONNX models | Moonshine tests without full model | `src/backend/moonshine.rs:784` |
| State machine tests | AudioState transitions (partial→commit→committed) | `src/ui/mod.rs:104-209` |
| Thread safety tests | SharedAudioState concurrent access | `src/ui/mod.rs` |

### Test Utilities

- `generate_sine(freq, sample_rate, num_samples, amplitude)` - synthetic audio for AGC tests
- `tempfile::TempDir` - temporary directories for download tests
- Epsilon comparisons: `(value - expected).abs() < threshold` for float assertions
- `rms(&samples)` - RMS calculation for audio verification

When adding:
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

## UI/Config Principles

### Dual UX Requirement
Every user-facing control ("frob") MUST have both:
- **Terminal UI** (ANSI): works in `--ansi` mode
- **Graphical UI** (WGPU): works in overlay mode

No frob may exist in only one surface. If it can't be represented in terminal, reconsider whether it belongs.

### Four Control Surfaces (Precedence Order)
Settings can come from multiple sources. Higher precedence wins:

1. **GUI** (runtime changes in control panel)
2. **Command line** (`--flag value`)
3. **Config file** (`~/.config/barbara/barbara.toml`)
4. **Defaults** (compiled-in)

Use a unified config framework (e.g., `config-rs` + `clap`) to keep these in sync. Avoid hand-rolling precedence logic per-setting.

### Config Persistence
- **Auto-save mode** (checkbox in control panel): each user change persists immediately to config file
- **Manual mode**: control panel has OK/Save/Cancel buttons
- AGC-mediated changes (the algorithm adjusting gain) do NOT persist; only explicit user choices do

### Control Panel Contents (Current Spec)
- Input device selector (dropdown/list)
- Software gain slider (disabled or becomes "aggressiveness" when soft AGC enabled)
- Hardware gain slider (disabled or becomes "aggressiveness" when hard AGC enabled)
- Soft AGC checkbox
- Hard AGC checkbox
- Auto-save checkbox
- Description panel: 1-2 sentence help text for focused/hovered control

### Frob Behavior When AGC Active
When AGC controls a gain slider, that slider either:
- Becomes disabled (AGC owns it), OR
- Transforms into an "AGC aggressiveness" control

User should not see AGC fighting their manual adjustments.

### Future Control Panel TODOs
- [ ] Monitoring/metering UI (levels, clipping indicator, noise floor)
- [ ] "Wrong mic" detection warning
- [ ] Push-to-talk vs always-on mode toggle

## Phase 1 TODO (Current)

- [x] Port audio capture (PipeWire, 16kHz mono)
- [x] Port Moonshine inference
- [x] Port minimal UI (layer shell + spectrogram + text)
- [x] Wire streaming loop
- [x] Partial vs committed text rendering (two-tone)

### Policies

## All state controls should have four control surfaces, in order of precedence:
- gui (ansi/tui control panel), or, equivalently,
- gui (opengl control panel)
- cmdline
- ~/.config/barbara/barbara.conf

## try to keep tui layout and opengl layout as analogous as possible; switching between the two should feel natural

## work is not done until
- all skip tests are audited to ensure the reasons they were marked out is still valid
- all tests are either passing or marked as skip with documentation as to why it couldn't be fixed
- all enabled tests represent plausible future regressions or test core functionality
- all enabled tests pass
- a build succeeds and advice from rust is either implemented or the compiler is appeased
- the executable runs in ansi and opengl demo mode without crashing
- the testing will continue until morale improves! but if you are sure it's a dead-end test, document why and mark it as a skip.

## Agent Execution Patterns

### Visual Agent Observation (tmux environments)

When running in a tmux-enabled environment (oh-my-opencode with `tmux.enabled: true`), subagents spawn in **visible tmux panes**. This provides real-time observability of agent work.

**Benefits:**
- See agents thinking and working in parallel
- Monitor progress without waiting for completion
- Debug issues by watching agent output live
- Understand what agents are actually doing

**How it works:**
- `delegate_task()` calls automatically spawn new tmux panes
- Panes arrange in a grid (main pane left, agents right)
- Panes auto-close when agent completes or times out
- Multiple agents can run in parallel with visible progress

**Best Practices:**

1. **Prefer parallel exploration**: When gathering information, fire multiple explore/librarian agents simultaneously:
   ```
   delegate_task(subagent_type="explore", prompt="Find X...", run_in_background=true)
   delegate_task(subagent_type="explore", prompt="Find Y...", run_in_background=true)
   delegate_task(subagent_type="librarian", prompt="Lookup Z...", run_in_background=true)
   ```
   You'll see 3 panes appear and work in parallel.

2. **Use interactive_bash for TUI apps**: For apps needing ongoing interaction (vim, htop, debuggers):
   ```
   interactive_bash(tmux_command="split-window -h 'htop'")
   ```

3. **Background vs foreground**: Use `run_in_background=true` for exploration while continuing conversation. Use `run_in_background=false` when you need the result before proceeding.

**Note for non-tmux environments**: If tmux is not available (Cursor, VS Code, non-tmux terminal), agents still work but run invisibly. The same `delegate_task` patterns apply - you just won't see the visual panes.
