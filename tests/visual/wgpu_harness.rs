//! WGPU test harness for visual regression testing
//!
//! Spawns Barbara with isolated config, captures screenshots at demo milestones,
//! and compares against golden images.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use tempfile::TempDir;

use crate::visual::comparison::{compare_images, CompareResult};
use crate::visual::screenshot::capture_screenshot;

pub struct WgpuTestHarness {
    child: Child,
    temp_dir: TempDir,
}

impl WgpuTestHarness {
    /// Spawn Barbara with given args, isolated from user config
    pub fn spawn(args: &[&str]) -> Result<Self> {
        let temp_dir = tempfile::TempDir::new()?;

        // Isolate from user config
        let config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&config_dir)?;

        let child = Command::new(env!("CARGO_BIN_EXE_barbara"))
            .args(args)
            .env("XDG_CONFIG_HOME", &config_dir)
            .spawn()
            .context("failed to spawn barbara")?;

        Ok(Self { child, temp_dir })
    }

    /// Wait for demo milestone (sleep exactly `seconds` - margin already included)
    pub fn wait_demo_milestone(&self, seconds: f32) {
        thread::sleep(Duration::from_secs_f32(seconds));
    }

    /// Capture screenshot to temp directory
    pub fn capture(&self, name: &str) -> Result<PathBuf> {
        let output = self.temp_dir.path().join(format!("{}.png", name));
        capture_screenshot(&output)?;
        Ok(output)
    }

    /// Compare captured screenshot against golden
    pub fn compare_golden(&self, capture: &Path, golden_name: &str) -> Result<CompareResult> {
        let golden = golden_dir().join(golden_name);
        compare_images(capture, &golden)
    }

    /// Kill Barbara process
    pub fn shutdown(&mut self) -> Result<()> {
        self.child.kill().context("failed to kill barbara")?;
        Ok(())
    }
}

impl Drop for WgpuTestHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Get path to golden images directory
pub fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("visual")
        .join("golden")
}

/// Get path to fixtures directory
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("visual")
        .join("fixtures")
}
