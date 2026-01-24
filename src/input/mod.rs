//! Text injection for Wayland

use anyhow::Result;

pub mod wrtype;

/// Trait for text injection backends
pub trait TextInjector {
    /// Returns the name of this injector backend
    fn name(&self) -> &'static str;

    /// Inject text into the focused window
    fn inject(&mut self, text: &str) -> Result<()>;
}

pub use wrtype::WrtypeInjector;
