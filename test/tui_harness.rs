//! TUI test harness using portable-pty and vt100
//!
//! Provides infrastructure for testing usit's ANSI terminal UI
//! at various terminal sizes.

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::time::{Duration, Instant};
use vt100::Parser;

/// Test harness for TUI integration tests
pub struct TuiTestHarness {
    master_writer: Box<dyn Write + Send>,
    master_reader: Box<dyn Read + Send>,
    slave: Box<dyn portable_pty::SlavePty>,
    child: Option<Box<dyn portable_pty::Child>>,
    parser: Parser,
}

impl TuiTestHarness {
    /// Create new test harness with specified terminal size
    pub fn new(cols: u16, rows: u16) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();

        let pty_pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let parser = Parser::new(rows, cols, 0);
        let master_reader = pty_pair.master.try_clone_reader()?;
        let master_writer = pty_pair.master.take_writer()?;

        Ok(Self {
            master_writer,
            master_reader,
            slave: pty_pair.slave,
            child: None,
            parser,
        })
    }

    /// Spawn usit binary with given arguments
    pub fn spawn(&mut self, args: &[&str]) -> anyhow::Result<()> {
        let exe_path = env!("CARGO_BIN_EXE_usit");
        let mut cmd = CommandBuilder::new(exe_path);

        for arg in args {
            cmd.arg(arg);
        }

        let child = self.slave.spawn_command(cmd)?;
        self.child = Some(child);
        Ok(())
    }

    /// Send keys to the PTY (simulates user input)
    pub fn send_keys(&mut self, text: &str) -> anyhow::Result<()> {
        self.master_writer.write_all(text.as_bytes())?;
        self.master_writer.flush()?;
        Ok(())
    }

    /// Wait for N frames (50ms each), reading PTY output continuously
    pub fn wait_frames(&mut self, n: usize) -> anyhow::Result<()> {
        let total_time = Duration::from_millis(50 * n as u64);
        let deadline = Instant::now() + total_time;

        while Instant::now() < deadline {
            let mut buf = [0u8; 4096];
            match self.master_reader.read(&mut buf) {
                Ok(bytes_read) if bytes_read > 0 => {
                    self.parser.process(&buf[..bytes_read]);
                }
                _ => {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        Ok(())
    }

    /// Get current screen contents as plain text (ANSI codes stripped)
    pub fn screen_contents(&self) -> String {
        self.parser.screen().contents()
    }

    /// Check if screen contains the given text
    pub fn has_text(&self, needle: &str) -> bool {
        self.screen_contents().contains(needle)
    }

    /// Wait for child process to exit with timeout
    pub fn wait_exit(&mut self, timeout_ms: u64) -> anyhow::Result<i32> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No child process"))?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status.exit_code() as i32);
            }

            if Instant::now() > deadline {
                child.kill()?;
                return Err(anyhow::anyhow!("Timeout waiting for exit"));
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Check if PTY is available on this system
pub fn pty_available() -> bool {
    let pty_system = native_pty_system();
    pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .is_ok()
}

impl Drop for TuiTestHarness {
    fn drop(&mut self) {
        // Ensure child process is cleaned up if harness is dropped
        // (e.g., test panic) without explicit wait_exit call.
        // portable_pty's kill() sends SIGHUP first, waits 250ms,
        // then SIGKILL - this gives the child time for graceful cleanup.
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            // Give it a moment to actually exit
            for _ in 0..10 {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_basic() {
        if !pty_available() {
            eprintln!("Skipping: PTY not available");
            return;
        }

        let harness = TuiTestHarness::new(80, 24);
        assert!(harness.is_ok(), "Failed to create harness");
    }

    #[test]
    fn test_pty_available() {
        let available = pty_available();
        eprintln!("PTY available: {}", available);
    }
}
