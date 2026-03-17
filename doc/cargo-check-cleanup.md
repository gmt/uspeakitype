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

Completed:

- removed the unused `inject::TextInjector::name` surface
- removed the parked `SileroVad::reset`
- removed the unused model/cache helper cluster from the old section 2

Still relevant here:

### `src/spectrum.rs`

- `DEFAULT_WATERFALL_SECONDS_PER_SCREEN`

Likely choice:

- delete or move next to the waterfall code when that feature comes back

## 5. Suggested Cleanup Order

1. Spectrum split
   - decide whether waterfall is next-tranche work or quarantine material
2. Capture/control audit
   - keep the richer audio control surface, but document which pieces are
     intentionally parked versus accidentally unused

## 6. Non-Goal

Do not blindly “fix” the warning count by deleting:

- capture control affordances
- model lifecycle helpers
- visualization subsystems

unless we can say which future tranche no longer needs them.
