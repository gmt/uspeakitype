# Upstream Audit - March 13, 2026

This note captures the upstream model and project changes reviewed while refreshing `usit`.

## Adopted In This Pass

- Added the newer official Moonshine tiny language variants that still fit the existing ONNX download and decoding path:
  - `moonshine-tiny-ar`
  - `moonshine-tiny-zh`
  - `moonshine-tiny-ja`
  - `moonshine-tiny-ko`
  - `moonshine-tiny-uk`
  - `moonshine-tiny-vi`
- Cleaned up direct Rust dependencies and removed the unused direct `ashpd` dependency.
- Pinned the `winit` layer-shell fork by commit instead of a floating branch for reproducibility.

## Worth Tracking Next

### Moonshine Voice / streaming family

Useful Sensors now publishes a newer Moonshine stack oriented around streaming voice UX:

- `UsefulSensors/moonshine-voice`
- `UsefulSensors/moonshine-streaming-tiny`
- `UsefulSensors/moonshine-streaming-medium`
- `UsefulSensors/moonshine-streaming-small`

Why it matters:

- This is the clearest upstream precedent for low-latency streaming ASR in the same ecosystem.
- It may supersede parts of the current "Moonshine + Silero commit detection" design over time.
- Adopting it likely means a new runtime path rather than a drop-in model swap.

What blocks immediate adoption:

- Different packaging/runtime assumptions than the current ONNX merged Moonshine path.
- Needs a fresh adapter layer and a decision about how `partial` versus `commit` semantics map into `usit`.

### Newer NVIDIA Parakeet line

Recent official Parakeet model cards now emphasize newer RNN-T and CTC releases such as:

- `nvidia/parakeet-rnnt-1.1b`
- `nvidia/parakeet-ctc-0.6b`
- `nvidia/parakeet-ctc-1.1b`

Why it matters:

- They represent NVIDIA's actively promoted ASR direction.
- The current `usit` Parakeet path depends on a third-party ONNX export of `parakeet-tdt-0.6b-v3`.

What blocks immediate adoption:

- `usit` currently expects either Moonshine merged ONNX packaging or the existing NeMo transducer export shape.
- Official Hugging Face model cards are not equivalent to ready-to-download ONNX artifacts for the current loader.

### Rust precedents

Two project precedents looked especially relevant:

- `vic1707/parakeet-rs`
  - Good reference for a Rust-native Parakeet runtime and model handling strategy.
- `the-vk/hyprwhspr`
  - Useful precedent for a Linux desktop speech UX that injects text into Wayland-focused apps.

## Recommendation

The highest-value follow-on is a separate spike that evaluates `moonshine-voice` / `moonshine-streaming-*` as a new backend family, while keeping the current ONNX Moonshine path as the stable default. After that, revisit Parakeet with a goal of either:

- ingesting an official ONNX export path, or
- borrowing implementation ideas from `parakeet-rs` for a cleaner Rust-native integration.

## Sources

- https://huggingface.co/UsefulSensors/moonshine-tiny-ja
- https://huggingface.co/UsefulSensors/moonshine-base-ja
- https://huggingface.co/UsefulSensors/moonshine-voice
- https://huggingface.co/UsefulSensors/moonshine-streaming-tiny
- https://huggingface.co/UsefulSensors/moonshine-streaming-medium
- https://huggingface.co/UsefulSensors/moonshine-streaming-small
- https://huggingface.co/nvidia/parakeet-rnnt-1.1b
- https://huggingface.co/nvidia/parakeet-ctc-0.6b
- https://huggingface.co/nvidia/parakeet-ctc-1.1b
- https://github.com/vic1707/parakeet-rs
- https://github.com/the-vk/hyprwhspr
