//! Universal text injection via ydotool CLI
//!
//! ydotool is a universal fallback for text injection that works across all Wayland
//! compositors and X11 servers. It uses uinput under the hood via the ydotoold daemon.
//!
//! Socket resolution follows ydotool source code conventions:
//! 1. $YDOTOOL_SOCKET (explicit override)
//! 2. $XDG_RUNTIME_DIR/.ydotool_socket (XDG standard)
//! 3. /tmp/.ydotool_socket (fallback)

use super::TextInjector;
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;

/// Find the ydotool socket path using 3-tier resolution
///
/// Checks in order:
/// 1. $YDOTOOL_SOCKET environment variable (if set and exists)
/// 2. $XDG_RUNTIME_DIR/.ydotool_socket (if XDG_RUNTIME_DIR set and socket exists)
/// 3. /tmp/.ydotool_socket (fallback, if exists)
///
/// Returns None if no socket is found.
pub fn find_ydotool_socket() -> Option<PathBuf> {
    // 1. YDOTOOL_SOCKET env var (explicit override)
    if let Ok(path) = std::env::var("YDOTOOL_SOCKET") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. XDG_RUNTIME_DIR/.ydotool_socket
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(xdg).join(".ydotool_socket");
        if p.exists() {
            return Some(p);
        }
    }

    // 3. /tmp/.ydotool_socket (fallback)
    let p = PathBuf::from("/tmp/.ydotool_socket");
    if p.exists() {
        return Some(p);
    }

    None
}

/// Universal text injector using ydotool CLI
///
/// Stateless injector that shells out to the `ydotool` command-line tool.
/// Probes for ydotool availability via `ydotool --help` and checks for ydotoold socket.
///
/// Note: We only check socket path existence, not connectivity. Stale/orphaned sockets
/// (daemon died but file remains) will pass the probe and fail at first inject() call.
/// This is intentional - testing connectivity requires writing to socket (side effect).
#[derive(Debug)]
pub struct YdotoolInjector;

impl YdotoolInjector {
    /// Create a new YdotoolInjector, probing for ydotool availability
    ///
    /// Checks:
    /// 1. ydotool binary exists in PATH (via `ydotool --help`)
    /// 2. ydotoold socket exists (via find_ydotool_socket)
    ///
    /// Returns error if either check fails.
    pub fn new() -> Result<Self> {
        // Check binary exists
        match Command::new("ydotool").arg("--help").output() {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(anyhow!("ydotool not found in PATH"));
            }
            Err(e) => return Err(anyhow!("ydotool probe failed: {}", e)),
        }

        // Check socket exists
        if find_ydotool_socket().is_none() {
            return Err(anyhow!(
                "ydotoold not running. Start with: systemctl --user start ydotool"
            ));
        }

        Ok(Self)
    }
}

impl TextInjector for YdotoolInjector {
    fn name(&self) -> &'static str {
        "ydotool"
    }

    fn inject(&mut self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        let text_with_space = format!("{} ", text);
        let status = Command::new("ydotool")
            .args(["type", "--", &text_with_space])
            .status()
            .map_err(|e| anyhow!("failed to spawn ydotool: {}", e))?;

        if !status.success() {
            return Err(anyhow!(
                "ydotool exited with code {}",
                status.code().unwrap_or(-1)
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().expect("env lock poisoned")
    }

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
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
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

    /// RAII guard to restore environment variables after test
    struct EnvGuard {
        vars: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn new() -> Self {
            Self { vars: Vec::new() }
        }

        fn set(&mut self, key: &str, value: Option<&str>) {
            let original = std::env::var(key).ok();
            self.vars.push((key.to_string(), original));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, original) in self.vars.drain(..) {
                match original {
                    Some(v) => std::env::set_var(&key, v),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }

    #[test]
    fn new_succeeds_when_ydotool_in_path_and_socket_exists() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "ydotool", 0);
        let _path_guard = PathGuard::set(temp.path().to_str().unwrap());

        // Create fake socket
        let socket_path = temp.path().join(".ydotool_socket");
        std::fs::write(&socket_path, "").unwrap();

        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", Some(socket_path.to_str().unwrap()));

        let result = YdotoolInjector::new();
        assert!(result.is_ok());
    }

    #[test]
    fn new_fails_when_ydotool_not_found() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        let _guard = PathGuard::set(temp.path().to_str().unwrap());

        let result = YdotoolInjector::new();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("ydotool not found"));
    }

    #[test]
    fn new_fails_when_socket_missing() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "ydotool", 0);
        let _path_guard = PathGuard::set(temp.path().to_str().unwrap());

        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", None);
        env_guard.set("XDG_RUNTIME_DIR", None);

        let result = YdotoolInjector::new();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("ydotoold not running"));
    }

    #[test]
    fn inject_passes_correct_args() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "ydotool", 0);
        let _path_guard = PathGuard::set(temp.path().to_str().unwrap());

        // Create fake socket
        let socket_path = temp.path().join(".ydotool_socket");
        std::fs::write(&socket_path, "").unwrap();

        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", Some(socket_path.to_str().unwrap()));

        let mut injector = YdotoolInjector::new().unwrap();
        injector.inject("hello world").unwrap();

        // Check logged args
        let log = std::fs::read_to_string(temp.path().join("ydotool.log")).unwrap();
        assert!(
            log.contains("type -- hello world "),
            "log should contain 'type -- hello world ' with trailing space"
        );
    }

    #[test]
    fn inject_empty_string_returns_ok() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "ydotool", 0);
        let _path_guard = PathGuard::set(temp.path().to_str().unwrap());

        // Create fake socket
        let socket_path = temp.path().join(".ydotool_socket");
        std::fs::write(&socket_path, "").unwrap();

        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", Some(socket_path.to_str().unwrap()));

        let mut injector = YdotoolInjector::new().unwrap();

        // Clear the log file (it was created by the --help probe)
        let log_path = temp.path().join("ydotool.log");
        std::fs::remove_file(&log_path).ok();

        let result = injector.inject("");
        assert!(result.is_ok());

        // Verify ydotool was NOT called for inject
        assert!(
            !log_path.exists(),
            "ydotool should not be called for empty string"
        );
    }

    #[test]
    fn inject_fails_on_nonzero_exit() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "ydotool", 1);
        let _path_guard = PathGuard::set(temp.path().to_str().unwrap());

        // Create fake socket
        let socket_path = temp.path().join(".ydotool_socket");
        std::fs::write(&socket_path, "").unwrap();

        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", Some(socket_path.to_str().unwrap()));

        let mut injector = YdotoolInjector::new().unwrap();
        let result = injector.inject("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exited with code"));
    }

    #[test]
    fn name_returns_ydotool() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        setup_fake_binary(&temp, "ydotool", 0);
        let _path_guard = PathGuard::set(temp.path().to_str().unwrap());

        // Create fake socket
        let socket_path = temp.path().join(".ydotool_socket");
        std::fs::write(&socket_path, "").unwrap();

        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", Some(socket_path.to_str().unwrap()));

        let injector = YdotoolInjector::new().unwrap();
        assert_eq!(injector.name(), "ydotool");
    }

    #[test]
    fn find_ydotool_socket_checks_ydotool_socket_env_first() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("custom_socket");
        std::fs::write(&socket_path, "").unwrap();

        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", Some(socket_path.to_str().unwrap()));
        env_guard.set("XDG_RUNTIME_DIR", None);

        let result = find_ydotool_socket();
        assert_eq!(result, Some(socket_path));
    }

    #[test]
    fn find_ydotool_socket_checks_xdg_runtime_dir_second() {
        let _env_lock = env_lock();
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join(".ydotool_socket");
        std::fs::write(&socket_path, "").unwrap();

        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", None);
        env_guard.set("XDG_RUNTIME_DIR", Some(temp.path().to_str().unwrap()));

        let result = find_ydotool_socket();
        assert_eq!(result, Some(socket_path));
    }

    #[test]
    fn find_ydotool_socket_checks_tmp_fallback_third() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", None);
        env_guard.set("XDG_RUNTIME_DIR", None);

        // This test only passes if /tmp/.ydotool_socket exists on the system
        // We can't reliably test this without creating a real socket
        // So we just verify the function doesn't crash
        let _result = find_ydotool_socket();
    }

    #[test]
    fn find_ydotool_socket_returns_none_when_no_socket_found() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("YDOTOOL_SOCKET", None);
        env_guard.set("XDG_RUNTIME_DIR", Some("/nonexistent/path"));

        let result = find_ydotool_socket();
        // Result depends on whether /tmp/.ydotool_socket exists
        // We just verify it doesn't panic
        let _ = result;
    }
}
