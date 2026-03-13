# Model Lifecycle

This note describes how `usit` chooses, validates, downloads, and swaps ASR models.

## Artifact Families

Today the runtime supports two model packaging families:

- Moonshine merged ONNX layouts
- NeMo transducer ONNX exports such as `parakeet-tdt-0.6b-v3`

Moonshine is expected to use the official merged ONNX file layout. The project no longer treats the older `preprocess.onnx` style packaging as the canonical path.

For NeMo-specific export details, see [`nemo-tdt-onnx-workflow.md`](./nemo-tdt-onnx-workflow.md).

## Requested vs Active

`usit` tracks two different model identities in shared UI state:

- `requested_model`: what the user or startup config asked for
- `active_model`: the designated driver currently producing transcription

Those values may differ temporarily.

Example: if the requested model is not cached yet, `usit` may activate another verified cached model immediately so transcription is available while the requested model downloads in the background.

## Startup Behavior

Outside demo mode, startup does this:

1. Resolve the requested model from CLI or config.
2. Look for already cached models.
3. Prefer the requested model if it is cache-ready.
4. Otherwise try another cached model as the designated driver.
5. If the requested model is missing or quarantined, enqueue an async download.

This keeps startup responsive while still converging on the user's chosen model.

## Activation Rules

Downloads are non-blocking once the app is running.

When a download completes:

- `usit` activates the downloaded model only if it still matches `requested_model`
- it skips duplicate activation if that model is already active
- it keeps the current active model if the user requested something else in the meantime

Manual model switches use the same rule set. A cached, valid model swaps in immediately; otherwise the UI records the request and waits for the download manager.

## Degradation Behavior

Model failure is not automatically fatal.

- In headless mode with no usable model, `usit` exits cleanly after reporting the error.
- In ANSI or WGPU mode with no usable model, `usit` continues as an audio visualizer and surfaces the error in the UI.
- In all no-model cases, text injection stays disabled.

This keeps the input stack safe while still letting the user inspect audio and download status.

## Download Status in the UI

The shared state carries both:

- a session-wide `download_progress`
- per-model `download_progress_by_model`

Rendering prefers the highest-priority status:

1. model/cache error
2. download progress
3. transcript content

That ordering applies to both ANSI and WGPU surfaces.

## Cache Integrity

Model cache validation follows a three-level chain of trust:

1. remote manifest, if upstream provides checksums
2. local `.manifest.json` generated after successful download
3. heuristic validation such as size bounds and ONNX load checks

If validation fails, the model directory is quarantined rather than used opportunistically.

The quarantine workflow moves suspect model data into backup/archive locations and logs the event at error level.

## Why This Matters

The requested/active split is what makes the current UX workable for background downloads and will also matter later if we start attaching learning or uncertainty metadata to outputs. The active model is the one with real provenance at any moment; the requested model is only intent.
