# Documentation

This directory holds the durable project notes that are still useful after the implementation churn settles down.

## Core Behavior

- [`audio-input-policy.md`](./audio-input-policy.md) - device, gain, and AGC policy
- [`input-backends.md`](./input-backends.md) - text injection backend selection and backend-specific contracts
- [`kde-plasma-posture.md`](./kde-plasma-posture.md) - why Plasma Wayland is the current reference desktop and what that implies for `usit`
- [`model-lifecycle.md`](./model-lifecycle.md) - requested vs active model state, downloads, integrity, and fallback
- [`qt-quick-container-concept.md`](./qt-quick-container-concept.md) - mockup note for a Qt Quick shell around the spectrogram/transcript overlay
- [`ui-contracts.md`](./ui-contracts.md) - ANSI and WGPU surface behavior, layout rules, and control-panel contracts
- [`testing-visual.md`](./testing-visual.md) - canonical visual-test environment and what counts as pass vs skip
- [`runtime-operations.md`](./runtime-operations.md) - logging, instance tags, and operational CLI behavior

## Model Notes

- [`bridge-to-moonshine-voice.md`](./bridge-to-moonshine-voice.md) - design note for bridging to the newer Moonshine runtime family
- [`nemo-tdt-onnx-workflow.md`](./nemo-tdt-onnx-workflow.md) - NeMo/Parakeet export layout expected by `usit`
- [`upstream-audit-2026-03.md`](./upstream-audit-2026-03.md) - recent upstream model and project audit

## Early Ideas

- [`ideas/nascent/README.md`](./ideas/nascent/README.md) - intentionally lightweight speculative notes
