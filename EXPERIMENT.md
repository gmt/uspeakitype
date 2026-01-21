# Ratatui TUI Framework Evaluation

## Purpose
Evaluate ratatui as a potential replacement for manual ANSI terminal rendering in Barbara's control panel.

## Scope
- **In scope**: Port control panel rendering (`render_control_panel` in `src/ui/terminal.rs`)
- **Out of scope**: Spectrogram rendering, full application port

## Goals
1. Reduce manual coordinate calculations
2. Simplify layout management
3. Improve code maintainability
4. Assess performance impact

## Evaluation Criteria
- **Code complexity**: LOC delta, manual calculations reduced
- **Maintainability**: Easier to modify/extend?
- **Performance**: Any noticeable lag?
- **Integration**: How well does it fit with existing code?

## Timeline
- Time-boxed to 2 hours for porting
- Decision: adopt (merge to main) or abandon (delete branch)

## Status
- [x] Branch created
- [x] Control panel ported to ratatui
- [x] Evaluation complete
- [x] Decision documented

## Metrics

### Before (Manual ANSI)
- **LOC**: 123 lines in render_control_panel (lines 585-707)
- **Manual coordinate calculations**: 20 cursor_to() calls in entire file
- **Manual padding**: 1 .repeat() call in entire file
- **Complexity**: High (manual positioning, padding, borders, ANSI codes)

### After (Ratatui Widgets)
- **LOC**: 89 lines in render_control_panel (lines 602-690)
- **Manual coordinate calculations**: 13 cursor_to() calls in entire file (7 fewer)
- **Manual padding**: 0 .repeat() calls in entire file (1 fewer)
- **Complexity**: Low (declarative widget composition)

### Delta
- **LOC reduction**: 34 lines (28% fewer lines)
- **Coordinate calculations**: 7 fewer cursor_to() calls (35% reduction)
- **Manual padding**: Eliminated (100% reduction)
- **Maintainability**: Significantly improved

## Observations

### Advantages of Ratatui
1. **Declarative**: Widget-based approach is more intuitive than manual ANSI codes
2. **Layout management**: Ratatui handles positioning automatically (no cursor_to needed)
3. **Styling**: Built-in support for colors, modifiers (REVERSED for focus)
4. **Borders**: Block widget handles borders automatically
5. **Less error-prone**: No manual padding calculations or coordinate math
6. **Cleaner code**: Easier to read and understand intent

### Disadvantages / Challenges
1. **Terminal management**: Ratatui expects to manage the entire terminal, but Barbara uses manual ANSI for spectrogram
2. **Integration complexity**: Mixing ratatui with manual ANSI rendering requires careful coordination
3. **Performance**: Ratatui's draw() call may have overhead vs direct ANSI output
4. **Partial adoption**: Only porting control panel leaves inconsistency (ratatui for panel, manual for spectrogram)

### Technical Notes
- Ratatui terminal initialized in init_terminal() method
- Panel area calculated using panel_geometry() helper
- List widget with ListItem for each control
- REVERSED modifier for focused control (equivalent to manual ANSI 7m)
- Block widget with borders and title

## Decision: ADOPT (with caveats)

**Recommendation**: Merge to main with the following understanding:
- Ratatui simplifies control panel rendering significantly
- Code is more maintainable and less error-prone
- Performance impact is minimal (single draw() call per frame)
- Future work: Consider full application port if spectrogram rendering is refactored

**Remaining work** (out of scope for this task):
- [ ] Port spectrogram rendering to ratatui (complex, requires layout refactoring)
- [ ] Unify terminal management (currently mixed manual ANSI + ratatui)
- [ ] Remove dead code (draw_keybind_hints, safe_truncate if no longer needed)

## Notes
- ratatui uses crossterm as backend (already a dependency)
- MIT licensed (compatible)
- Active maintenance, good documentation
