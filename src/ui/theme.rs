//! Centralized theme system for UI colors
//!
//! This module provides a unified color theme that can be converted to both
//! WGPU (graphical) and ANSI (terminal) formats. The same Theme struct drives
//! both rendering surfaces, ensuring visual consistency.

use crate::spectrum::Color;

/// UI color theme
///
/// Contains all colors used for UI chrome (background, text, panels).
/// Does NOT include spectrogram colors - those use ColorScheme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Main background color
    pub background: Color,
    /// Shadow/border color
    pub shadow: Color,
    /// Committed (finalized) text color
    pub text_committed: Color,
    /// Partial (in-progress) text color
    pub text_partial: Color,
    /// Error text color (red)
    pub text_error: Color,
    /// Control panel background color
    pub panel_bg: Color,
    /// Elevated card / status strip background color
    pub panel_alt: Color,
    /// Warm accent used for headings and dividers
    pub accent: Color,
}

/// Theme colors converted to WGPU format ([f32; 4] arrays)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeWgpu {
    pub background: [f32; 4],
    pub shadow: [f32; 4],
    pub text_committed: [f32; 4],
    pub text_partial: [f32; 4],
    pub text_error: [f32; 4],
    pub panel_bg: [f32; 4],
    pub panel_alt: [f32; 4],
    pub accent: [f32; 4],
}

/// Theme colors converted to ANSI escape sequences
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeAnsi {
    pub background: String,
    pub shadow: String,
    pub text_committed: String,
    pub text_partial: String,
    pub text_error: String,
    pub panel_bg: String,
    pub panel_alt: String,
    pub accent: String,
}

impl Theme {
    /// Convert theme to WGPU format for graphical rendering
    pub fn to_wgpu(&self) -> ThemeWgpu {
        ThemeWgpu {
            background: self.background.to_array(),
            shadow: self.shadow.to_array(),
            text_committed: self.text_committed.to_array(),
            text_partial: self.text_partial.to_array(),
            text_error: self.text_error.to_array(),
            panel_bg: self.panel_bg.to_array(),
            panel_alt: self.panel_alt.to_array(),
            accent: self.accent.to_array(),
        }
    }

    /// Convert theme to ANSI format for terminal rendering
    pub fn to_ansi(&self) -> ThemeAnsi {
        ThemeAnsi {
            background: self.background.to_ansi_bg(),
            shadow: self.shadow.to_ansi_bg(),
            text_committed: self.text_committed.to_ansi_fg(),
            text_partial: self.text_partial.to_ansi_fg(),
            text_error: self.text_error.to_ansi_fg(),
            panel_bg: self.panel_bg.to_ansi_bg(),
            panel_alt: self.panel_alt.to_ansi_bg(),
            accent: self.accent.to_ansi_fg(),
        }
    }
}

/// Default dark theme
///
/// Dark background with white committed text and gray partial text.
/// Suitable for both terminal and graphical rendering.
pub const DEFAULT_THEME: Theme = Theme {
    background: Color::rgb(0.06, 0.05, 0.05), // near-black umber
    shadow: Color::rgb(0.0, 0.0, 0.0),        // black
    text_committed: Color::rgb(0.96, 0.94, 0.9),
    text_partial: Color::rgb(0.66, 0.61, 0.56),
    text_error: Color::rgb(0.72, 0.28, 0.23), // distant brick red
    panel_bg: Color::rgb(0.11, 0.08, 0.08),   // smoked walnut
    panel_alt: Color::rgb(0.18, 0.13, 0.12),  // warm card
    accent: Color::rgb(0.84, 0.64, 0.34),     // amber brass
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_wgpu_conversion() {
        let theme = DEFAULT_THEME;
        let wgpu = theme.to_wgpu();
        assert_eq!(wgpu.background, [0.06, 0.05, 0.05, 1.0]);
        assert_eq!(wgpu.text_committed, [0.96, 0.94, 0.9, 1.0]);
        assert_eq!(wgpu.text_partial, [0.66, 0.61, 0.56, 1.0]);
        assert_eq!(wgpu.text_error, [0.72, 0.28, 0.23, 1.0]);
        assert_eq!(wgpu.shadow, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(wgpu.panel_bg, [0.11, 0.08, 0.08, 1.0]);
        assert_eq!(wgpu.panel_alt, [0.18, 0.13, 0.12, 1.0]);
        assert_eq!(wgpu.accent, [0.84, 0.64, 0.34, 1.0]);
    }

    #[test]
    fn theme_ansi_conversion() {
        let theme = DEFAULT_THEME;
        let ansi = theme.to_ansi();

        // Committed text should be white foreground
        assert!(ansi.text_committed.contains("\x1b[38;2;244;239;229m"));

        // Partial text should be gray foreground
        assert!(ansi.text_partial.contains("\x1b[38;2;168;155;142m"));

        // Background should be dark gray background
        assert!(ansi.background.contains("\x1b[48;2;15;12;12m"));

        // Shadow should be black background
        assert!(ansi.shadow.contains("\x1b[48;2;0;0;0m"));

        // Panel should be slightly lighter gray background
        assert!(ansi.panel_bg.contains("\x1b[48;2;28;20;20m"));
        assert!(ansi.panel_alt.contains("\x1b[48;2;45;33;30m"));
        assert!(ansi.accent.contains("\x1b[38;2;214;163;86m"));
    }

    #[test]
    fn theme_colors_are_valid() {
        let theme = DEFAULT_THEME;

        // All colors should have alpha = 1.0
        assert_eq!(theme.background.a, 1.0);
        assert_eq!(theme.shadow.a, 1.0);
        assert_eq!(theme.text_committed.a, 1.0);
        assert_eq!(theme.text_partial.a, 1.0);
        assert_eq!(theme.text_error.a, 1.0);
        assert_eq!(theme.panel_bg.a, 1.0);
        assert_eq!(theme.panel_alt.a, 1.0);
        assert_eq!(theme.accent.a, 1.0);

        // All RGB components should be in [0.0, 1.0]
        for color in [
            theme.background,
            theme.shadow,
            theme.text_committed,
            theme.text_partial,
            theme.text_error,
            theme.panel_bg,
            theme.panel_alt,
            theme.accent,
        ] {
            assert!(color.r >= 0.0 && color.r <= 1.0);
            assert!(color.g >= 0.0 && color.g <= 1.0);
            assert!(color.b >= 0.0 && color.b <= 1.0);
        }
    }

    #[test]
    fn theme_partial_darker_than_committed() {
        let theme = DEFAULT_THEME;

        // Partial text should be darker (lower intensity) than committed
        let partial_intensity = theme.text_partial.r + theme.text_partial.g + theme.text_partial.b;
        let committed_intensity =
            theme.text_committed.r + theme.text_committed.g + theme.text_committed.b;

        assert!(partial_intensity < committed_intensity);
    }
}
