# Technical Debt

## Ratatui Partial Adoption

### Status
**Adopted**: 2026-01-21

### What Was Ported
- Control panel rendering (`render_control_panel` in `src/ui/terminal.rs`)
- Uses ratatui widgets: List, Block, ListItem
- Eliminates manual coordinate calculations for panel
- Reduces LOC by 28% (34 lines saved)

### What Was NOT Ported
- Spectrogram rendering (bar meter and waterfall)
- Status line rendering
- Transcript text rendering
- Terminal initialization and cleanup (still uses crossterm directly)

## Metrics from Evaluation
- **LOC reduction**: 34 lines (28% fewer lines in render_control_panel)
- **Coordinate calculations**: 7 fewer cursor_to() calls (35% reduction)
- **Manual padding**: Eliminated (100% reduction)
- **Maintainability**: Significantly improved (declarative vs imperative)

### Remaining Work
If we decide to complete the ratatui adoption:

1. **Port spectrogram rendering**:
   - Bar meter: Use ratatui's `BarChart` or custom widget
   - Waterfall: Use `Canvas` widget with custom drawing
   - Estimated effort: 4-8 hours
   - Complexity: High (requires understanding current spectrogram layout)

2. **Port status line**:
   - Use `Paragraph` widget
   - Estimated effort: 1 hour
   - Complexity: Low

3. **Port transcript text**:
   - Use `Paragraph` with styled spans
   - Estimated effort: 2 hours
   - Complexity: Low

4. **Unify terminal management**:
   - Consolidate ratatui terminal usage
   - Remove mixed rendering approach (ratatui for panel, manual ANSI for spectrogram)
   - Estimated effort: 2-4 hours
   - Complexity: Medium

**Total estimated effort for full port**: 9-15 hours

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
