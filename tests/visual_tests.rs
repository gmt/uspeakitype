mod visual;

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("visual")
        .join("fixtures")
}

#[test]
#[ignore]
fn test_compositor_detection() {
    println!("Compositor: {:?}", visual::screenshot::compositor_type());
}

#[test]
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
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
    std::env::var("BARBARA_CANONICAL_TEST_ENV").is_ok()
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

/// Measure "pinkness" of an image region
/// Hot pink is RGB(255, 20, 147) - high R, low G, high B
/// Returns average pink score (0.0 = no pink, 1.0 = pure hot pink)
fn measure_pink_bleedthrough(image: &image::RgbaImage, region: (u32, u32, u32, u32)) -> f32 {
    let (x, y, w, h) = region;
    let mut pink_score = 0.0;
    let mut count = 0;

    for py in y..(y + h) {
        for px in x..(x + w) {
            let pixel = image.get_pixel(px, py);
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;

            let score = ((r - g) + (b - g)).max(0.0) / 2.0;
            pink_score += score;
            count += 1;
        }
    }

    pink_score / count as f32
}

/// Test that opacity CLI flag actually affects rendering
/// and verify directional semantics (higher = more opaque)
#[test]
#[ignore] // Visual test - requires compositor
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

    // Load images for pink measurement
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

    // Measure pink in Barbara's window region
    // (adjust region based on actual window position)
    let region = (100, 800, 1720, 200); // x, y, w, h - bottom overlay area

    let pink_75 = measure_pink_bleedthrough(&image_75, region);
    let pink_85 = measure_pink_bleedthrough(&image_85, region);
    let pink_95 = measure_pink_bleedthrough(&image_95, region);

    println!("Pink bleedthrough measurements:");
    println!("  75% opacity: {:.4}", pink_75);
    println!("  85% opacity: {:.4}", pink_85);
    println!("  95% opacity: {:.4}", pink_95);

    // Directional assertions:
    // Lower opacity value = more transparent = MORE pink visible
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

    // Verify they're actually different (not all same due to bug)
    let diff_75_95 = (pink_75 - pink_95).abs();
    assert!(
        diff_75_95 > 0.05,
        "75% and 95% should be measurably different\n\
         Got difference: {:.4} (expected > 0.05)",
        diff_75_95
    );

    println!("PASS: Opacity directional test");
    println!("  Semantics confirmed: higher value = more opaque = less pink");
}

/// Test extreme transparency - should show almost all background
#[test]
#[ignore] // Visual test - requires compositor
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

    let region = (100, 800, 1720, 200);
    let pink = measure_pink_bleedthrough(&image, region);

    println!("Extreme opacity (0.1%):");
    println!("  Pink bleedthrough: {:.4}", pink);

    // Should be very high - almost pure pink showing through
    // Threshold: at least 70% pink visible
    assert!(
        pink > 0.70,
        "0.1% opacity should show mostly background (>70% pink)\n\
         Got: {:.4}",
        pink
    );

    println!(
        "PASS: Near-transparent shows ~{:.0}% background",
        pink * 100.0
    );
}

/// Test extreme opacity - should show almost no background
#[test]
#[ignore] // Visual test - requires compositor
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

    let region = (100, 800, 1720, 200);
    let pink = measure_pink_bleedthrough(&image, region);

    println!("Extreme opacity (99.9%):");
    println!("  Pink bleedthrough: {:.4}", pink);

    // Should be very low - almost no pink showing through
    // Threshold: less than 10% pink visible
    assert!(
        pink < 0.10,
        "99.9% opacity should show almost no background (<10% pink)\n\
         Got: {:.4}",
        pink
    );

    println!("PASS: Near-opaque shows ~{:.0}% background", pink * 100.0);
}

#[test]
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
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
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"], "wgpu_control_panel_full"),
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
