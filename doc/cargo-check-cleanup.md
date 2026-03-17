# Cargo Check Cleanup Map

This note groups the current `cargo check` warnings into cleanup tranches so we
do not delete potentially useful code just to silence `dead_code`.

Snapshot taken on `topic/usit-qt` after the fcitx bridge dev-flow cleanup.

## 1. Likely Next-Tranche Code

These look like real seams we probably want to keep and reintroduce properly.

### `src/audio/capture.rs`

- `CaptureControl::{pause, resume, sample_rate, channels}`
- `AudioCapture::{new, control, sources}`
- `AudioCapture::sources` field

Interpretation:

- this is the richer live capture/control surface
- the rebuilt app is using a narrower startup path at the moment
- likely tied to future control-panel and device-selection cleanup

### `src/input/fcitx5_bridge.rs`

- `Fcitx5BridgeInjector::{reload_fcitx5, new}`

Interpretation:

- `new_passive()` is currently used by the rebuilt app
- the active reload path still matters conceptually for a future “fix it for me”
  flow or backend selection path
- not obviously trash; just not wired into the current runtime slice

### `src/streaming.rs`

- `swap_transcriber`
- `is_speaking`

Interpretation:

- these smell like hot-swap / state-reporting affordances we may want once
  model lifecycle and interposition get richer

## 2. Legacy Utility Surface Worth Auditing

These may be useful helpers, but they are not currently exercised by the rebuilt
app and should be kept only if we can name the consumer.

### `src/download.rs`

- `ModelPaths::{silero_vad, asr_dir}`
- `is_model_downloaded`
- `available_models`

### `src/model_cache.rs`

- `IntegrityError::OnnxLoadFailed`
- `fallback_order`
- `find_cached_models`

### `src/backend/moonshine.rs`

- `transcribe`

Interpretation:

- these look like leftovers from broader model-management and batch-ish utility
  APIs
- they may still be useful for tooling or fallback work, but the rebuilt app is
  currently using a narrower runtime path

## 3. Spectrum/Waterfall Parking Lot

This is the single biggest warning cluster.

### `src/spectrum.rs`

Unused right now:

- alternate color schemes and color conversion helpers
- `WaterfallHistory`
- `WaterfallPacer`
- quantization/height helpers
- some analyzer reset/config accessors

Interpretation:

- we clearly kept more of the old visualization toolkit than the current Qt and
  ANSI shells are using
- this is not random junk, but it is currently a parked subsystem

Recommendation:

- either reintroduce waterfall mode deliberately
- or quarantine the unused parts behind a smaller live interface

## 4. Small Cleanup Candidates

These are the easiest warnings to decide on and likely the best first cleanup
targets.

### `src/inject.rs`

- trait method `name`

Likely choice:

- remove if we no longer display/log backend names through the trait object
- otherwise wire it into status text so it earns its keep

### `src/audio/vad.rs`

- `reset`

Likely choice:

- keep if model/session hot-reset is planned soon
- otherwise remove for now and re-add when needed

### `src/spectrum.rs`

- `DEFAULT_WATERFALL_SECONDS_PER_SCREEN`

Likely choice:

- delete or move next to the waterfall code when that feature comes back

## 5. Suggested Cleanup Order

1. Cheap certainty pass
   - remove or wire tiny stragglers like `inject::TextInjector::name`
   - remove obviously orphaned constants/helpers
2. Model/cache audit
   - decide which utility APIs are still part of the intended public/internal
     shape
3. Spectrum split
   - decide whether waterfall is next-tranche work or quarantine material
4. Capture/control audit
   - keep the richer audio control surface, but document which pieces are
     intentionally parked versus accidentally unused

## 6. Non-Goal

Do not blindly “fix” the warning count by deleting:

- capture control affordances
- model lifecycle helpers
- visualization subsystems

unless we can say which future tranche no longer needs them.
