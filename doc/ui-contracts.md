# UI Contracts

This note captures the stable behavior shared by `usit`'s ANSI and WGPU interfaces.

## Two Surfaces, One State

Both frontends render from the same shared `AudioState`:

- committed transcript
- partial transcript
- speaking / paused / injection state
- requested and active model state
- download progress and model errors

The two UIs are intentionally analogous even when they are not pixel-for-pixel identical.

## Transcript Semantics

Transcript rendering always distinguishes:

- committed text: stable output
- partial text: revisable output

The display model is append-on-commit. Partial text moves into the committed buffer when a commit occurs, and a space is inserted between committed phrases when needed.

## Status Priority

When multiple things compete for the same attention slot, the UI favors:

1. model or cache error
2. download progress
3. normal transcript display

That priority order is intentional so a stalled model or in-flight download is never visually hidden by stale transcript text.

## ANSI Layout Modes

The terminal UI has four responsive modes:

- `Full`: width >= 50 and height >= 10
- `Compact`: width >= 35 and height >= 10
- `Minimal`: width >= 25 and height >= 8
- `Degenerate`: width < 25 or height < 8

In degenerate mode the control panel is hidden instead of trying to squeeze into an unreadable mess.

## Control Model

The control panel uses a shared control enum plus shared section/help metadata so both surfaces stay
aligned on ordering, intent, and explanatory copy.

Current controls are:

- device selector
- gain
- AGC
- pause
- visualization mode
- color scheme
- injection toggle
- model selector
- auto-save
- opacity
- quit

The current implementation has one explicit surface-specific exception:

- `opacity` is WGPU-only because it affects overlay alpha and has no meaningful ANSI analogue

Everything else is intended to behave equivalently across surfaces.

The panel is grouped into the same four sections on both surfaces:

- `Capture`
- `Recognition`
- `Desktop`
- `Session`

The current KDE/Plasma-facing bias is visible here: desktop trust and session ownership are treated
as first-class sections instead of being buried among audio sliders.

Each focused control also has shared help copy. ANSI and WGPU present that help differently, but
they should describe the same trust boundary or behavioral nuance when a user lands on a control.

The source selector is intentionally deferred:

- choosing a source records startup intent instead of pretending to hot-swap a live capture stream
- when the chosen source differs from the current session, both surfaces label it as applying on
  the next launch
- auto-save persists that same deferred intent regardless of whether the change happened in ANSI or
  WGPU
- one-off startup sources from the command line stay startup-only unless the user explicitly
  changes the selector in the session

## Control Timing Semantics

Shared controls also need honest timing semantics so neither surface implies powers the runtime does
not have.

- `DeviceSelector`: deferred until next launch or capture restart
- `Gain`, `AGC`, `Pause`, `Injection`, `Auto-save`, `Quit`: immediate
- `ModelSelector`: immediate request, async activation
- `Opacity`: WGPU-local and purely visual

## WGPU Layout Contract

The graphical overlay uses reserved-space layout, not text-over-spectrogram occlusion:

- the spectrogram fills the available height above the transcript panel
- the transcript panel reserves space at the bottom
- the control panel, when open, is a centered modal that overlays both

Opaque panel backgrounds are intentional. Control and transcript panels should remain visually legible regardless of the global overlay opacity applied to the spectrogram/background layer.

## Small Window Behavior

WGPU layout math clamps panel sizes for small windows instead of assuming desktop-sized geometry.

ANSI and WGPU take different mechanical approaches here, but the shared product goal is the same: degrade cleanly before becoming confusing.

## Resize Behavior

WGPU handles live resize as part of the normal window lifecycle.

ANSI also has resize handling now, but the contract is still conservative: when space is tight, the UI prefers reduced structure or hidden controls over broken composition.
