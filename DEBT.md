# Technical Debt

Known limitations accepted for now.

## WGPU Keyboard Focus Model

**Location**: `src/ui/app.rs` - `KeyboardInteractivity::OnDemand`

**Problem**: Overlay hotkeys (c, p, g, w, q) only work after clicking the overlay. Users must click-to-focus before keyboard shortcuts respond.

**Why it's wrong**: A speech transcription overlay should be hands-free. Requiring mouse interaction defeats the purpose for accessibility use cases.

**Proper fix**: Global hotkeys via DBus, uinput, or compositor-specific protocols. Significantly more complex - requires daemon communication, permissions handling, and per-compositor implementations.

**Workaround**: Click the overlay first, or use mouse to interact with control panel directly.
