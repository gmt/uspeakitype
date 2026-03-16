# Golden Reference Images

This directory contains golden reference images for visual regression testing.

## Status: DEVELOPMENT GOLDENS (Non-Canonical)

**Current goldens:** Captured on KDE Plasma using spectacle at 5120x1440 resolution.

**Canonical goldens:** Must be recaptured in headless Sway at 1920x1080 for CI/CD.

These development goldens are useful for:
- Local visual regression testing during development
- Verifying usit's overlay renders correctly on KDE
- Perceptual hash comparison (tolerates resolution differences)

However, they are **not suitable for pixel-perfect CI testing** due to:
- Non-standard resolution (5120x1440 vs expected 1920x1080)
- KDE compositor differences vs headless Sway
- Potential wallpaper/theme interference

## Why Goldens Can't Be Captured on KDE

The capture script (`../scripts/capture_goldens.sh`) requires:
1. A clean environment with NO existing Wayland sockets
2. Headless Sway compositor for deterministic rendering
3. Controlled 1920x1080 resolution with black background

Running from within a KDE session fails because:
- Existing `wayland-0` socket conflicts with headless Sway
- KDE's compositor would interfere with screenshot capture
- Background, resolution, and scale would not be deterministic

## How to Capture Goldens

### Option 1: Switch to TTY (Recommended)

```bash
# 1. Switch to a virtual terminal (Ctrl+Alt+F2)
# 2. Log in
# 3. Navigate to usit repo
cd /home/greg/src/usit

# 4. Run the capture script
./test/visual/script/capture_goldens.sh

# 5. Return to KDE (Ctrl+Alt+F1 or F7)
```

### Option 2: Use a Container

```bash
# Run in a container with sway and grim installed
podman run --rm -it \
  -v /home/greg/src/usit:/workspace \
  -w /workspace \
  archlinux:latest \
  bash -c "pacman -Sy --noconfirm sway grim cargo && ./test/visual/script/capture_goldens.sh"
```

### Option 3: CI Environment

The CI pipeline will capture goldens automatically in a headless environment.

## Current Golden Files

### Demo Mode Goldens

| File | Capture Time | Expected Content | Status |
|------|--------------|------------------|--------|
| `demo_partial_listening.png` | t=3.0s | Gray "Listening..." text | ✓ Captured (5120x1440, KDE) |
| `demo_committed_hello.png` | t=5.5s | White "Hello world" text | ✓ Captured (5120x1440, KDE) |
| `demo_twotone_streaming.png` | t=7.5s | White committed + Gray partial text | ✓ Captured (5120x1440, KDE) |

### WGPU Enhancement Goldens

| File | Capture Time | Expected Content | Status |
|------|--------------|------------------|--------|
| `wgpu_opacity_half.png` | t=3.0s | Window at 50% opacity | ✓ Captured (5120x1440, KDE) |
| `wgpu_control_panel_full.png` | t=3.0s | Control panel with all 10 controls | ✓ Captured (5120x1440, KDE) |

### Capture Details

- **Environment:** KDE Plasma 6
- **Tool:** spectacle (KDE's screenshot utility)
- **Resolution:** 5120x1440 (ultra-wide)
- **Format:** PNG, 8-bit RGBA
- **Date:** 2026-01-26

### Recapture for CI

For canonical CI goldens at 1920x1080, use the headless Sway capture script (see below).

## Verification

Current images:
```bash
$ file test/visual/golden/*.png
demo_committed_hello.png:        PNG image data, 5120 x 1440, 8-bit/color RGBA, non-interlaced
demo_partial_listening.png:      PNG image data, 5120 x 1440, 8-bit/color RGBA, non-interlaced
demo_twotone_streaming.png:      PNG image data, 5120 x 1440, 8-bit/color RGBA, non-interlaced
wgpu_control_panel_full.png:     PNG image data, 5120 x 1440, 8-bit/color RGBA, non-interlaced
wgpu_opacity_half.png:      PNG image data, 5120 x 1440, 8-bit/color RGBA, non-interlaced
```

Visual inspection should show:

**Demo mode goldens:**
- usit's overlay window with spectrogram at bottom
- Text in correct color (gray for partial, white for committed)
- Transparent background (overlay composited over desktop)

**WGPU enhancement goldens:**
- `wgpu_opacity_half.png`: Window at 50% opacity (more background visible)
- `wgpu_control_panel_full.png`: Control panel visible with all 10 controls including TransparencySlider

**Note:** For canonical CI goldens, expect:
- Resolution: 1920 x 1080
- Black background (no wallpaper)
- Deterministic rendering from headless Sway
