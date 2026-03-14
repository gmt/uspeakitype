# Trine

This note is the short version of what `usit` is trying to become, and why.

## What We Are Building

`usit` is not just a speech model runner with a floating overlay.

It is a desktop input helper that aims to:

- hear speech continuously
- revise text live
- commit intentionally
- act on the user's behalf only when explicitly trusted to do so

That last point matters most.

## Why The Architecture Is Changing

The hard problem is not primarily:

- WGPU vs Qt
- terminal vs overlay
- OpenGL vs some other renderer

The hard problem is that the project eventually wants powers that modern desktop systems are right
to treat with suspicion:

- broad contextual awareness
- cross-application interaction
- text insertion on the user's behalf
- possibly deeper assistive or helper-style behavior over time

Wayland is hostile to ambient omniscience for good reasons. We should not try to smuggle the old
X11 trap door back in.

## The New Bias

We should think of `usit` as having two broad modes:

### Ordinary mode

- transcription
- local display
- narrow, obvious permissions
- graceful degradation across desktops

### Trusted helper mode

- stronger desktop cooperation
- explicit capability grants
- visible user intent
- behavior that is auditable and revocable

This is a healthier goal than becoming a permanently omniscient background process.

## What Follows From That

- Plasma/KWin is the most useful reference desktop right now.
- Cross-desktop neutrality is good, but not at the expense of pretending all desktops offer the
  same legitimate path to trusted helper behavior.
- UI framework choices are downstream of capability architecture, not the other way around.
- Fallbacks are important, but the ideal future is cooperation with the desktop, not endless
  key-faking cleverness.

## Practical Guidance

- Prefer designs that make trust boundaries legible.
- Keep normal-mode behavior useful on its own.
- Treat privileged behavior as explicit and capability-gated.
- Optimize first for honest architecture, then for pretty shells.

## Tone

The project should stay ambitious without becoming sneaky.

If `usit` becomes powerful, it should do so in a way the user can understand, consent to, and
withdraw from.
