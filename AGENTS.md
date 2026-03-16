## Project Overview

**usit** (uspeakitype - "you speak, I type") is a streaming speech-to-text overlay for Linux. Unlike batch ASR systems (Whisper), usit streams words as you speak and revises them in real-time as context improves accuracy. It provides both terminal (ANSI/Ratatui) and graphical (Wayland layer shell/WGPU) interfaces.

## Repository Drift Note

The repo is in active reconstruction. A few layout details are intentionally
nonstandard right now:

- Root directories were renamed to singular forms on purpose:
  `doc/`, `example/`, `experiment/`, `script/`, `test/`
- `oldcrap/` is a quarantine zone for retired or legacy code that we are
  reintroducing selectively; do not assume paths there are live
- `oldcrap/README.md` explains why the quarantine exists and how to treat it
- Some architecture notes below describe the historical WGPU-era layout and may
  lag the current rebuild; trust the live tree over stale prose if they diverge

If you are looking for something and the plural path seems "obvious," check the
singular form first before assuming the repo is broken.


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
Streaming (usit): Show words as spoken → revise earlier guesses → commit on silence

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

| Anti-pattern                | Rationale                                                                                   | Reference            |
| --------------------------- | ------------------------------------------------------------------------------------------- | -------------------- |
| Single-surface frobs        | Every control MUST work in both TUI and WGPU; if terminal can't represent it, reconsider    | UI/Config Principles |
| 29s chunking logic          | Removed from sonori; continuous streaming model                                             |                      |
| Persistent AGC adjustments  | Algorithm-driven gain changes don't save to config; only explicit user choices persist      |                      |
| AGC vs manual gain conflict | When AGC active, gain slider becomes disabled or "aggressiveness" control; no fighting user |                      |

## Key Dependencies

| Crate      | Purpose                                           |
| ---------- | ------------------------------------------------- |
| `ort`      | ONNX Runtime for ML inference (Moonshine, Silero) |
| `pipewire` | PipeWire audio capture (16kHz mono)               |
| `wgpu`     | GPU rendering for overlay                         |
| `glyphon`  | Text rendering on WGPU                            |
| `winit`    | Windowing (forked for Wayland layer shell)        |
| `anyhow`   | Error handling                                    |
| `clap`     | CLI argument parsing                              |
| `tokio`    | Async runtime                                     |

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

### Docker Visual Tests

- Requires Docker Compose v2 plugin (`docker compose`).
- If Docker build fails with openssl-sys, ensure `libssl-dev` is in the image (see Dockerfile).
- If Docker build fails with xkbcommon (smithay-client-toolkit), ensure `libxkbcommon-dev` is in the image.
- If Docker build fails with bindgen/libspa-sys, ensure `libclang-dev` is in the image.
- Run: `docker compose run visual-tests cargo test --release --test visual_tests opacity -- --ignored --nocapture`

### Test Patterns

| Pattern                     | Purpose                                           | Location                       |
| --------------------------- | ------------------------------------------------- | ------------------------------ |
| Synthetic signal generation | AGC convergence tests                             | `src/audio/capture.rs:645-865` |
| Toy ONNX models             | Moonshine tests without full model                | `src/backend/moonshine.rs:784` |
| State machine tests         | AudioState transitions (partial→commit→committed) | `src/ui/mod.rs:104-209`        |
| Thread safety tests         | SharedAudioState concurrent access                | `src/ui/mod.rs`                |

### Test Utilities

- `generate_sine(freq, sample_rate, num_samples, amplitude)` - synthetic audio for AGC tests
- `tempfile::TempDir` - temporary directories for download tests
- Epsilon comparisons: `(value - expected).abs() < threshold` for float assertions
- `rms(&samples)` - RMS calculation for audio verification

When adding:

- Unit tests in same file with `#[cfg(test)]` module
- Integration tests in `test/` directory
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
3. **Config file** (`~/.config/usit/usit.toml`)
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
- ~/.config/usit/usit.toml

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

## Versioning

- Bump **patch version** when a Sisyphus boulder is complete (e.g., 0.2.5 → 0.2.6)
- Version lives in `Cargo.toml` - update it as part of the final commit
- Tag after committing: `./script/tag-version.sh`
- Push with tags: `git push` (followTags config handles it)

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



AGENT POLICY: "Requires Human Intervention" Claims

Agents MUST NOT reject work as "requires human intervention" or "cannot be automated" for Wayland/WGPU testing unless they provide a specific technical explanation of why the Docker test infrastructure is insufficient.

Invalid rejections:

- "Wayland testing requires a display" → Docker provides virtual Wayland compositor
- "WGPU needs GPU access" → Docker uses software rendering (llvmpipe)
- "Visual verification needs human eyes" → Screenshot comparison with golden images exists
- "Cannot test layer shell behavior" → Docker compositor supports layer shell protocol
- "Docker requires root/sudo" → Rootless Docker is configured, no privileges needed

Valid rejections (with explanation required):

- "Docker container lacks X protocol/library" → Specify which, propose adding to Dockerfile
- "Test requires hardware feature unavailable in software rendering" → Specify feature
- "Golden image comparison insufficient for this visual property" → Explain why, propose alternative

When in doubt: try it in Docker first, then report specific failures and why they can't be fixed (hint: they can probably be fixed).

## Model Cache Integrity Chain-of-Trust

The model cache system (`src/model_cache.rs`) provides deterministic integrity verification that is **independent of model choice**. Corruption detection is not a consequence of which inference engine is selected - it flows from a chain of trust anchored in file hashes and metadata.

### Integrity Verification Hierarchy

1. **Remote manifest** (preferred): If upstream provides checksums (e.g., HuggingFace LFS), fetch and verify against them.
2. **Local manifest** (fallback): After successful download, we generate and persist a `.manifest.json` containing SHA-256 hashes, file sizes, and timestamps.
3. **Heuristic validation** (last resort): Size bounds + ONNX Runtime load validation. If this fails, data is treated as corrupt.

If none of these can verify integrity, the cache is quarantined.

### Quarantine and Backup Policy

When corruption is detected:

1. **Quarantine**: Move the entire model directory to `~/.cache/usit/models/.backup/<timestamp>-<model-id>/`
2. **Archive rotation**: If `.backup` already contains data for this model, move it to `.backup_archive/`
3. **Archive cleanup**: If `.backup_archive` entry already exists or is older than 30 days, discard it and log an error
4. **Logging**: Every corruption detection and mitigation MUST be logged as `error` level (except resumable `.downloading` partials)

### Partial Downloads

Files with `.downloading` extension are in-progress downloads. They are:
- Ignored during integrity checks (not corruption)
- Cleaned up before download attempts
- Not logged as errors when removed

### Model Fallback Order

When the selected model fails to activate:

1. Try other already-cached models (verified or unverified integrity)
2. If none cached, attempt downloads in priority order: MoonshineBase → MoonshineTiny → Parakeet
3. Track attempted models in memory to prevent infinite download loops
4. If all fail, continue as audio visualizer with red error message

## Input Device Safety Rule

**CRITICAL**: When model activation fails (no transcription available), we MUST NOT register as an input device.

This ensures:
- We never break the user's input system when models fail to load
- The IME/input method framework never sees us if we can't transcribe
- Deactivation and garbage collection follow all framework rules

Implementation (`src/main.rs`):
- `transcription_available` flag gates injection thread registration
- If no model loads, injector thread consumes the channel but never calls `select_backend()`
- On shutdown, injector thread joins to ensure proper Wayland IME cleanup

### Graceful Degradation

| Condition | Behavior |
|-----------|----------|
| No model, headless mode | Exit cleanly with error message (exit code 0) |
| No model, TUI/WGPU mode | Continue as audio visualizer with red error text |
| Model load fails mid-session | Log error, continue with previous model if hot-swapping |
| Download fails | Try next model in fallback order, then show error |

### Error Display

Errors are displayed prominently in both UIs:
- **TUI**: Red bold text in status area: `ERR: <message>`
- **WGPU**: Red text in transcript panel: `ERROR: <message>`

The spectrogram continues to function as an audio visualizer even when transcription is unavailable.
