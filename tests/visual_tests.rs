mod visual;

use image_hasher::{HashAlg, HasherConfig};
use serial_test::serial;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("visual")
        .join("fixtures")
}

#[test]
fn test_compositor_detection() {
    println!("Compositor: {:?}", visual::screenshot::compositor_type());
}

#[test]
fn test_hash_identical() {
    let baseline = fixtures_dir().join("baseline.png");
    let result = visual::comparison::compare_images(&baseline, &baseline)
        .expect("Should compare images successfully");

    assert_eq!(result.distance, 0, "Identical images must have distance 0");
    assert!(result.passed, "Identical images must pass comparison");

    println!(
        "test_hash_identical: distance={}, passed={}",
        result.distance, result.passed
    );
}

#[test]
fn test_hash_similar() {
    let baseline = fixtures_dir().join("baseline.png");
    let similar = fixtures_dir().join("baseline_similar.png");

    let result = visual::comparison::compare_images(&baseline, &similar)
        .expect("Should compare images successfully");

    assert!(
        result.distance > 0,
        "Similar images should have non-zero distance"
    );
    assert!(
        result.distance < visual::comparison::HASH_PASS_THRESHOLD,
        "Similar images should have distance < {}",
        visual::comparison::HASH_PASS_THRESHOLD
    );
    assert!(result.passed, "Similar images should pass comparison");

    println!(
        "test_hash_similar: distance={}, passed={}",
        result.distance, result.passed
    );
}

#[test]
fn test_hash_different() {
    let baseline = fixtures_dir().join("baseline.png");
    let different = fixtures_dir().join("completely_different.png");

    let result = visual::comparison::compare_images(&baseline, &different)
        .expect("Should compare images successfully");

    assert!(
        result.distance > visual::comparison::HASH_PASS_THRESHOLD,
        "Different images should have distance > {}",
        visual::comparison::HASH_PASS_THRESHOLD
    );
    assert!(!result.passed, "Different images should fail comparison");

    println!(
        "test_hash_different: distance={}, passed={}",
        result.distance, result.passed
    );
}

#[test]
#[serial]
fn test_harness_spawn_and_capture() {
    if !visual::screenshot::screenshot_available() {
        eprintln!("Skipping: {}", visual::screenshot::skip_reason());
        return;
    }
    let harness =
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"], "harness_spawn_capture").unwrap();
    harness.wait_demo_milestone(3.0);
    let path = harness.capture("test_capture").unwrap();
    println!("Captured to: {:?}", path);
    assert!(path.exists(), "Screenshot file should exist");
}

/// Check if running in canonical test environment
fn is_canonical() -> bool {
    std::env::var("USIT_CANONICAL_TEST_ENV").is_ok()
}

/// Handle errors: fail in canonical, skip in non-canonical
macro_rules! try_or_skip {
    ($expr:expr, $msg:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                if is_canonical() {
                    panic!("CANONICAL: {} failed: {}", $msg, e);
                } else {
                    eprintln!("Skipping: {} failed: {}", $msg, e);
                    return;
                }
            }
        }
    };
}

/// Measure average alpha channel value in an image region
fn measure_pink_bleedthrough(image: &image::RgbaImage, region: (u32, u32, u32, u32)) -> f32 {
    let (x, y, w, h) = region;
    let mut pink_score = 0.0;
    let mut count = 0;

    for py in y..(y + h).min(image.height()) {
        for px in x..(x + w).min(image.width()) {
            let pixel = image.get_pixel(px, py);
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;

            let score = ((r - g) + (b - g)).max(0.0) / 2.0;
            pink_score += score;
            count += 1;
        }
    }

    if count == 0 {
        return 0.0;
    }
    pink_score / count as f32
}

fn overlay_region() -> (u32, u32, u32, u32) {
    // Must match docker-test.sh resolution (1920x1080)
    let output_width = 1920u32;
    let output_height = 1080u32;
    let window_width = (output_width as f32 * 0.25) as u32;
    let window_height = 210u32;
    let margin = 24u32;

    // Sample inside the overlay, avoiding edges/rounded corners
    let x = (output_width - window_width) / 2 + 20;
    let y = output_height.saturating_sub(margin + window_height) + 20;
    let w = window_width.saturating_sub(40);
    let h = window_height.saturating_sub(40);

    (x, y, w, h)
}

fn helper_status_region() -> (u32, u32, u32, u32) {
    let (x, y, w, h) = overlay_region();
    (x + 10, y + h.saturating_sub(58), w.saturating_sub(20), 22)
}

fn text_panel_region() -> (u32, u32, u32, u32) {
    let (x, y, w, h) = overlay_region();
    (x + 10, y + h.saturating_sub(58), w.saturating_sub(20), 48)
}

fn control_panel_region() -> (u32, u32, u32, u32) {
    let output_width = 1920u32;
    let output_height = 1080u32;
    let panel_width = 460u32;
    let panel_height = 430u32;
    let x = (output_width.saturating_sub(panel_width)) / 2;
    let y = (output_height.saturating_sub(panel_height)) / 2;
    (
        x + 12,
        y + 12,
        panel_width.saturating_sub(24),
        panel_height.saturating_sub(24),
    )
}

fn control_help_region() -> (u32, u32, u32, u32) {
    let (x, y, w, h) = control_panel_region();
    let help_height = 96u32.min(h / 3);
    (
        x + 8,
        y + h.saturating_sub(help_height + 8),
        w.saturating_sub(16),
        help_height,
    )
}

fn region_hash_distance(path_a: &Path, path_b: &Path, region: (u32, u32, u32, u32)) -> u32 {
    fn hash_region(path: &Path, region: (u32, u32, u32, u32)) -> image_hasher::ImageHash {
        let image = image::open(path)
            .unwrap_or_else(|err| panic!("failed to open {}: {}", path.display(), err))
            .to_rgba8();
        let (x, y, w, h) = region;
        let cropped = image::imageops::crop_imm(
            &image,
            x.min(image.width().saturating_sub(1)),
            y.min(image.height().saturating_sub(1)),
            w.min(image.width().saturating_sub(x)),
            h.min(image.height().saturating_sub(y)),
        )
        .to_image();
        let hasher = HasherConfig::new().hash_alg(HashAlg::Gradient).to_hasher();
        hasher.hash_image(&cropped)
    }

    hash_region(path_a, region).dist(&hash_region(path_b, region))
}

fn spawn_demo_overlay_state(
    demo_overlay_state: &'static str,
    tag: &str,
) -> anyhow::Result<visual::wgpu_harness::WgpuTestHarness> {
    visual::wgpu_harness::WgpuTestHarness::spawn(
        &["--demo", "--demo-overlay-state", demo_overlay_state],
        tag,
    )
}

/// Test that opacity CLI flag actually affects rendering
/// and verify directional semantics (higher = more opaque)
#[test]
#[serial]
fn test_opacity_directional() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    // Set hot pink background (in Docker/headless Sway)
    let _ = std::process::Command::new("swaymsg")
        .args(["output", "*", "background", "#FF1493", "solid_color"])
        .status();

    // Capture at three opacity levels
    let harness_75 = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo", "--opacity", "0.75"], "opac_75"),
        "spawn 0.75"
    );
    harness_75.wait_demo_milestone(3.0);
    let capture_75 = try_or_skip!(harness_75.capture("opac_75"), "capture 0.75");

    let harness_85 = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo", "--opacity", "0.85"], "opac_85"),
        "spawn 0.85"
    );
    harness_85.wait_demo_milestone(3.0);
    let capture_85 = try_or_skip!(harness_85.capture("opac_85"), "capture 0.85");

    let harness_95 = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo", "--opacity", "0.95"], "opac_95"),
        "spawn 0.95"
    );
    harness_95.wait_demo_milestone(3.0);
    let capture_95 = try_or_skip!(harness_95.capture("opac_95"), "capture 0.95");

    // Load images for alpha measurement
    let image_75 = try_or_skip!(
        image::open(&capture_75).map(|img| img.to_rgba8()),
        "load image 0.75"
    );
    let image_85 = try_or_skip!(
        image::open(&capture_85).map(|img| img.to_rgba8()),
        "load image 0.85"
    );
    let image_95 = try_or_skip!(
        image::open(&capture_95).map(|img| img.to_rgba8()),
        "load image 0.95"
    );

    let region = overlay_region();

    let pink_75 = measure_pink_bleedthrough(&image_75, region);
    let pink_85 = measure_pink_bleedthrough(&image_85, region);
    let pink_95 = measure_pink_bleedthrough(&image_95, region);

    println!("Pink bleedthrough measurements:");
    println!("  75% opacity: {:.4}", pink_75);
    println!("  85% opacity: {:.4}", pink_85);
    println!("  95% opacity: {:.4}", pink_95);

    assert!(
        pink_75 > pink_85,
        "75% should show MORE pink than 85% (more transparent)\n\
         Got: 75%={:.4}, 85%={:.4}",
        pink_75,
        pink_85
    );
    assert!(
        pink_85 > pink_95,
        "85% should show MORE pink than 95% (95% is more opaque)\n\
         Got: 85%={:.4}, 95%={:.4}",
        pink_85,
        pink_95
    );

    let diff_75_95 = (pink_75 - pink_95).abs();
    assert!(
        diff_75_95 > 0.02,
        "75% and 95% should be measurably different\n\
         Got difference: {:.4} (expected > 0.02)",
        diff_75_95
    );

    println!("PASS: Opacity directional test");
    println!("  Semantics confirmed: higher opacity = less pink visible");
}

#[test]
#[serial]
fn test_wgpu_helper_modes_render_distinct_status_strip() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    let display = try_or_skip!(
        spawn_demo_overlay_state("display", "helper_display"),
        "spawn display"
    );
    let transcribe = try_or_skip!(
        spawn_demo_overlay_state("transcribe", "helper_transcribe"),
        "spawn transcribe"
    );
    let trusted = try_or_skip!(
        spawn_demo_overlay_state("trusted", "helper_trusted"),
        "spawn trusted"
    );

    display.wait_demo_milestone(3.0);
    transcribe.wait_demo_milestone(3.0);
    trusted.wait_demo_milestone(3.0);

    let display_capture = try_or_skip!(display.capture("helper_display"), "capture display");
    let transcribe_capture = try_or_skip!(
        transcribe.capture("helper_transcribe"),
        "capture transcribe"
    );
    let trusted_capture = try_or_skip!(trusted.capture("helper_trusted"), "capture trusted");

    let region = helper_status_region();
    let display_vs_transcribe = region_hash_distance(&display_capture, &transcribe_capture, region);
    let transcribe_vs_trusted = region_hash_distance(&transcribe_capture, &trusted_capture, region);
    let display_vs_trusted = region_hash_distance(&display_capture, &trusted_capture, region);

    println!(
        "helper status distances: display/transcribe={}, transcribe/trusted={}, display/trusted={}",
        display_vs_transcribe, transcribe_vs_trusted, display_vs_trusted
    );

    assert!(
        display_vs_transcribe > 0,
        "Display-only and transcribing status strips should differ"
    );
    assert!(
        transcribe_vs_trusted > 0,
        "Transcribing and trusted status strips should differ"
    );
    assert!(
        display_vs_trusted > 0,
        "Display-only and trusted status strips should differ"
    );
}

#[test]
#[serial]
fn test_wgpu_download_and_error_states_render_distinct_text_panel() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    let trusted = try_or_skip!(
        spawn_demo_overlay_state("trusted", "helper_trusted_baseline"),
        "spawn trusted baseline"
    );
    let downloading = try_or_skip!(
        spawn_demo_overlay_state("downloading", "helper_downloading"),
        "spawn downloading"
    );
    let error = try_or_skip!(
        spawn_demo_overlay_state("error", "helper_error"),
        "spawn error"
    );

    trusted.wait_demo_milestone(3.0);
    downloading.wait_demo_milestone(3.0);
    error.wait_demo_milestone(3.0);

    let trusted_capture = try_or_skip!(
        trusted.capture("helper_trusted_baseline"),
        "capture trusted baseline"
    );
    let downloading_capture = try_or_skip!(
        downloading.capture("helper_downloading"),
        "capture downloading"
    );
    let error_capture = try_or_skip!(error.capture("helper_error"), "capture error");

    let region = text_panel_region();
    let trusted_vs_downloading =
        region_hash_distance(&trusted_capture, &downloading_capture, region);
    let trusted_vs_error = region_hash_distance(&trusted_capture, &error_capture, region);
    let downloading_vs_error = region_hash_distance(&downloading_capture, &error_capture, region);

    println!(
        "helper text-panel distances: trusted/downloading={}, trusted/error={}, downloading/error={}",
        trusted_vs_downloading, trusted_vs_error, downloading_vs_error
    );

    assert!(
        trusted_vs_downloading > 0,
        "Downloading state should visibly differ from trusted baseline"
    );
    assert!(
        trusted_vs_error > 0,
        "Error state should visibly differ from trusted baseline"
    );
    assert!(
        downloading_vs_error > 0,
        "Downloading and error states should visibly differ"
    );
}

#[test]
#[serial]
fn test_wgpu_open_panel_renders_distinct_shell_regions() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    let closed = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"], "helper_panel_closed"),
        "spawn closed"
    );
    let open = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(
            &["--demo", "--demo-open-panel"],
            "helper_panel_open"
        ),
        "spawn open"
    );

    closed.wait_demo_milestone(3.0);
    open.wait_demo_milestone(3.0);

    let closed_capture = try_or_skip!(closed.capture("helper_panel_closed"), "capture closed");
    let open_capture = try_or_skip!(open.capture("helper_panel_open"), "capture open");

    let panel_distance =
        region_hash_distance(&closed_capture, &open_capture, control_panel_region());
    let help_distance = region_hash_distance(&closed_capture, &open_capture, control_help_region());

    println!(
        "helper panel distances: panel={}, help={}",
        panel_distance, help_distance
    );

    assert!(
        panel_distance > 0,
        "Open helper panel should alter the panel shell region"
    );
    assert!(
        help_distance > 0,
        "Open helper panel should alter the help card region"
    );
}

/// Test extreme transparency - should show almost all background
#[test]
#[serial]
fn test_opacity_extreme_transparent() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    // Set hot pink background
    let _ = std::process::Command::new("swaymsg")
        .args(["output", "*", "background", "#FF1493", "solid_color"])
        .status();

    // 0.1% opacity = nearly invisible overlay
    let harness = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo", "--opacity", "0.001"], "opac_001"),
        "spawn 0.001"
    );
    harness.wait_demo_milestone(3.0);
    let capture = try_or_skip!(harness.capture("opac_001"), "capture 0.001");

    let image = try_or_skip!(
        image::open(&capture).map(|img| img.to_rgba8()),
        "load image 0.001"
    );

    let region = overlay_region();
    let pink = measure_pink_bleedthrough(&image, region);

    println!("Extreme opacity (0.1%):");
    println!("  Pink bleedthrough: {:.4}", pink);

    assert!(
        pink > 0.3,
        "0.1% opacity should show lots of pink (>0.3)\n\
         Got: {:.4}",
        pink
    );

    println!("PASS: Near-transparent shows ~{:.0}% pink", pink * 100.0);
}

/// Test extreme opacity - should show almost no background
#[test]
#[serial]
fn test_opacity_extreme_opaque() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    // Set hot pink background
    let _ = std::process::Command::new("swaymsg")
        .args(["output", "*", "background", "#FF1493", "solid_color"])
        .status();

    // 99.9% opacity = nearly solid overlay
    let harness = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo", "--opacity", "0.999"], "opac_999"),
        "spawn 0.999"
    );
    harness.wait_demo_milestone(3.0);
    let capture = try_or_skip!(harness.capture("opac_999"), "capture 0.999");

    let image = try_or_skip!(
        image::open(&capture).map(|img| img.to_rgba8()),
        "load image 0.999"
    );

    let region = overlay_region();
    let pink = measure_pink_bleedthrough(&image, region);

    println!("Extreme opacity (99.9%):");
    println!("  Pink bleedthrough: {:.4}", pink);

    assert!(
        pink < 0.15,
        "99.9% opacity should show minimal pink (<0.15)\n\
         Got: {:.4}",
        pink
    );

    println!("PASS: Near-opaque shows ~{:.0}% pink", pink * 100.0);
}

#[test]
#[serial]
fn test_demo_partial_listening() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    let harness = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"], "demo_partial_listening"),
        "spawn"
    );

    harness.wait_demo_milestone(3.0);

    let capture = try_or_skip!(harness.capture("partial_listening"), "capture");

    let result = try_or_skip!(
        harness.compare_golden(&capture, "demo_partial_listening.png"),
        "golden comparison"
    );

    if !result.passed {
        if is_canonical() {
            panic!(
                "CANONICAL: screenshot differs: distance={}",
                result.distance
            );
        } else {
            eprintln!(
                "Skipping: hash mismatch (non-canonical): distance={}",
                result.distance
            );
            return;
        }
    }

    println!("PASS: distance={}", result.distance);
}

#[test]
#[serial]
fn test_demo_committed_hello() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    let harness = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"], "demo_committed_hello"),
        "spawn"
    );

    harness.wait_demo_milestone(5.5);

    let capture = try_or_skip!(harness.capture("committed_hello"), "capture");

    let result = try_or_skip!(
        harness.compare_golden(&capture, "demo_committed_hello.png"),
        "golden comparison"
    );

    if !result.passed {
        if is_canonical() {
            panic!(
                "CANONICAL: screenshot differs: distance={}",
                result.distance
            );
        } else {
            eprintln!(
                "Skipping: hash mismatch (non-canonical): distance={}",
                result.distance
            );
            return;
        }
    }

    println!("PASS: distance={}", result.distance);
}

#[test]
#[serial]
fn test_demo_twotone_streaming() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    let harness = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"], "demo_twotone_streaming"),
        "spawn"
    );

    harness.wait_demo_milestone(7.5);

    let capture = try_or_skip!(harness.capture("twotone_streaming"), "capture");

    let result = try_or_skip!(
        harness.compare_golden(&capture, "demo_twotone_streaming.png"),
        "golden comparison"
    );

    if !result.passed {
        if is_canonical() {
            panic!(
                "CANONICAL: screenshot differs: distance={}",
                result.distance
            );
        } else {
            eprintln!(
                "Skipping: hash mismatch (non-canonical): distance={}",
                result.distance
            );
            return;
        }
    }

    println!("PASS: distance={}", result.distance);
}

#[test]
#[serial]
fn test_wgpu_opacity_half() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    // Spawn with --opacity 0.5
    let harness = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(
            &["--demo", "--opacity", "0.5"],
            "wgpu_opacity_half"
        ),
        "spawn"
    );

    harness.wait_demo_milestone(3.0); // Wait for stable render

    let capture = try_or_skip!(harness.capture("opacity_half"), "capture");

    let result = try_or_skip!(
        harness.compare_golden(&capture, "wgpu_opacity_half.png"),
        "golden comparison"
    );

    if !result.passed {
        if is_canonical() {
            panic!(
                "CANONICAL: screenshot differs: distance={}",
                result.distance
            );
        } else {
            eprintln!(
                "Skipping: hash mismatch (non-canonical): distance={}",
                result.distance
            );
            return;
        }
    }

    println!("PASS: opacity test distance={}", result.distance);
}

#[test]
#[serial]
fn test_wgpu_control_panel_full() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    let harness = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(
            &["--demo", "--demo-open-panel"],
            "wgpu_control_panel_full"
        ),
        "spawn"
    );

    harness.wait_demo_milestone(3.0); // Wait for control panel render

    let capture = try_or_skip!(harness.capture("control_panel_full"), "capture");

    let result = try_or_skip!(
        harness.compare_golden(&capture, "wgpu_control_panel_full.png"),
        "golden comparison"
    );

    if !result.passed {
        if is_canonical() {
            panic!(
                "CANONICAL: screenshot differs: distance={}",
                result.distance
            );
        } else {
            eprintln!(
                "Skipping: hash mismatch (non-canonical): distance={}",
                result.distance
            );
            return;
        }
    }

    println!("PASS: control panel test distance={}", result.distance);
}
