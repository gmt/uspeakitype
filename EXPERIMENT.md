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
- [ ] Control panel ported to ratatui
- [ ] Evaluation complete
- [ ] Decision documented

## Notes
- ratatui uses crossterm as backend (already a dependency)
- MIT licensed (compatible)
- Active maintenance, good documentation
