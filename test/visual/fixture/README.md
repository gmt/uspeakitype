# Visual Test Fixtures

This directory contains synthetic images for testing perceptual hash comparison.

## Fixture Images

### baseline.png
- **Description**: White rectangle (500x300) centered on black background (1920x1080)
- **Position**: Rectangle at (710, 390) to (1210, 690)
- **Purpose**: Reference image for comparison tests

### baseline_similar.png
- **Description**: White rectangle shifted by 10 pixels from baseline
- **Position**: Rectangle at (720, 400) to (1220, 700)
- **Purpose**: Test "similar enough" detection (minor visual difference)

### completely_different.png
- **Description**: Checkerboard pattern (1920x1080)
- **Purpose**: Test "clearly different" detection

## Measured Hamming Distances

Using `image_hasher` with `HashAlg::Gradient`:

| Comparison | Distance | Pass Threshold (≤10) | Result |
|------------|----------|----------------------|--------|
| baseline.png vs baseline.png | 0 | ✓ | PASS |
| baseline.png vs baseline_similar.png | 2 | ✓ | PASS |
| baseline.png vs completely_different.png | 28 | ✗ | FAIL |

## Regenerating Fixtures

```bash
cd test/visual/fixture

# Baseline: white rectangle on black
magick -size 1920x1080 xc:black -fill white -draw "rectangle 710,390 1210,690" baseline.png

# Similar: shifted rectangle
magick -size 1920x1080 xc:black -fill white -draw "rectangle 720,400 1220,700" baseline_similar.png

# Different: checkerboard pattern
magick -size 1920x1080 pattern:checkerboard -resize 1920x1080! completely_different.png
```

## Test Coverage

These fixtures validate:
1. **Identical images** (distance = 0): Self-comparison
2. **Similar images** (distance < threshold): Minor position shifts
3. **Different images** (distance > threshold): Completely different content

The threshold of 10 was chosen to allow minor rendering variations while catching significant visual differences.
