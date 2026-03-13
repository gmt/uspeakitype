# Visual Testing Infrastructure for usit

This directory contains the visual regression testing infrastructure for usit's WGPU overlay mode. Tests capture screenshots via compositor tools and compare them against golden images using perceptual hashing.

## Prerequisites

### System Requirements

**Wayland Compositor** (wlroots-based):
- Sway, Hyprland, River, or other wlroots compositor
- `grim` - wlroots screenshot tool (via `wlr-screencopy-unstable-v1` protocol)

**Fonts** (for deterministic text rendering):
```bash
# Debian/Ubuntu
sudo apt-get install fonts-dejavu-core fonts-liberation fontconfig

# Arch Linux
sudo pacman -S ttf-dejavu ttf-liberation

# Verify installation
fc-list | grep -i "dejavu\|liberation"
```

**Image Processing** (for golden image generation):
```bash
# Debian/Ubuntu
sudo apt-get install imagemagick

# Arch Linux
sudo pacman -S imagemagick
```

**Software Rendering** (for CI without GPU):
```bash
# Debian/Ubuntu
sudo apt-get install mesa-utils libegl-mesa0 libgl1-mesa-dri mesa-vulkan-drivers libvulkan1 vulkan-tools

# Arch Linux
sudo pacman -S mesa vulkan-tools
```

## How to Run Visual Tests Locally

### On wlroots Compositor (Sway, Hyprland, River)

Visual tests are marked with `#[ignore]` and require explicit `--ignored` flag to run:

```bash
# Run all visual tests (single-threaded, required for screenshot capture)
cargo test --release --test visual_tests -- --ignored --nocapture --test-threads=1

# Run specific test
cargo test --release --test visual_tests -- test_demo_committed_hello --ignored --nocapture --test-threads=1

# Compile-only check (no execution)
cargo test --test visual_tests --no-run
```

**Important**: Always use `--test-threads=1` to prevent parallel test execution, which would interfere with screenshot capture.

### Expected Behavior

**On supported wlroots compositors**:
- Tests may **PASS** (screenshot matches golden image)
- Tests may **SKIP** (environmental issues like grim not found)

**On unsupported compositors** (GNOME, KDE, X11):
- Tests will **SKIP** with diagnostic message: "Skipping: not a verified wlroots compositor"

**On non-Wayland systems**:
- Tests will **SKIP** with diagnostic message: "Skipping: not running under Wayland"

## How to Run in CI with Headless Wayland

For reproducible golden images and CI testing, use headless Sway with controlled environment:

### Headless Sway Setup

```bash
#!/bin/bash
set -e

# PRECONDITION: No existing Wayland sessions
if ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null | grep -q .; then
    echo "ERROR: Existing Wayland sockets found. Run from clean TTY or container."
    exit 1
fi

# 1. Record existing sockets
BEFORE_SOCKETS=$(ls -1 "$XDG_RUNTIME_DIR" 2>/dev/null || true)

# 2. Start headless sway
export WLR_BACKENDS=headless
export WLR_HEADLESS_OUTPUTS=1
sway -c /dev/null &
SWAY_PID=$!
sleep 2

# 3. Discover newly created Wayland socket
AFTER_SOCKETS=$(ls -1 "$XDG_RUNTIME_DIR" 2>/dev/null || true)
NEW_WAYLAND=$(comm -13 <(echo "$BEFORE_SOCKETS" | sort) <(echo "$AFTER_SOCKETS" | sort) | grep '^wayland-' | head -1)
NEW_SWAYSOCK=$(comm -13 <(echo "$BEFORE_SOCKETS" | sort) <(echo "$AFTER_SOCKETS" | sort) | grep '^sway-ipc' | head -1)

[[ -n "$NEW_WAYLAND" ]] || { echo "ERROR: No Wayland socket created"; kill $SWAY_PID 2>/dev/null; exit 1; }

export WAYLAND_DISPLAY="$NEW_WAYLAND"
export SWAYSOCK="$XDG_RUNTIME_DIR/$NEW_SWAYSOCK"

# 4. Configure output (1920x1080, 1x scale, black background)
swaymsg output HEADLESS-1 resolution 1920x1080 scale 1 background "#000000" solid_color

# 5. Verify configuration
swaymsg -t get_outputs | grep -q '"width": 1920' || { echo "ERROR: Resolution not set"; exit 1; }

# 6. Isolate from user config (use defaults)
export XDG_CONFIG_HOME=$(mktemp -d)

# 7. Mark as canonical environment
export USIT_CANONICAL_TEST_ENV=1

# 8. Run tests
cargo test --release --test visual_tests -- --ignored --nocapture --test-threads=1

# 9. Cleanup
kill $SWAY_PID 2>/dev/null || true
rm -rf "$XDG_CONFIG_HOME"
```

### Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `WAYLAND_DISPLAY` | Wayland socket name | `wayland-0` |
| `SWAYSOCK` | Sway IPC socket path | `/run/user/1000/sway-ipc.1000.123.sock` |
| `XDG_CONFIG_HOME` | Config directory (set to temp for defaults) | `/tmp/tmpXXXXXX` |
| `USIT_CANONICAL_TEST_ENV` | Mark as canonical environment (CI) | `1` |
| `WLR_BACKENDS` | Force headless compositor | `headless` |
| `WLR_HEADLESS_OUTPUTS` | Number of headless outputs | `1` |

### Software Rendering (No GPU)

For CI environments without GPU:

```bash
# Force software rendering
export WLR_RENDERER=pixman
export LIBGL_ALWAYS_SOFTWARE=1
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json

# Verify Vulkan works
vulkaninfo --summary 2>/dev/null | grep -q "lavapipe" || echo "WARNING: lavapipe not detected"

# Run tests
cargo test --release --test visual_tests -- --ignored --nocapture --test-threads=1
```

### Example GitHub Actions Workflow

```yaml
name: Visual Tests

on: [push, pull_request]

jobs:
  visual-tests:
    runs-on: ubuntu-latest
    container:
      image: ubuntu:24.04
    steps:
      - uses: actions/checkout@v4
      
      - name: Install dependencies
        run: |
          apt-get update
          apt-get install -y \
            build-essential \
            cargo rustc \
            grim sway \
            fonts-dejavu-core fonts-liberation fontconfig \
            imagemagick \
            mesa-utils libegl-mesa0 libgl1-mesa-dri mesa-vulkan-drivers \
            libvulkan1 vulkan-tools
          fc-cache -f -v
      
      - name: Build usit
        run: cargo build --release
      
      - name: Run visual tests (headless)
        run: |
          # Setup headless Sway
          BEFORE=$(ls -1 "$XDG_RUNTIME_DIR" 2>/dev/null || true)
          export WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1
          sway -c /dev/null &
          SWAY_PID=$!
          sleep 2
          
          AFTER=$(ls -1 "$XDG_RUNTIME_DIR" 2>/dev/null || true)
          WAYLAND=$(comm -13 <(echo "$BEFORE" | sort) <(echo "$AFTER" | sort) | grep '^wayland-' | head -1)
          SWAYSOCK=$(comm -13 <(echo "$BEFORE" | sort) <(echo "$AFTER" | sort) | grep '^sway-ipc' | head -1)
          
          export WAYLAND_DISPLAY="$WAYLAND"
          export SWAYSOCK="$XDG_RUNTIME_DIR/$SWAYSOCK"
          export XDG_CONFIG_HOME=$(mktemp -d)
          export USIT_CANONICAL_TEST_ENV=1
          export WLR_RENDERER=pixman
          export LIBGL_ALWAYS_SOFTWARE=1
          
          swaymsg output HEADLESS-1 resolution 1920x1080 scale 1 background "#000000" solid_color
          
          # Run tests
          cargo test --release --test visual_tests -- --ignored --nocapture --test-threads=1
          
          # Cleanup
           kill $SWAY_PID 2>/dev/null || true
           rm -rf "$XDG_CONFIG_HOME"
```

## Docker Testing (Recommended for CI)

The Docker environment provides reproducible visual testing with software rendering.

### Quick Start

```bash
# Build and run all visual tests
docker compose run visual-tests

# Interactive shell for debugging
docker compose run shell

# Run specific test
docker compose run visual-tests cargo test --release --test visual_tests test_demo_partial_listening -- --ignored --nocapture
```

### Environment

The Docker image uses:
- Debian Bookworm (slim)
- Headless Sway compositor
- Software rendering (pixman + lavapipe)
- Fixed fonts (DejaVu, Liberation)
- 1920x1080 @ 1x scale

### Regenerating Golden Images

If golden images need updating for the Docker environment:

```bash
docker compose run shell bash tests/visual/scripts/capture_goldens.sh
```

## How to Update Golden Images

Golden images are reference screenshots used for regression testing. Update them when visual changes are intentional.

### Using the Capture Script

```bash
# Run from clean TTY (Ctrl+Alt+F2) or CI container
bash tests/visual/scripts/capture_goldens.sh
```

The script:
1. Starts headless Sway with canonical configuration
2. Isolates usit from user config (uses defaults)
3. Captures screenshots at demo milestones (t=3.0s, t=5.5s, t=7.5s)
4. Saves to `tests/visual/golden/`

### Manual Capture (Development)

For quick iteration during development:

```bash
# On Sway/Hyprland with usit running
usit --demo &
sleep 3.0
grim /tmp/demo_partial.png
sleep 2.5
grim /tmp/demo_committed.png
sleep 2.0
grim /tmp/demo_twotone.png
```

Then copy to `tests/visual/golden/` after visual inspection.

### Using KDE Spectacle (Phase 2)

KDE Plasma users can use Spectacle for development goldens:
```bash
spectacle -b -o tests/visual/golden/demo_partial_listening.png
```

(Full CI support for KDE is Phase 2)

## Troubleshooting Common Issues

### "Skipping: not running under Wayland"

**Cause**: `WAYLAND_DISPLAY` environment variable not set

**Solution**:
- Ensure you're running on a Wayland compositor (Sway, Hyprland, GNOME, KDE Plasma)
- Check: `echo $WAYLAND_DISPLAY` (should show `wayland-0` or similar)
- On X11 systems, tests will skip (expected)

### "Skipping: grim not found"

**Cause**: `grim` tool not installed or not in PATH

**Solution**:
```bash
# Debian/Ubuntu
sudo apt-get install grim

# Arch Linux
sudo pacman -S grim

# Verify
grim --version
```

### "Skipping: not a verified wlroots compositor"

**Cause**: Running on GNOME, KDE, or other non-wlroots Wayland compositor

**Solution**:
- Tests only support wlroots compositors (Sway, Hyprland, River) in Phase 1
- KDE/spectacle support is Phase 2
- GNOME support requires xdg-desktop-portal (Phase 2+)
- Use headless Sway for CI: see "How to Run in CI with Headless Wayland"

### "hash mismatch" errors

**Cause**: Screenshot differs from golden image (distance > 10)

**Possible reasons**:
- **Different fonts**: Install `fonts-dejavu-core` and `fonts-liberation`
- **Different resolution/scale**: Ensure 1920x1080 @ 1x scale
- **Different background**: Use black background (`#000000`)
- **Different theme/config**: Set `XDG_CONFIG_HOME` to temp directory
- **Timing jitter**: Increase milestone sleep times (e.g., 3.5s instead of 3.0s)
- **Real regression**: Visual output changed (investigate and update golden)

**Debugging**:
```bash
# Run with verbose output
cargo test --release --test visual_tests -- --ignored --nocapture --test-threads=1 2>&1 | grep -A5 "distance"

# Manually compare
grim /tmp/current.png
# Compare /tmp/current.png with tests/visual/golden/demo_committed_hello.png visually
```

### "CANONICAL: screenshot differs"

**Cause**: Test running in canonical environment (CI) with hash mismatch

**Solution**:
- This is a real failure - investigate the regression
- Check if usit code changed (visual output)
- Check if fonts/theme changed in CI environment
- Update golden images if change is intentional: `bash tests/visual/scripts/capture_goldens.sh`

### usit crashes under headless Sway

**Cause**: Missing dependencies or rendering issues

**Solution**:
```bash
# Verify usit starts
timeout 5 usit --demo &
sleep 2
ps aux | grep usit  # Should still be running

# Check for errors
cargo run --release -- --demo 2>&1 | head -20

# Verify software rendering
export WLR_RENDERER=pixman
export LIBGL_ALWAYS_SOFTWARE=1
cargo run --release -- --demo
```

## Architecture

### Test Structure

```
tests/
├── visual_tests.rs          # Integration test entry point
└── visual/
    ├── mod.rs               # Module declarations
    ├── screenshot.rs        # Compositor detection + grim capture
    ├── comparison.rs        # Perceptual hash comparison
    ├── wgpu_harness.rs      # Test harness (spawn + capture orchestration)
    ├── golden/              # Reference images (committed)
    │   ├── demo_partial_listening.png
    │   ├── demo_committed_hello.png
    │   └── demo_twotone_streaming.png
    ├── fixtures/            # Test fixture images (for threshold validation)
    │   ├── baseline.png
    │   ├── baseline_similar.png
    │   └── completely_different.png
    └── scripts/
        └── capture_goldens.sh  # Golden image capture script
```

### Key Concepts

**Compositor Detection**: Tests detect wlroots compositors via environment variables (`SWAYSOCK`, `HYPRLAND_INSTANCE_SIGNATURE`, `RIVER_SOCKET`) rather than relying on `grim` presence alone.

**Perceptual Hashing**: Uses gradient-based hashing (via `image_hasher` crate) to tolerate minor anti-aliasing and timing differences while catching real regressions.

**Canonical Environment**: Tests behave differently based on `USIT_CANONICAL_TEST_ENV`:
- **Canonical (CI)**: Failures panic (real bugs must be caught)
- **Non-canonical (dev)**: Failures skip (environmental differences expected)

**Config Isolation**: Tests set `XDG_CONFIG_HOME` to temp directory to ensure usit uses default settings, making golden images reproducible across machines.

## Demo Mode Timeline

usit's demo mode generates synthetic transcription events at specific times:

| Time | Event | Visual State |
|------|-------|--------------|
| 2.0s | set_partial("Listening...") | Gray "Listening..." |
| 4.0s | set_partial("Hello world") | Gray "Hello world" |
| 5.0s | commit() | White "Hello world" (committed) |
| 6.0s | set_partial("this is streaming") | White + Gray two-tone |
| 7.0s | commit() + set_partial("transcription") | White "Hello world this is streaming" + Gray "transcription" |

**Test Milestones** (with 1.0s startup margin):
- **t=3.0s**: Partial-only state (gray "Listening...")
- **t=5.5s**: Committed-only state (white "Hello world")
- **t=7.5s**: Two-tone state (white committed + gray partial)

## References

- **Spec**: [`docs/testing-visual.md`](../../docs/testing-visual.md) - Stable visual-testing contract and rationale
- **Demo mode**: `src/main.rs:521-575` - Synthetic event timeline
- **Layer shell config**: `src/ui/app.rs:353-355` - Wayland layer shell setup
- **Text rendering**: `src/ui/text_renderer.rs` - WGPU text rendering
