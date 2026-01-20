# UI Subsystem

Dual-surface rendering: WGPU overlay + ANSI terminal. Every control must work in both.

## Architecture

```
src/ui/
├── mod.rs              # SharedAudioState, AudioState, ProcessingState
├── app.rs              # Winit event loop + Wayland layer shell
├── terminal.rs         # Crossterm TUI with spectrogram + text
├── renderer.rs         # WGPU surface orchestration
├── spectrogram.rs      # GPU spectrogram (bar meter + waterfall)
├── text_renderer.rs    # Glyphon text rendering
├── control_panel.rs    # Control panel state + 6 controls
└── theme.rs            # Theme → WGPU/ANSI conversion
```

## Shared State

`SharedAudioState = Arc<RwLock<AudioState>>` - thread-safe state container.

Key fields (mod.rs:29-40):
- `samples: Vec<f32>` - audio buffer for spectrogram
- `committed: String` - finalized transcription
- `partial: String` - in-progress transcription
- `is_speaking: bool` - VAD state
- `processing_state: ProcessingState` - Idle/Listening/Transcribing
- `is_paused: bool` - capture pause state
- `auto_gain_enabled: bool` - AGC toggle
- `current_gain: f32` - software gain multiplier
- `available_sources: Vec<AudioSourceInfo>` - audio device list
- `selected_source_id: Option<u32>` - active device

## Dual Surface Requirement

From root AGENTS.md: every "frob" MUST have both TUI and GUI implementations.

Current controls (6 total):
- Device selector
- Gain slider
- AGC checkbox
- Pause button
- Viz mode toggle (bar meter ↔ waterfall)
- Color scheme picker (flame/ice/mono)

## Key Patterns

| Pattern | Location | Notes |
|---------|----------|-------|
| Layer shell setup | app.rs:318-349 | Wayland overlay with `Anchor::BOTTOM` |
| State sync | app.rs:83-90 | `sync_control_state()` pulls from CaptureControl |
| Two-tone text (WGPU) | text_renderer.rs:114-128 | Committed=white, partial=gray |
| Two-tone text (TUI) | terminal.rs:269-298 | ANSI bold+color for committed, dim for partial |
| Control panel click | app.rs:92-184 | Gear icon + modal panel with 6 controls |
| TUI layout | terminal.rs:3-28 | Bottom-anchored box matching WGPU position |
| Theme conversion | theme.rs:48-68 | `Theme::to_wgpu()` and `Theme::to_ansi()` |

## WHERE TO LOOK

| Task | File | Notes |
|------|------|-------|
| Add keyboard shortcut | app.rs:247-284 | `WindowEvent::KeyboardInput` handler |
| Change overlay position | app.rs:339 | `Anchor::BOTTOM` (also TOP/LEFT/RIGHT) |
| Modify spectrogram colors | spectrogram.rs:209-211 | `set_color_scheme()` |
| Add TUI element | terminal.rs:432-546 | `render_control_panel()` |
| Change text colors | theme.rs:75-81 | `DEFAULT_THEME` constants |
| Add control to panel | control_panel.rs:16-25 | Add to `Control` enum |
| Sync new state field | app.rs:83-90 | Update `sync_control_state()` |
