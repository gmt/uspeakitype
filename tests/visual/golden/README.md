# Golden reference images

These 1920×1080 PNG files are reference screenshots for the WGPU overlay.
Tests compare gradient perceptual hashes with a maximum Hamming distance of 10.
They also compare selected status, transcript and panel regions between states;
these assertions catch missing content independently of the full-screen hash.

| File | Expected content |
| --- | --- |
| `demo_partial_listening.png` | Gray partial text, "Listening..." |
| `demo_committed_hello.png` | White committed text, "Hello world" |
| `demo_twotone_streaming.png` | White committed and gray partial text |
| `wgpu_opacity_half.png` | Overlay at 50% opacity |
| `wgpu_control_panel_full.png` | Open input-helper controls and contextual help |

Use the repository's isolated Sway environment for reproducible capture:

```sh
docker compose run --build --rm visual-tests
```

This environment supplies grim, software rendering, fixed fonts and a 1920×1080
output. It does not connect to the host compositor. Screenshot artifacts are
written to `test-output/`, including failed comparisons. The control-panel test
sets a black background explicitly; opacity tests use pink to measure visibility
through the overlay.

To update a reference, inspect the corresponding artifact and confirm that the
expected text, controls and layout are visible without overlap or clipping.
Copy only that accepted screenshot over its matching reference and rerun the
test. A mismatch alone is not grounds for replacing a reference or increasing
the threshold. Capturing one overlay at a time is essential: grim captures the
whole output, including every overlapping surface.
