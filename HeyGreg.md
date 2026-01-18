# Hey Greg - Questions & Status

## What I Did Tonight

Ported the UI layer from sonori to barbara with simplifications:

1. **app.rs** - Winit event loop with Wayland layer shell (using your forked winit)
2. **renderer.rs** - WGPU window state and render loop  
3. **text_renderer.rs** - Glyphon text rendering
4. **shaders/rounded_rect.wgsl** - Transparent rounded rect background
5. **mod.rs** - SharedTranscriptState with Arc<RwLock<T>> for thread safety

The demo spawns a background thread that simulates streaming text updates.

## How to Test

```bash
cd ~/src/barbara
cargo run
```

Should show a transparent overlay at bottom of screen with text appearing over ~6 seconds:
- "Listening..." (partial, gray)
- "Hello world" -> commits (white)  
- "this is streaming" -> commits
- "transcription" (stays partial)

## Known Issues / TODO

### 1. Text Color for Partial vs Committed
Currently using simple color switch based on whether committed is empty. The proper solution would use `Buffer::set_rich_text()` with spans, but I simplified for now. Want me to implement proper two-tone rendering?

### 2. Window Size
Hardcoded to 25% screen width, 100px height. Good for now or want it configurable?

### 3. Layer Shell Position
Currently anchored to BOTTOM. Sonori had configurable positions (TopLeft, BottomCenter, etc). Need that?

## Questions for You

1. **sonori/src/src/src/** - Why the nested src directories? Should I follow that pattern or keep it flat?

2. **Running on Wayland?** - This uses layer shell which is Wayland-only. The forked winit has X11 fallback but transparency might not work. You running Wayland?

3. **Test Run** - Can you try `cargo run` and let me know if the overlay appears? If it crashes, grab the error.

## Next Steps (when you're back)

1. Port audio capture (portaudio)
2. Port Moonshine inference  
3. Wire real streaming loop (audio -> inference -> UI)
4. VAD for commit detection

The UI scaffold is ready for real transcription data!
