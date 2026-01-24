# Technical Debt

## Ratatui Adoption

### Status
**In Progress**: Phase 1 (Spike) Completed 2026-01-24.
**Verdict**: GO for Phase 2.

### What Was Ported
- Control panel rendering (`render_control_panel` in `src/ui/terminal.rs`)
- Spectrogram Bar Meter (validated in `src/ui/spectrogram_widget.rs` via spike)
- Uses ratatui widgets: List, Block, ListItem, Custom SpectrogramWidget

### Remaining Work (Phase 2)
1. **Unify Terminal Management**: Remove mixed rendering (manual ANSI + Ratatui).
2. **Port Status Line**: Use `Paragraph` widget.
3. **Port Transcript Text**: Use `Paragraph` with styled spans for two-tone (committed/partial) text.
4. **Port Waterfall Display**: Implement as a Ratatui widget.

### Metrics from Spike
- **Visual Match**: 100% parity with manual ANSI bar meter.
- **Performance**: 60fps stable in test harness.
- **Code Quality**: Replaces imperative cursor logic with declarative widgets.


### Current Issues
- **Mixed rendering**: Control panel uses ratatui, spectrogram uses manual ANSI
  - Requires careful coordination of terminal state
  - Both systems manage cursor position and styling
  - Potential for conflicts if not carefully managed

### Decision Points
- **Keep partial adoption**: Accept mixed rendering approach, document it well
- **Complete full port**: Allocate 9-15 hours for remaining work

If TUI complexity grows significantly in the future (e.g., adding new panels, complex layouts), complete the full port to unify under ratatui.

### Notes
- ratatui uses crossterm as backend (already a dependency)
- MIT licensed (compatible with project)
- Active maintenance, good documentation
- No performance regression observed in testing
