# Qt Quick Container Concept

This note accompanies the prototype in [`example/qt_quick_overlay`](../example/qt_quick_overlay).

## Why Bother

The current WGPU overlay does impressive low-level work, but it also hand-rolls a lot of UI container behavior:

- panel geometry
- hit testing
- modal/drawer feel
- text chrome
- control grouping

That makes every control-panel iteration feel more expensive than it should.

Qt Quick is interesting here not because it should automatically replace the renderer, but because it could plausibly take over the "container chrome" problem:

- layout
- controls
- drawers/sheets
- typography
- visual hierarchy
- transitions

while leaving the spectrogram surface itself as a separate concern.

## Intended Split

The prototype assumes a split like this:

- Qt Quick owns the window shell and controls
- Rust continues to own audio, model state, transcript state, and text injection
- the spectrogram viewport is bridged in as a specialized surface

That is intentionally narrower than a full framework migration.

## Possible Bridge Shapes

### Native child surface

Pros:

- preserves current renderer investment
- smallest conceptual leap

Cons:

- Wayland embedding can get awkward fast
- focus/input ownership becomes delicate

### Texture provider or shared image path

Pros:

- cleaner visual integration into Qt Quick
- keeps shell and content more composable

Cons:

- harder rendering bridge
- synchronization and copy costs matter

### Full Qt Quick spectrogram rewrite

Pros:

- one UI stack
- easier native polish

Cons:

- highest rewrite cost
- easy to lose current rendering behavior during port

## What The Mockup Is Trying To Validate

The prototype is mainly asking:

- does a drawer feel better than the current control modal?
- does a distinct transcript card improve hierarchy?
- does a Qt-owned shell make the spectrogram feel more intentional instead of more debug-like?
- is there enough value in the container/chrome layer alone to justify a deeper experiment?

The current mockup also leans on a more realistic viewport placeholder than the first pass did:

- a wider, fuller host surface instead of a cramped inner island
- stronger visual continuity in the fake spectrogram/waterfall content
- clearer labeling of the renderer seam so the shell reads as intentional rather than as a fake rewrite

## Outcome

This mockup did its job. The project has now started a real Qt-based graphical path, but the first production step moved to Qt Widgets rather than embedding the QML scene directly.

That still follows the core lesson from this note:

1. keep Rust as the source of truth for state
2. let Qt own shell/chrome
3. bridge snapshots and commands first
4. migrate rendering details later, only if they earn their keep
