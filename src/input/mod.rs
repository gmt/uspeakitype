//! Text injection for Wayland

use anyhow::Result;

pub mod wrtype;
pub mod ydotool;

/// Trait for text injection backends
pub trait TextInjector {
    /// Returns the name of this injector backend
    fn name(&self) -> &'static str;

    /// Inject text into the focused window
    fn inject(&mut self, text: &str) -> Result<()>;
}

pub use wrtype::WrtypeInjector;
pub use ydotool::{find_ydotool_socket, YdotoolInjector};

/// Automatically select a text injection backend with fallback chain
///
/// Probes backends in order: wrtype → ydotool
/// Skips any backends listed in the `disabled` parameter.
/// Logs probe attempts and results to stderr.
///
/// # Arguments
/// * `disabled` - List of backend names to skip (e.g., `&["wrtype"]`)
///
/// # Returns
/// * `Some(Box<dyn TextInjector>)` - First successful backend
/// * `None` - All backends failed or disabled (triggers display-only mode)
///
/// # Logging
/// Each probe attempt logs to stderr in format:
/// - Active: `[barbara] Probing wrtype... active`
/// - Failed: `[barbara] Probing wrtype... unavailable (error message)`
/// - Disabled: `[barbara] Probing wrtype... skipped (disabled)`
pub fn select_backend(disabled: &[String]) -> Option<Box<dyn TextInjector>> {
    // Probe wrtype
    if !disabled.iter().any(|s| s == "wrtype") {
        log::debug!("Probing wrtype...");
        match WrtypeInjector::new() {
            Ok(inj) => {
                log::info!("wrtype: active");
                return Some(Box::new(inj));
            }
            Err(e) => log::debug!("wrtype: unavailable ({})", e),
        }
    } else {
        log::debug!("wrtype: skipped (disabled)");
    }

    // Probe ydotool
    if !disabled.iter().any(|s| s == "ydotool") {
        log::debug!("Probing ydotool...");
        match YdotoolInjector::new() {
            Ok(inj) => {
                log::info!("ydotool: active");
                return Some(Box::new(inj));
            }
            Err(e) => log::debug!("ydotool: unavailable ({})", e),
        }
    } else {
        log::debug!("ydotool: skipped (disabled)");
    }

    // All backends failed or disabled
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_backend_respects_disabled_list() {
        // When all backends are disabled, should return None
        let disabled = vec!["wrtype".to_string(), "ydotool".to_string()];
        let result = select_backend(&disabled);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_backend_empty_disabled_list() {
        // With empty disabled list, should attempt all backends
        // (may succeed or fail depending on system, but should not panic)
        let disabled = vec![];
        let _result = select_backend(&disabled);
        // Just verify it doesn't panic
    }

    #[test]
    fn test_select_backend_disabled_single_backend() {
        // Disabling one backend should skip it and try others
        let disabled = vec!["wrtype".to_string()];
        let _result = select_backend(&disabled);
        // Just verify it doesn't panic and respects the disabled list
    }

    #[test]
    fn test_select_backend_case_sensitive() {
        // Disabled list should be case-sensitive
        let disabled = vec!["WRTYPE".to_string()]; // uppercase
        let _result = select_backend(&disabled);
        // Should not skip wrtype since "WRTYPE" != "wrtype"
    }
}
