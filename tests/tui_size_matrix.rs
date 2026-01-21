//! Size matrix tests for TUI responsive layout
//!
//! Tests the 9-tile grid of terminal sizes to verify LayoutMode behavior.

mod tui_harness;
use tui_harness::{pty_available, TuiTestHarness};

// ============================================================================
// 9-TILE GRID TESTS
// ============================================================================

#[test]
#[ignore]
fn test_size_20x6_degenerate() {
    if !pty_available() {
        eprintln!("Skipping: PTY not available");
        return;
    }

    let mut harness = TuiTestHarness::new(20, 6).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // At degenerate size, should show only status indicator
    // 'c' should be ignored (no panel)
    harness.send_keys("c").unwrap();
    harness.wait_frames(2).unwrap();
    assert!(!harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_size_20x8_degenerate_width() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(20, 8).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // Width < 25, so degenerate even though height >= 8
    harness.send_keys("c").unwrap();
    harness.wait_frames(2).unwrap();
    assert!(!harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_size_20x12_degenerate_width() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(20, 12).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // Width < 25, so degenerate
    harness.send_keys("c").unwrap();
    harness.wait_frames(2).unwrap();
    assert!(!harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_size_30x6_degenerate_height() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(30, 6).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // Height < 8, so degenerate even though width >= 25
    harness.send_keys("c").unwrap();
    harness.wait_frames(2).unwrap();
    assert!(!harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_size_30x8_minimal() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(30, 8).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // Minimal mode: 25 <= width < 35, height >= 8
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();

    // Should show panel with minimal labels (single-char)
    assert!(harness.has_text("...") || harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_size_30x12_minimal() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(30, 12).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // Minimal mode
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("...") || harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_size_40x12_compact() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(40, 12).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // Compact mode: 35 <= width < 50
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_size_80x6_degenerate_height() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 6).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // Height < 8, so degenerate even though width >= 50
    harness.send_keys("c").unwrap();
    harness.wait_frames(2).unwrap();
    assert!(!harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_size_80x24_full() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();

    // Full mode: width >= 50, height >= 10
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("Panel"));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

// ============================================================================
// BOUNDARY TESTS
// ============================================================================

#[test]
#[ignore]
fn test_boundary_width_24_25() {
    if !pty_available() {
        return;
    }

    // 24: degenerate
    let mut harness = TuiTestHarness::new(24, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(2).unwrap();
    assert!(!harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 25: minimal
    let mut harness = TuiTestHarness::new(25, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("...") || harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_boundary_width_34_35() {
    if !pty_available() {
        return;
    }

    // 34: minimal
    let mut harness = TuiTestHarness::new(34, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("...") || harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 35: compact
    let mut harness = TuiTestHarness::new(35, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_boundary_width_49_50() {
    if !pty_available() {
        return;
    }

    // 49: compact
    let mut harness = TuiTestHarness::new(49, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 50: full
    let mut harness = TuiTestHarness::new(50, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_boundary_height_7_8() {
    if !pty_available() {
        return;
    }

    // 7: degenerate
    let mut harness = TuiTestHarness::new(50, 7).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(2).unwrap();
    assert!(!harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 8: minimal (if width < 35) or compact/full (if width >= 35)
    let mut harness = TuiTestHarness::new(50, 8).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
#[ignore]
fn test_boundary_height_9_10() {
    if !pty_available() {
        return;
    }

    // Both should show panel (height >= 8)
    // 9: valid
    let mut harness = TuiTestHarness::new(50, 9).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 10: valid
    let mut harness = TuiTestHarness::new(50, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("Panel"));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}
