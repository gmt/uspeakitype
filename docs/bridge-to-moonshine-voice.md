# Bridge To Moonshine Voice

This note captures a likely next architectural step for `usit`: adopting the newer Moonshine runtime family without giving up the product contracts that already make sense for local desktop transcription.

## Why This Looks Attractive

The current Moonshine project direction is unusually well-aligned with `usit`'s actual deployment environment:

- low-latency speech on ordinary local hardware
- explicit focus on edge and resource-constrained devices
- no assumption that a GPU, NPU, or datacenter-class accelerator is available
- model/runtime packaging that looks bridgeable from Rust even if the first-party developer experience is not Rust-native

For `usit`, that matters more than where the weights were distilled from.

## Product Stance

We should aim to get on the Moonshine train without turning `usit` into a thin wrapper around someone else's Python UX.

The right move is:

- adopt the runtime and model family where it helps
- keep `usit`'s own transcript, provenance, and UX contracts
- isolate upstream-specific assumptions behind a backend adapter

## What Must Stay Ours

Even if Moonshine Voice becomes the preferred backend family, `usit` should continue to own:

- `partial` versus `commit` transcript semantics
- requested model versus active model tracking
- desktop-oriented download and cache behavior
- input-injection safety rules
- local provenance for any future RL or correction data
- user-facing policy on confidence and uncertainty display

These are product contracts, not backend accidents.

## Minimal Bridge Contract

Any new backend family should be adapted into a small internal contract with roughly these questions:

- Can it emit revisable partial text?
- Can it emit commit-worthy boundaries on its own, or does it still need external VAD?
- Can it expose token or span uncertainty?
- Can it hot-swap models at runtime?
- What provenance must be attached to every emitted span?
- What is the startup and steady-state memory footprint on commodity machines?

This does not need to be a grand universal ASR trait. It just needs to be enough structure that backend-specific quirks stop leaking everywhere.

## Candidate Integration Surfaces

There are at least three plausible bridge surfaces, in descending order of desirability.

### 1. C API bridge

If Moonshine's current C-facing runtime is stable enough, this is probably the cleanest path for Rust.

Why it is attractive:

- avoids embedding Python
- gives us a relatively explicit ABI boundary
- fits the shape of a local desktop application well

Risks:

- ABI churn
- ownership and threading contracts may still be immature
- we inherit upstream runtime decisions we do not control

### 2. Artifact/runtime bridge

Treat Moonshine as a model/runtime packaging standard and reproduce the required runtime logic in Rust around ONNX Runtime or compatible artifacts.

Why it is attractive:

- maximum control
- easier to preserve `usit`'s current architecture
- likely best long-term maintainability if the artifact format is stable

Risks:

- most implementation work falls on us
- easy to lag behind subtle upstream behavior changes
- higher chance of "looks compatible, acts differently"

### 3. Sidecar process bridge

Run the upstream runtime as a companion process and communicate over a narrow IPC boundary.

Why it is attractive:

- fastest way to validate product fit
- least initial Rust FFI complexity

Risks:

- awkward packaging
- harder latency and lifecycle control
- feels like a temporary bridge, not a destination

## Proposed Prototype Order

### Phase 1: strengthen the current pipeline

Before adding a new backend family, tighten the internal seams:

- keep explicit `requested_model` and `active_model`
- add backend capability metadata
- attach provenance to emitted transcript spans
- add internal-only uncertainty plumbing

This makes the future adapter simpler and also helps the current system.

### Phase 2: experimental Moonshine Voice adapter

Build a single experimental adapter around the most promising Moonshine Voice or streaming-family integration surface.

Success criteria should be local-product criteria, not benchmark vanity:

- works well on laptops and mini-PCs
- behaves sanely on CPU-only systems
- does not explode memory bandwidth
- integrates with current transcript revision semantics
- degrades cleanly when models are absent or invalid

### Phase 3: backend comparison harness

Once the experimental adapter exists, compare:

- current Moonshine merged ONNX path
- Moonshine Voice or streaming-family path
- existing Parakeet path where relevant

Judge them on:

- latency to first useful partial
- stability of revisions
- commit quality
- memory footprint
- CPU load
- user-visible quality on realistic dictation

## Important Warning

We should not let "streaming-native backend" automatically dissolve `usit`'s commit semantics.

Even if the backend can produce its own incremental output, `usit` still needs a stable answer to:

- when text becomes committed enough to inject
- how unstable spans are surfaced
- how active-model provenance is recorded across downloads and swaps

Those questions belong to the product layer.

## Recommendation

The likely next move is not a wholesale backend migration. It is a bridge spike:

1. formalize the internal backend contract
2. prototype Moonshine Voice behind that contract
3. measure it on commodity hardware
4. then decide whether it should replace or coexist with the current Moonshine path

That gives us a way onto the train without getting dragged under it.
