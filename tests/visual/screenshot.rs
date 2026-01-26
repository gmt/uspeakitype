//! Screenshot capture via compositor tools
//!
//! Detects compositor type and captures screenshots using grim (wlroots).

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compositor {
    Wlroots,
    Unknown,
    NotWayland,
}

fn grim_available() -> bool {
    Command::new("grim")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn compositor_type() -> Compositor {
    if std::env::var("WAYLAND_DISPLAY").is_err() {
        return Compositor::NotWayland;
    }

    let is_wlroots = std::env::var("SWAYSOCK").is_ok()
        || std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
        || std::env::var("RIVER_SOCKET").is_ok();

    if is_wlroots && grim_available() {
        return Compositor::Wlroots;
    }

    Compositor::Unknown
}

pub fn screenshot_available() -> bool {
    matches!(compositor_type(), Compositor::Wlroots)
}

pub fn skip_reason() -> String {
    match compositor_type() {
        Compositor::Wlroots => {
            if !grim_available() {
                "grim not available (install grim package)".to_string()
            } else {
                "screenshot should be available".to_string()
            }
        }
        Compositor::Unknown => {
            "not running under a verified wlroots compositor (Sway, Hyprland, River)".to_string()
        }
        Compositor::NotWayland => "not running under Wayland (WAYLAND_DISPLAY not set)".to_string(),
    }
}

pub fn capture_screenshot(output: &Path) -> Result<()> {
    let status = Command::new("grim")
        .arg(output)
        .status()
        .context("failed to execute grim")?;

    if !status.success() {
        anyhow::bail!("grim exited with status: {}", status);
    }

    Ok(())
}
