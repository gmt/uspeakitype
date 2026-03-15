//! Size matrix tests for TUI responsive layout
//!
//! Tests the 9-tile grid of terminal sizes to verify LayoutMode behavior.

mod tui_harness;
use tui_harness::{pty_available, TuiTestHarness};

fn has_helper_panel_title(harness: &TuiTestHarness) -> bool {
    harness.has_text("Input") || harness.has_text("Panel")
}

fn has_any_panel_title(harness: &TuiTestHarness) -> bool {
    harness.has_text("...") || has_helper_panel_title(harness)
}

// ============================================================================
// 9-TILE GRID TESTS
// ============================================================================

#[test]

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
    assert!(!has_helper_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(!has_helper_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(!has_helper_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(!has_helper_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(has_any_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(has_any_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(has_helper_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(!has_helper_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

fn test_size_80x24_full() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(5).unwrap();

    // Full mode: width >= 50, height >= 10
    harness.send_keys("c").unwrap();
    harness.wait_frames(5).unwrap();
    assert!(has_helper_panel_title(&harness));

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

// ============================================================================
// BOUNDARY TESTS
// ============================================================================

#[test]

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
    assert!(!has_helper_panel_title(&harness));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 25: minimal
    let mut harness = TuiTestHarness::new(25, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(has_any_panel_title(&harness));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(has_any_panel_title(&harness));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 35: compact
    let mut harness = TuiTestHarness::new(35, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(has_helper_panel_title(&harness));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(has_helper_panel_title(&harness));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 50: full
    let mut harness = TuiTestHarness::new(50, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(has_helper_panel_title(&harness));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

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
    assert!(!has_helper_panel_title(&harness));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 8: Minimal mode (height >= 8 but < 10 with width >= 35 falls back to Minimal)
    let mut harness = TuiTestHarness::new(50, 8).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("..."));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]

fn test_boundary_height_9_10() {
    if !pty_available() {
        return;
    }

    // 9: Minimal mode (height < 10 falls back to Minimal even with width >= 50)
    let mut harness = TuiTestHarness::new(50, 9).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("..."));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);

    // 10: Full mode (height >= 10 && width >= 50)
    let mut harness = TuiTestHarness::new(50, 10).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(3).unwrap();
    harness.send_keys("c").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(has_helper_panel_title(&harness));
    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

// ============================================================================
// KEYBOARD SHORTCUT TESTS
// ============================================================================

#[test]
fn test_viz_mode_toggle() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(5).unwrap();

    let initial_screen = harness.screen_contents();

    harness.send_keys("w").unwrap();
    harness.wait_frames(5).unwrap();

    let toggled_screen = harness.screen_contents();
    assert_ne!(
        initial_screen, toggled_screen,
        "Screen should change after 'w' toggle"
    );

    harness.send_keys("w").unwrap();
    harness.wait_frames(5).unwrap();

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
fn test_pause_resume() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(5).unwrap();

    harness.send_keys("c").unwrap();
    harness.wait_frames(5).unwrap();

    assert!(
        harness.has_text("Listening"),
        "Should show listening state initially"
    );

    harness.send_keys(" ").unwrap();
    harness.wait_frames(3).unwrap();

    assert!(
        harness.has_text("Standby"),
        "Should show standby state after spacebar"
    );

    harness.send_keys(" ").unwrap();
    harness.wait_frames(3).unwrap();

    assert!(
        harness.has_text("Listening"),
        "Should return to listening state after second spacebar"
    );

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
fn test_color_scheme_cycling() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness.spawn(&["--ansi", "--demo"]).unwrap();
    harness.wait_frames(5).unwrap();

    harness.send_keys("c").unwrap();
    harness.wait_frames(5).unwrap();

    assert!(
        harness.has_text("flame"),
        "Should show flame scheme initially"
    );

    // Navigate to ColorPicker (6th control)
    for _ in 0..5 {
        harness.send_keys("\x1b[B").unwrap();
        harness.wait_frames(1).unwrap();
    }
    harness.wait_frames(2).unwrap();

    harness.send_keys("\r").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(harness.has_text("ice"), "Should show ice after first Enter");

    harness.send_keys("\r").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(
        harness.has_text("mono"),
        "Should show mono after second Enter"
    );

    harness.send_keys("\r").unwrap();
    harness.wait_frames(3).unwrap();
    assert!(
        harness.has_text("flame"),
        "Should show flame after third Enter"
    );

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
fn test_ascii_charset_mode() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness
        .spawn(&["--ansi", "--demo", "--ansi-charset", "ascii"])
        .unwrap();
    harness.wait_frames(10).unwrap();

    let screen = harness.screen_contents();

    // ASCII charset uses: [' ', '.', ':', '-', '=', '+', '*', '#', '@']
    // Should NOT contain Unicode block characters
    let unicode_blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    for block in unicode_blocks {
        assert!(
            !screen.contains(block),
            "Screen should not contain Unicode block '{}' in ASCII mode",
            block
        );
    }

    // Should contain at least some ASCII gradient chars (the spectrogram renders them)
    let ascii_chars = ['.', ':', '-', '=', '+', '*', '#', '@'];
    let has_ascii = ascii_chars.iter().any(|&c| screen.contains(c));
    assert!(has_ascii, "Screen should contain ASCII gradient characters");

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
fn test_ice_color_scheme() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness
        .spawn(&["--ansi", "--demo", "--color", "ice"])
        .unwrap();
    harness.wait_frames(5).unwrap();

    // Open panel to verify scheme name
    harness.send_keys("c").unwrap();
    harness.wait_frames(5).unwrap();

    assert!(
        harness.has_text("ice"),
        "Should show ice color scheme in panel"
    );

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
fn test_mono_color_scheme() {
    if !pty_available() {
        return;
    }

    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness
        .spawn(&["--ansi", "--demo", "--color", "mono"])
        .unwrap();
    harness.wait_frames(5).unwrap();

    harness.send_keys("c").unwrap();
    harness.wait_frames(5).unwrap();

    assert!(
        harness.has_text("mono"),
        "Should show mono color scheme in panel"
    );

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}

#[test]
fn test_ascii_ice_combination() {
    if !pty_available() {
        return;
    }

    // Test that ASCII charset and Ice color scheme work together
    let mut harness = TuiTestHarness::new(80, 24).unwrap();
    harness
        .spawn(&[
            "--ansi",
            "--demo",
            "--ansi-charset",
            "ascii",
            "--color",
            "ice",
        ])
        .unwrap();
    harness.wait_frames(10).unwrap();

    let screen = harness.screen_contents();

    // Should NOT contain Unicode blocks (ASCII mode)
    let unicode_blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    for block in unicode_blocks {
        assert!(
            !screen.contains(block),
            "Screen should not contain Unicode block '{}' in ASCII mode",
            block
        );
    }

    // Open panel to verify ice scheme
    harness.send_keys("c").unwrap();
    harness.wait_frames(5).unwrap();
    assert!(harness.has_text("ice"), "Should show ice color scheme");

    harness.send_keys("q").unwrap();
    let _ = harness.wait_exit(1000);
}
