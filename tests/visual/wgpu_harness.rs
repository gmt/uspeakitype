//! WGPU test harness for visual regression testing
//!
//! Spawns Barbara with isolated config, captures screenshots at demo milestones,
//! and compares against golden images.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Once;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use tempfile::TempDir;

use crate::visual::comparison::{compare_images, CompareResult};
use crate::visual::screenshot::capture_screenshot;
use usit::instance::find_instances;

static CLEANUP_ONCE: Once = Once::new();

pub struct WgpuTestHarness {
    child: Child,
    temp_dir: TempDir,
}

impl WgpuTestHarness {
    /// Spawn Barbara with given args, isolated from user config
    ///
    /// # Arguments
    /// * `args` - Command-line arguments (must not include `--tag`)
    /// * `test_tag` - Unique tag for this test instance (e.g., "harness_spawn_capture")
    ///
    /// # Behavior
    /// - Validates that `args` does not contain `--tag` (caller must use `test_tag` parameter)
    /// - Runs cleanup once per process to kill stale test instances
    /// - Adds `--tag visual-test-{test_tag}` and `--no-duplicate-tag` to args
    pub fn spawn(args: &[&str], test_tag: &str) -> Result<Self> {
        // Validate no --tag in args
        for arg in args {
            if arg.starts_with("--tag") {
                panic!(
                    "WgpuTestHarness::spawn: caller must not pass --tag; use test_tag parameter"
                );
            }
        }

        // Run cleanup once per process
        CLEANUP_ONCE.call_once(|| {
            Self::cleanup_all_test_instances();
        });

        let temp_dir = tempfile::TempDir::new()?;

        // Isolate from user config
        let config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&config_dir)?;

        let tag = format!("visual-test-{}", test_tag);
        let mut cmd_args = vec!["--tag", &tag, "--no-duplicate-tag"];
        cmd_args.extend(args.iter());

        let child = Command::new(env!("CARGO_BIN_EXE_usit"))
            .args(&cmd_args)
            .env("XDG_CONFIG_HOME", &config_dir)
            .spawn()
            .context("failed to spawn usit")?;

        Ok(Self { child, temp_dir })
    }

    /// Clean up all test instances with tags starting with "visual-test-"
    ///
    /// Kills processes and waits for /proc/{pid} to disappear (anti-flake).
    fn cleanup_all_test_instances() {
        let instances = find_instances(None);
        let mut killed_pids = Vec::new();

        for inst in instances {
            if let Some(tag) = &inst.tag {
                if tag.starts_with("visual-test-") {
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &inst.pid.to_string()])
                        .status();
                    killed_pids.push(inst.pid);
                }
            }
        }

        // Wait for /proc/{pid} to disappear (anti-flake)
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        for pid in killed_pids {
            let proc_path = format!("/proc/{}", pid);
            while Path::new(&proc_path).exists() {
                if std::time::Instant::now() > deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
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
        // NOTE: Child::kill() sends SIGKILL on Unix (immediate termination)
        // This is fine for test cleanup - we don't need graceful SIGTERM
        let _ = self.child.kill();

        // Poll with timeout to reap zombie - 2 second deadline
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(_status)) => {
                    // Child exited, zombie reaped
                    break;
                }
                Ok(None) => {
                    // Process still showing as running (shouldn't happen after SIGKILL)
                    if std::time::Instant::now() > deadline {
                        // Timeout: give up, final wait attempt
                        let _ = self.child.wait();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => {
                    // Error checking status - process likely already gone
                    break;
                }
            }
        }
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
