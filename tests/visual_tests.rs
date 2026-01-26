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
    let harness = visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"]).unwrap();
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
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"]),
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
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"]),
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
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"]),
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
fn test_wgpu_transparency_half() {
    if !visual::screenshot::screenshot_available() {
        if is_canonical() {
            panic!("CANONICAL: {}", visual::screenshot::skip_reason());
        } else {
            eprintln!("Skipping: {}", visual::screenshot::skip_reason());
            return;
        }
    }

    // Spawn with --transparency 0.5
    let harness = try_or_skip!(
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo", "--transparency", "0.5"]),
        "spawn"
    );

    harness.wait_demo_milestone(3.0); // Wait for stable render

    let capture = try_or_skip!(harness.capture("transparency_half"), "capture");

    let result = try_or_skip!(
        harness.compare_golden(&capture, "wgpu_transparency_half.png"),
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

    println!("PASS: transparency test distance={}", result.distance);
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
        visual::wgpu_harness::WgpuTestHarness::spawn(&["--demo"]),
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
