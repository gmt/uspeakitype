# KDE / Plasma Posture

This note captures the current desktop targeting posture for `usit`.

## Working Assumption

For now, **KDE Plasma on Wayland is the reference desktop** for the full overlay/input-helper
experience.

That does not mean `usit` should become KDE-only. It means Plasma is the most useful reality
anchor for design decisions when we need one desktop to optimize around first.

## Why Plasma

Plasma currently looks like the least-insane place to daily-drive a compositor-heavy speech tool:

- broad enough Wayland protocol coverage that normal desktop life still works
- more pragmatic attitude toward automation and desktop tooling than some peers
- visible interest in input-method cleanup, helper-like input tools, and accessibility plumbing
- enough boring desktop completeness that `usit` is less likely to feel like a science project

That matters because `usit` is not just a transcription model launcher. It is trying to be a live
desktop speech surface with real capture, real overlay behavior, and real text insertion pressure.

## What We Should Infer

- We should design **Wayland-first**, not X11-first with a Wayland apology path.
- We should treat Plasma as the primary environment for validating whether the product feels sane.
- We should not assume KDE is secretly building the whole speech stack for us.
- We should pay attention to KDE's interest in input helpers and text-input semantics, because that
  is closer to `usit`'s long-term home than pretending to be a hardware keyboard forever.

## Product Consequences

### 1. Favor input-helper semantics over key fakery

The long-term target should be "speech as a desktop input helper", not "speech app that fakes key
events until the compositor looks away".

That means:

- preserve provenance for emitted text
- prefer semantic insertion where possible
- keep key-emulation fallbacks for degraded environments
- make it obvious when we are in a fallback path

### 2. Use Plasma as the first-class QA target

When we need to choose where to verify a new overlay/input behavior first:

1. Plasma Wayland
2. other wlroots-family compositors
3. GNOME

GNOME still matters strategically, but it is not the best "does this feel like a usable desktop
tool?" reference point right now.

### 3. Treat compositor collaboration as a win, not a purity failure

If Plasma or KWin offers a practical affordance that makes `usit` feel more native, we should be
willing to use it as long as it degrades cleanly elsewhere.

The goal is one speech product with native-feeling frontends, not an abstractly pure overlay that
reimplements every missing desktop behavior itself.

## UI Implications

The Qt Quick shell experiment fits this posture well:

- Qt/Plasma visual language is a more natural host than a fully custom mini-framework
- the shell can become more desktop-like without forcing a renderer rewrite on day one
- control surfaces can evolve toward drawer/sheet/helper idioms instead of bespoke floating debug
  panels

## Things Plasma Has Not Solved For Us

We should still assume the following remain ours to solve:

- designated-driver model semantics for learning/corrections
- uncertainty/confidence handling
- speech-specific commit/revision UX
- backend provenance and fallback transparency
- portable injection ladders across non-Plasma compositors

## Short-Term Guidance

- Test new overlay behaviors on Plasma first.
- Keep the ANSI surface aligned in behavior, not necessarily in implementation mechanics.
- Watch for KDE-adjacent seams that make `usit` feel like an input helper instead of an external
  hack.
- Do not block product progress on full ecosystem convergence around accessibility or semantic text
  insertion.
