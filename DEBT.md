# Technical Debt

## ✅ COMPLETED

### Phase 2: Full Ratatui Port (COMPLETE - 2026-01-24)

All manual ANSI terminal rendering has been replaced with idiomatic ratatui widgets.

**Completed:**
- ✅ SpectrogramWidget, WaterfallWidget, StatusWidget, TranscriptWidget created
- ✅ Control panel ported to List + ListState
- ✅ Unified terminal.draw() loop
- ✅ Dead ANSI code removed
- ✅ 96 tests passing, cargo clippy clean

**Commits:**
- ef87568: feat(ui): add waterfall, status, transcript widgets
- 7cdbd07: feat(examples): add widget_test harness
- 34433bc: refactor(ui): port control panel to ratatui List
- ee18bd0: refactor(ui): unify terminal rendering under single draw() loop
- d89f352: refactor(ui): remove legacy ANSI rendering code

**Remaining:**
- Visual QA at multiple terminal sizes (user verification)
- Performance profiling if needed

## Ratatui Adoption

### Status
**In Progress**: Phase 1 (Spike) Completed 2026-01-24.
**Verdict**: GO for Phase 2.

### What Was Ported
- Control panel rendering (`render_control_panel` in `src/ui/terminal.rs`)
- Spectrogram Bar Meter (validated in `src/ui/spectrogram_widget.rs` via spike)
- Uses ratatui widgets: List, Block, ListItem, Custom SpectrogramWidget

### Metrics from Spike
- **Visual Match**: 100% parity with manual ANSI bar meter.
- **Performance**: 60fps stable in test harness.
- **Code Quality**: Replaces imperative cursor logic with declarative widgets.

### Notes
- ratatui uses crossterm as backend (already a dependency)
- MIT licensed (compatible with project)
- Active maintenance, good documentation
- No performance regression observed in testing
