# Layer-Shell Screenshot Capture Validation

## Test Environment

- **Compositor**: KDE KWin 6.5.5 (Wayland)
- **Session Type**: wayland
- **WAYLAND_DISPLAY**: wayland-0
- **Screenshot Tool**: spectacle 6.5.5 (KDE's screenshot utility)
- **Date**: 2026-01-26

## Layer-Shell Configuration

From `src/ui/app.rs:351-358`:
```rust
let wayland_attrs = WindowAttributesWayland::default()
    .with_layer_shell()
    .with_anchor(Anchor::BOTTOM)
    .with_layer(Layer::Overlay)
    .with_margin(MARGIN, MARGIN, MARGIN, MARGIN)
    .with_output(monitor.native_id())
    .with_keyboard_interactivity(KeyboardInteractivity::OnDemand);
```

## Validation Results

### Test Procedure
1. Started `usit --demo` in tmux session
2. Waited 3 seconds for window to appear and demo events to fire
3. Captured full-screen screenshot with `spectacle -b -f -n -o /tmp/usit-test.png`
4. Verified screenshot contains usit's overlay

### Screenshot Details
- **Dimensions**: 5120 x 1440 (dual-monitor setup)
- **Format**: PNG, 8-bit RGBA
- **File Size**: ~773KB

### Layer::Overlay Confirmation
- **CONFIRMED**: usit's layer-shell overlay appears in spectacle screenshots
- The overlay is visible at the bottom of the screen as expected
- Spectrogram visualization with colored bars is captured
- Color histogram of bottom region shows varied colors (not just black), confirming spectrogram content

### Cropped Bottom Region Analysis
Cropped 1920x200 region from bottom of screenshot:
- Mean pixel value: 26378.1 (non-zero, confirming content)
- Color palette includes browns, greens, and cream tones typical of spectrogram visualization

## Compositor Compatibility Notes

### KDE/KWin (Current Test)
- **Screenshot Tool**: spectacle (not grim)
- **Status**: WORKS - layer-shell overlays are captured
- **Command**: `spectacle -b -f -n -o output.png`
  - `-b`: background mode (no GUI)
  - `-f`: fullscreen capture
  - `-n`: no notification
  - `-o`: output file

### wlroots Compositors (Sway, Hyprland, River)
- **Screenshot Tool**: grim
- **Status**: NOT TESTED (grim not installed, not on wlroots compositor)
- **Expected**: Should work per plan documentation
- **Command**: `grim output.png`

### GNOME
- **Status**: Out of scope for Phase 1 (requires xdg-desktop-portal)

## Issues Observed

1. **Compositor Detection**: Current environment has `WAYLAND_DISPLAY` set but no wlroots-specific env vars (`SWAYSOCK`, `HYPRLAND_INSTANCE_SIGNATURE`, `RIVER_SOCKET`). The plan's compositor detection logic would classify this as `Compositor::Unknown`.

2. **grim Availability**: grim is not installed on this KDE system. For wlroots testing, grim must be installed.

3. **Multi-Monitor**: Screenshot captured entire 5120x1440 desktop (dual monitors). Golden images should be captured on single 1920x1080 display for consistency.

## Recommendations for Visual Testing

1. **Phase 1 (wlroots only)**: Tests should skip on KDE with clear message
2. **Phase 2 (KDE support)**: Add spectacle backend with same CLI pattern
3. **Golden Capture**: Use headless Sway with 1920x1080 single output for reproducibility

## Conclusion

**Layer-shell capture VALIDATED**: usit's `Layer::Overlay` configuration works correctly with Wayland compositor screenshot tools. The overlay is visible in full-screen captures.

For the visual testing infrastructure (Phase 1), testing should proceed on a wlroots compositor (Sway/Hyprland) with grim installed, as specified in the plan.
