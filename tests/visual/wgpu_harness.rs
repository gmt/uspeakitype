//! WGPU test harness for visual regression testing
//!
//! Spawns usit with isolated config, captures screenshots at demo milestones,
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
    /// Spawn usit with given args, isolated from user config
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
    /// Uses graceful signal progression (SIGTERM -> SIGKILL) to allow
    /// processes to clean up Wayland IME state before termination.
    fn cleanup_all_test_instances() {
        let instances = find_instances(None);
        let mut pids_to_kill = Vec::new();

        for inst in instances {
            if let Some(tag) = &inst.tag {
                if tag.starts_with("visual-test-") {
                    pids_to_kill.push(inst.pid);
                }
            }
        }

        if pids_to_kill.is_empty() {
            return;
        }

        // Send SIGTERM first for graceful shutdown
        for &pid in &pids_to_kill {
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }

        // Wait up to 200ms for graceful exit
        let grace_deadline = std::time::Instant::now() + Duration::from_millis(200);
        while std::time::Instant::now() < grace_deadline {
            pids_to_kill.retain(|&pid| Path::new(&format!("/proc/{}", pid)).exists());
            if pids_to_kill.is_empty() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }

        // SIGKILL any remaining processes
        for &pid in &pids_to_kill {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status();
        }

        // Wait for /proc/{pid} to disappear (anti-flake)
        let deadline = std::time::Instant::now() + Duration::from_millis(300);
        for pid in pids_to_kill {
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

    /// Kill usit process
    pub fn shutdown(&mut self) -> Result<()> {
        self.child.kill().context("failed to kill usit")?;
        Ok(())
    }
}

impl Drop for WgpuTestHarness {
    fn drop(&mut self) {
        // Use graceful signal progression to allow Wayland IME cleanup.
        // SIGTERM first, then SIGKILL if needed.
        let pid = self.child.id();

        // Send SIGTERM for graceful shutdown
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();

        // Wait up to 200ms for graceful exit
        let grace_deadline = std::time::Instant::now() + Duration::from_millis(200);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return, // Exited gracefully
                Ok(None) => {
                    if std::time::Instant::now() > grace_deadline {
                        break; // Grace period expired
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => return, // Error, process likely gone
            }
        }

        // SIGKILL if still running after grace period
        let _ = self.child.kill();

        // Reap zombie with timeout
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if std::time::Instant::now() > deadline {
                        let _ = self.child.wait();
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(_) => break,
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
