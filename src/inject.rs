//! Minimal text injection surface for the rebuilt app.

use anyhow::Result;

/// Trait for text injection backends.
pub trait TextInjector {
    fn inject(&mut self, text: &str) -> Result<()>;
}

#[path = "input/fcitx5_bridge.rs"]
mod fcitx5_bridge;

pub use fcitx5_bridge::Fcitx5BridgeInjector;
