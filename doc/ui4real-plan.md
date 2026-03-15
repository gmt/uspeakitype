# UI4real Plan

This note is the finish-line plan for the `topic/ui4real` branch family.

It is meant to be specific enough that an agent can pick up one milestone, implement it, and
prove whether it is done without re-deriving the product intent every time.

## Finish Line

`usit` should feel like a credible Plasma-first desktop input helper while still keeping the ANSI
surface honest and useful.

The finish line is not "perfect KDE integration". It is:

- one shared control model across ANSI and WGPU
- one unified config story for ML and helper-facing settings
- a WGPU overlay that behaves like a deliberate helper surface rather than a debug HUD
- clear trust boundaries around injection and model behavior
- an automation story strong enough that follow-up work does not depend on manual heroics

## Milestones

### 1. Shared Control Parity

Acceptance:

- WGPU and ANSI expose the same meaningful controls unless a control is explicitly desktop-local.
- Focus, help text, and section order come from shared state, not duplicated per-surface lore.
- WGPU model actions and config persistence match TUI behavior closely enough that switching
  surfaces does not change what a control actually does.

Primary files:

- [`src/ui/control_panel.rs`](../src/ui/control_panel.rs)
- [`src/ui/terminal.rs`](../src/ui/terminal.rs)
- [`src/ui/app.rs`](../src/ui/app.rs)
- [`src/ui/renderer.rs`](../src/ui/renderer.rs)
- [`src/main.rs`](../src/main.rs)

Suggested validation:

- `cargo test`
- `timeout 5s target/debug/usit --ansi --demo` in a PTY
- `cargo test --test visual_tests test_wgpu_control_panel_full -- --nocapture`

### 2. Real Device and Session Semantics

Acceptance:

- Device selection is not a decorative no-op.
- If live switching is still not viable, the UI must say so and persist startup intent honestly.
- Auto-save writes back the same source/model intent regardless of whether the user changed it in
  ANSI or WGPU.
- Session controls communicate whether they are immediate, deferred until restart, or purely visual.

Primary files:

- [`src/audio/capture.rs`](../src/audio/capture.rs)
- [`src/ui/control_panel.rs`](../src/ui/control_panel.rs)
- [`src/main.rs`](../src/main.rs)
- [`src/config.rs`](../src/config.rs)

Suggested validation:

- unit tests for source-selection persistence and startup resolution
- `cargo run -- --list-sources`
- ANSI and WGPU panel flows that both show the same chosen source on next launch

### 3. Plasma-World Overlay Shell

Acceptance:

- The WGPU overlay reads as a helper surface with intentional hierarchy, not a floating modal dump.
- Transcript, spectrogram, helper controls, and trust/status cues have stable visual roles.
- The graphical panel remains legible across opacity levels and small windows.
- ANSI mirrors the information architecture even when the mechanics differ.

Primary files:

- [`src/ui/renderer.rs`](../src/ui/renderer.rs)
- [`src/ui/text_renderer.rs`](../src/ui/text_renderer.rs)
- [`src/ui/theme.rs`](../src/ui/theme.rs)
- [`src/ui/terminal.rs`](../src/ui/terminal.rs)
- [`tests/visual_tests.rs`](../tests/visual_tests.rs)

Suggested validation:

- visual test additions for helper-panel hierarchy and small-window behavior
- `cargo test --test visual_tests`
- ANSI size matrix stays green

### 4. Trust and Capability Cues

Acceptance:

- Injection state is always obvious.
- Requested model, active model, and download state are visually distinguishable.
- Error/fallback states explain whether `usit` is display-only, transcribing, or empowered to act.
- The UI does not imply omniscient behavior when the underlying capability is narrower.

Primary files:

- [`src/ui/mod.rs`](../src/ui/mod.rs)
- [`src/ui/renderer.rs`](../src/ui/renderer.rs)
- [`src/ui/status_widget.rs`](../src/ui/status_widget.rs)
- [`src/ui/terminal.rs`](../src/ui/terminal.rs)
- [`doc/kde-plasma-posture.md`](./kde-plasma-posture.md)

Suggested validation:

- state-machine tests around requested vs active model display
- visual coverage for error, download, and injection-disabled states

### 5. Automation-Ready Finish

Acceptance:

- Every remaining skip is justified in repo-local docs or tests.
- Visual tests cover the helper panel enough that styling regressions are caught automatically.
- The executable runs in ANSI demo mode and the WGPU path is covered by the compositor harness.
- The next agent can see which milestone is incomplete and what "done" means without asking.

Primary files:

- [`tests/visual_tests.rs`](../tests/visual_tests.rs)
- [`tests/tui_size_matrix.rs`](../tests/tui_size_matrix.rs)
- [`doc/testing-visual.md`](./testing-visual.md)
- [`doc/ui-contracts.md`](./ui-contracts.md)

## Parallelizable Work

These are good candidate subagent lanes:

- `control-parity`: shared action dispatch, autosave, model/source persistence
- `overlay-shell`: WGPU layout, hierarchy, warm theme, control affordances
- `visual-harness`: golden coverage, compositor test additions, small-window regressions
- `device-semantics`: startup source persistence and honest device-selection UX
- `trust-cues`: requested/active model, injection state, fallback/error display

## Current Status

Already landed:

- shared sectioned helper panel model
- shared help copy
- warm theme shift for the real app
- WGPU keyboard navigation
- first pass at WGPU/TUI structural parity
- startup source discovery feeding shared ANSI/WGPU device state
- deferred "next launch" source semantics with unified config persistence

Still notably incomplete:

- WGPU needs richer status/provenance cues
- helper-shell polish is still behind the product posture docs
- visual coverage still needs to catch more of the helper-shell hierarchy and trust states
