# Audio Input Policy

This document defines usit's policy for device selection, gain control, and automatic gain management.

## Definitions

- **Soft AGC**: In-software gain adjustment after samples are captured. Safe, isolated, but cannot rescue already-clipped input.
- **Hard AGC**: Adjusts the actual PipeWire device/source volume via WirePlumber. Can prevent clipping but affects global system state.
- **Expecting input**: User is in push-to-talk active state, or always-on mode is enabled.
- **Control panel (CP)**: In-app settings UI, available in both terminal (ANSI) and graphical (WGPU) modes.

## Device Selection

### Policy
1. **Default**: Use last-used device if still present; otherwise fall back to system default.
2. **Persistence**: Selected device is saved to config when auto-save is enabled or user explicitly saves.
3. **Hot-plug**: If selected device disconnects, pause capture and show warning. Do NOT auto-switch.
4. **Wrong mic detection** (TODO): If input appears unusual (e.g., silent when speech expected, or picking up system audio), show a non-intrusive warning suggesting the user check their device selection.

### Non-goals
- Never auto-switch to a "better" mic without explicit user action.
- Never silently change devices.

## Gain Control

### Two Sliders
- **Software gain**: Multiplier applied in-app after capture. Range: 0.1x - 10x. Default: 1.0x.
- **Hardware gain**: Controls PipeWire source node volume. Range: 0% - 100%. Default: whatever system has set.

### Behavior When AGC Active
When soft or hard AGC is enabled for a gain type:
- The corresponding slider becomes **disabled** (AGC owns it), OR
- The slider transforms into an **AGC aggressiveness** control (how quickly/aggressively AGC responds)

Design choice: prefer disabling the slider for simplicity; aggressiveness control is a future enhancement.

## Automatic Gain Control

### Soft AGC (Current Implementation)
- **Mechanism**: Sample-by-sample gain adjustment targeting a desired RMS level.
- **Freeze on silence**: When RMS < 0.01, gain adjustments freeze to prevent runaway amplification.
- **Limits**: Gain clamped to [min_gain, max_gain] (default 0.1 - 10.0).
- **Output clamping**: Samples clamped to [-1.0, 1.0] to prevent digital clipping.

### Soft AGC Sidechain Contract

The current AGC implementation follows a sidechain pattern:

- measure speech-band energy, not full-band energy
- apply the resulting gain to the full signal path
- stay peak-aware so loud out-of-band content still avoids clipping

Why this split exists:

- low-frequency rumble should not trick the AGC into thinking speech is already loud enough
- gain decisions should follow intelligibility, not just broadband power
- the final audio should still sound natural rather than "bandpassed"

The practical design is:

- a speech-oriented bandpass informs the control signal
- a soft limiter and peak awareness guard the output path
- silence handling stays conservative to avoid pumping background noise

### AGC Test Heuristics

Two testing rules turned out to be worth preserving:

- ignore the earliest filter-settling region when asserting AGC behavior
- choose test frequencies well away from the bandpass edges when you want stable expectations

### Hard AGC (Future Implementation)
- **Mechanism**: Adjusts PipeWire device volume via WirePlumber-compatible methods.
- **Objective**: Keep the selected device out of the red (clipping) and balance noise floor against signal headroom.
- **Conservative**: Only acts when soft AGC cannot salvage the signal (sustained clipping or extremely weak input).
- **Rate-limited**: Changes device volume slowly to avoid fighting user/system controls.
- **Restore on exit**: If hard AGC adjusted device volume, restore original value when usit exits.
- **Opt-in only**: Hard AGC is never enabled by default; user must explicitly enable it.

### Signal Health Metrics

| Metric | Detection | Soft AGC Response | Hard AGC Response |
|--------|-----------|-------------------|-------------------|
| **Clipping** | Sustained peaks at 0 dBFS | Reduce gain (clamping handles output) | Reduce device volume slowly |
| **Noise floor too high** | High RMS, flat spectrum, no speech | Freeze (don't amplify noise) | Consider reducing device volume |
| **Weak signal** | Low RMS, speech-like variance | Increase gain toward target | Consider increasing device volume |
| **Dead silence** | RMS near zero | Freeze | No action (may be intentional) |
| **Feedback risk** | Sustained tone, increasing amplitude | Freeze or reduce | Reduce device volume |

### Decision Rules

1. **Clipping + expecting input**: If soft AGC cannot resolve (already at min gain), hard AGC may reduce device volume.
2. **Weak signal + expecting input**: Soft AGC increases gain; if at max gain and still weak, hard AGC may increase device volume.
3. **Noise floor high + NOT expecting input**: No action (don't fight a noisy room when user isn't speaking).
4. **Dead silence + expecting input**: Show "no input" warning; do not adjust gain.
5. **Any metric + NOT expecting input (PTT inactive)**: Minimal action; user may have muted intentionally.

## User Interaction Model

### Warnings as Primary UX
- usit typically does NOT have window manager focus (user is typing elsewhere).
- Warnings should be **visual but non-intrusive**: color change, icon, subtle animation.
- Avoid modal dialogs or anything requiring immediate interaction.

### Accept Suggestion Gestures
- When a warning is shown (e.g., "input may be clipping"), offer an easy gesture to accept the suggested fix.
- Examples: click the warning, press a hotkey, or confirm in control panel.
- Never apply fixes silently; always require at least a confirmation gesture.

### Control Panel Access
- Hotkey or click to open CP without stealing focus from other apps (if possible).
- CP must work in both terminal and graphical modes with equivalent functionality.

## Configuration

### Sources (Precedence High to Low)
1. GUI (runtime changes)
2. Command line arguments
3. Config file (`~/.config/usit/usit.toml`)
4. Compiled defaults

### Persisted Settings
- Selected device (by name, not ID)
- Software gain (when not AGC-controlled)
- Hardware gain (when not AGC-controlled)
- Soft AGC enabled/disabled
- Hard AGC enabled/disabled
- Auto-save preference

### NOT Persisted
- Current AGC-computed gain values (these are transient)
- Device volume changes made by hard AGC (restored on exit)

## Non-Goals

- No "smart" auto-device selection based on audio quality heuristics.
- No background device scanning or probing.
- No integration with desktop notification systems (keep it self-contained).
- No per-application volume control (we control our stream and optionally the source device, nothing else).

## Open Questions

- Should hard AGC changes be journaled so the user can review what was changed?
- Should there be a "hard AGC ceiling" that prevents us from ever setting device volume above the user's initial value?
- How to handle multiple usit instances (or other apps) competing for device volume control?
