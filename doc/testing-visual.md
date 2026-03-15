# Visual Testing

This note summarizes the durable rules behind `usit`'s graphical regression tests.

For the full harness walkthrough, package list, and golden-capture procedure, see [`tests/visual/README.md`](../tests/visual/README.md).

## Canonical Environment

The visual suite is designed around one canonical environment:

- Wayland
- wlroots compositor
- headless Sway in Docker for reproducibility
- software rendering
- deterministic fonts
- fixed output geometry and scale

This is the environment where visual tests are expected to pass consistently.

## Pass vs Skip

The suite intentionally distinguishes canonical failures from non-canonical skips.

Expect `PASS` in the canonical headless wlroots environment.

Expect `SKIP` rather than noise on systems such as:

- no Wayland session
- unsupported compositor families
- missing `grim`
- missing goldens in a non-canonical environment

This is a feature, not a weakness: visual regressions should be strict where the environment is controlled and conservative where it is not.

## Determinism Rules

The regression harness assumes:

- fixed theme inputs
- fixed demo timeline milestones
- fixed hidden demo helper-state overrides when a visual test needs a specific trust or fallback mode
- deterministic screenshot capture
- perceptual comparison rather than exact pixel equality

The goal is to catch user-visible regressions without overfitting to harmless anti-aliasing differences.

## Docker Story

The Docker flow exists so Wayland/WGPU testing does not depend on a user's current desktop session.

Key ingredients are:

- headless Sway
- `grim` for screenshots
- software rendering stack
- reproducible font packages

If a visual test fails in Docker, treat it as a real regression until proven otherwise.

## Helper-State Coverage

The visual suite now has a lightweight way to exercise helper-specific overlay states without
changing the normal product UI:

- hidden `--demo-overlay-state` values such as `display`, `transcribe`, `trusted`,
  `downloading`, and `error`
- hidden `--demo-open-panel` to force the helper panel open for deterministic shell captures
- screenshot-region comparisons over the transcript/status panel for those demo states
- deterministic open-panel coverage over the helper shell and focused-control card

This is intentionally narrower than a full golden-capture matrix. It gives us automated coverage
for trust and fallback cues while keeping the public CLI and the committed goldens relatively calm.

## Opacity Caveat

Screenshot-based tests can validate end-to-end opacity behavior, including directional checks like "more overlay opacity hides more background."

What they do not prove perfectly is the internal per-surface alpha decomposition. In other words, they tell us the user-facing result changed, not necessarily which render layer caused it.

## Recommended Commands

Fast path:

```bash
cargo test --release --test visual_tests opacity -- --ignored --nocapture
```

Canonical Docker path:

```bash
docker compose run visual-tests cargo test --release --test visual_tests -- --ignored --nocapture
```
