# Barbara - Streaming ASR with Live Revision

**Named for:** Greek "barbaros" - one who babbles unintelligibly ("bar bar bar"). Our job: make the babbling intelligible.

## What This Is

A streaming speech-to-text system that shows words as you speak and *revises* earlier guesses as more context arrives. Unlike batch ASR (wait for silence → transcribe → show), streaming ASR updates continuously.

```
Batch (Whisper/Sonori):     Streaming (Barbara):
[speak 3 seconds]           "The"
[silence detected]          "The qui"
[500ms processing]          "The quick br"
"The quick brown fox"       "The quick brown fox"
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      INFERENCE (Rust)                           │
│  ┌──────────┐   ┌───────────┐   ┌─────────┐   ┌─────────────┐  │
│  │  Audio   │ → │ Streaming │ → │  WGPU   │ → │  Clipboard  │  │
│  │ Capture  │   │ Moonshine │   │ Overlay │   │  / Paste    │  │
│  └──────────┘   └───────────┘   └─────────┘   └─────────────┘  │
│                      │                              │           │
│                      ▼                              ▼           │
│              ┌──────────────────────────────────────────┐       │
│              │  Corrections Store (future)              │       │
│              └──────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ (one-way, overnight)
┌─────────────────────────────────────────────────────────────────┐
│                    TRAINING (Python, future)                    │
│              Fine-tune on corrections → export ONNX             │
└─────────────────────────────────────────────────────────────────┘
```

## Key Insight: VAD Role Change

In Sonori (batch): VAD gates WHEN to transcribe (silence → send segment → transcribe)  
In Barbara (streaming): VAD triggers COMMIT (transcribe continuously, silence → finalize phrase)

```rust
loop {
    audio_chunk = recv_audio();
    buffer.extend(audio_chunk);
    
    partial_text = moonshine.transcribe(&buffer);  // ~30ms for 1s audio
    ui.show_partial(partial_text);
    
    if vad.is_silence() {
        ui.commit(partial_text);
        clipboard.copy(&partial_text);
        buffer.clear();
    }
}
```

## Why Moonshine

| Model | Params | Speed on CPU | Streaming |
|-------|--------|--------------|-----------|
| Whisper Base | 74M | 4.8x realtime | No |
| Moonshine Base | 62M | 27x realtime | Yes |

Moonshine is fast enough on CPU that CUDA isn't necessary. Designed for edge devices.

## Stealing from Sonori

**Keep:**
- WGPU overlay + Wayland layer shell (forked winit)
- Portal integration (global shortcuts)
- Audio capture (portaudio)
- Silero VAD model
- wl-copy clipboard

**Remove:**
- 29s chunking logic
- VAD-gated batching
- Prompt conditioning for chunk continuity
- Manual vs realtime mode distinction
- Multi-backend abstraction (Moonshine only)

## Phased Plan

### Phase 1: MVP Streaming (current)
- [x] Project skeleton with module structure
- [ ] Port audio capture from sonori
- [ ] Port Moonshine inference (use existing ONNX backend)
- [ ] Port minimal UI (layer shell + text)
- [ ] Wire streaming loop
- [ ] Partial vs committed text rendering

### Phase 2: Correction Infrastructure
- [ ] Store (audio, transcript, correction) tuples
- [ ] Implicit: user edits before paste = correction
- [ ] Explicit: approve/reject buttons (optional)

### Phase 3: Fine-tuning Pipeline  
- [ ] Python script reads corrections
- [ ] LoRA fine-tune Moonshine
- [ ] Export ONNX, inference side hot-reloads

### Phase 4: Wayland Input Device
- [ ] Investigate KDE/fcitx/ibus situation
- [ ] Currently blocked by upstream issues
- [ ] For now: clipboard hacks (wl-copy)

## Reference: Moonshine Demo

The official Moonshine `live_captions.py` demo shows the streaming pattern:
https://github.com/moonshine-ai/moonshine/tree/main/demo

## Current State

```
~/src/barbara/
├── Cargo.toml          # Dependencies locked, compiles
└── src/
    ├── main.rs         # CLI entry point
    ├── audio/
    │   ├── capture.rs  # TODO: port from sonori
    │   └── vad.rs      # TODO: port from sonori
    ├── backend/
    │   └── moonshine.rs # TODO: port inference
    └── ui/
        ├── mod.rs      # TranscriptState (partial/committed)
        ├── app.rs      # TODO: port layer shell
        └── renderer.rs # TODO: port WGPU/glyphon
```

## Sonori Reference Locations

When porting, look at:
- `sonori/src/src/audio_capture.rs` - PortAudio setup
- `sonori/src/src/silero_audio_processor.rs` - Silero VAD
- `sonori/src/src/backend/moonshine/` - Moonshine inference
- `sonori/src/src/ui/app.rs` - Layer shell setup
- `sonori/src/src/ui/text_renderer.rs` - Glyphon text
- `sonori/src/src/ui/window.rs` - WGPU setup

## Commands

```bash
cd ~/src/barbara
cargo check    # Verify compiles
cargo run      # Run (just prints stub messages for now)
cargo run -- --headless  # Future: terminal-only mode
```
