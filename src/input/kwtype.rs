//! KDE Plasma text injection via kwtype CLI

use super::TextInjector;
use anyhow::{anyhow, Result};
use std::process::Command;

/// KDE Plasma text injector using kwtype CLI
///
/// Stateless injector that shells out to the `kwtype` command-line tool.
/// Probes for kwtype availability via `kwtype --help` (safe, no Wayland connection).
#[derive(Debug)]
pub struct KwtypeInjector;

impl KwtypeInjector {
    /// Create a new KwtypeInjector, probing for kwtype availability
    ///
    /// Probes with `kwtype --help` which is safe and doesn't require Wayland connection.
    /// Authorization prompts appear on first actual use (first `inject()` call), not here.
    pub fn new() -> Result<Self> {
        match Command::new("kwtype").arg("--help").output() {
            Ok(_) => Ok(Self),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(anyhow!("kwtype not found in PATH"))
            }
            Err(e) => Err(anyhow!("kwtype probe failed: {}", e)),
        }
    }
}

impl TextInjector for KwtypeInjector {
    fn name(&self) -> &'static str {
        "kwtype"
    }

    fn inject(&mut self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        let text_with_space = format!("{} ", text);
        let status = Command::new("kwtype")
            .arg(&text_with_space)
            .status()
            .map_err(|e| anyhow!("failed to spawn kwtype: {}", e))?;

        if !status.success() {
            return Err(anyhow!(
                "kwtype exited with code {}",
                status.code().unwrap_or(-1)
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static PATH_LOCK: Mutex<()> = Mutex::new(());

    /// Create fake binary that logs invocation to a file
    fn setup_fake_binary(dir: &TempDir, name: &str, exit_code: i32) -> PathBuf {
        let log_file = dir.path().join(format!("{}.log", name));
        let log_path_str = log_file.to_string_lossy();
        let script = format!(
            "#!/bin/sh\necho \"$@\" >> \"{}\"\nexit {}",
            log_path_str, exit_code
        );
        let path = dir.path().join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        path
    }

    /// RAII guard to restore PATH after test
    struct PathGuard {
        original: Option<String>,
    }

    impl PathGuard {
        fn set(new_path: &str) -> Self {
            let original = std::env::var("PATH").ok();
            std::env::set_var("PATH", new_path);
            Self { original }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    #[test]
    fn new_succeeds_when_kwtype_in_path() {
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "kwtype", 0);
        let _guard = PathGuard::set(temp.path().to_str().unwrap());

        let result = KwtypeInjector::new();
        assert!(result.is_ok());
    }

    #[test]
    fn new_fails_when_kwtype_not_found() {
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        let _guard = PathGuard::set(temp.path().to_str().unwrap());

        let result = KwtypeInjector::new();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("kwtype not found"));
    }

    #[test]
    fn inject_passes_correct_args() {
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "kwtype", 0);
        let _guard = PathGuard::set(temp.path().to_str().unwrap());

        let mut injector = KwtypeInjector::new().expect("kwtype should be found in test PATH");
        injector
            .inject("hello world")
            .expect("inject should succeed");

        // Check logged args
        let log = std::fs::read_to_string(temp.path().join("kwtype.log")).unwrap();
        assert!(
            log.contains("hello world "),
            "log should contain 'hello world ' with trailing space"
        );
    }

    #[test]
    fn inject_empty_string_returns_ok() {
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "kwtype", 0);
        let _guard = PathGuard::set(temp.path().to_str().unwrap());

        let mut injector = KwtypeInjector::new().unwrap();

        // Get log content after new() but before inject()
        let log_path = temp.path().join("kwtype.log");
        let log_before = if log_path.exists() {
            std::fs::read_to_string(&log_path).unwrap()
        } else {
            String::new()
        };

        let result = injector.inject("");
        assert!(result.is_ok());

        // Verify kwtype was NOT called for inject (log should not have changed)
        let log_after = if log_path.exists() {
            std::fs::read_to_string(&log_path).unwrap()
        } else {
            String::new()
        };
        assert_eq!(
            log_before, log_after,
            "kwtype should not be called for empty string"
        );
    }

    #[test]
    fn inject_fails_on_nonzero_exit() {
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "kwtype", 1);
        let _guard = PathGuard::set(temp.path().to_str().unwrap());

        let mut injector = KwtypeInjector::new().expect("kwtype should be found in test PATH");
        let result = injector.inject("test");
        assert!(result.is_err(), "inject should fail with nonzero exit code");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("exited with code"),
            "error should mention exit code, got: {}",
            err_msg
        );
    }

    #[test]
    fn name_returns_kwtype() {
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "kwtype", 0);
        let _guard = PathGuard::set(temp.path().to_str().unwrap());

        let injector = KwtypeInjector::new().unwrap();
        assert_eq!(injector.name(), "kwtype");
    }
}
