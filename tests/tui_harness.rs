//! TUI test harness using portable-pty and vt100
//!
//! Provides infrastructure for testing Barbara's ANSI terminal UI
//! at various terminal sizes.

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::Read;
use std::time::{Duration, Instant};
use vt100::Parser;

/// Test harness for TUI integration tests
pub struct TuiTestHarness {
    master: Box<dyn Read + Send>,
    child: Box<dyn portable_pty::Child>,
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
        let master = pty_pair.master.try_clone_reader()?;

        let cmd = CommandBuilder::new("true");
        let child = pty_pair.slave.spawn_command(cmd)?;

        Ok(Self {
            master,
            child,
            parser,
        })
    }

    /// Spawn barbara binary with given arguments
    pub fn spawn(&mut self, _args: &[&str]) -> anyhow::Result<()> {
        // TODO: Implement spawn by restructuring to keep slave PTY alive
        Ok(())
    }

    /// Send keys to the PTY (simulates user input)
    pub fn send_keys(&mut self, _text: &str) -> anyhow::Result<()> {
        // TODO: Implement send_keys by keeping master writer in struct
        Ok(())
    }

    /// Wait for N frames (50ms each), reading PTY output
    pub fn wait_frames(&mut self, n: usize) -> anyhow::Result<()> {
        for _ in 0..n {
            std::thread::sleep(Duration::from_millis(50));

            let mut buf = [0u8; 4096];
            match self.master.read(&mut buf) {
                Ok(bytes_read) if bytes_read > 0 => {
                    self.parser.process(&buf[..bytes_read]);
                }
                _ => {}
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
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status.exit_code() as i32);
            }

            if Instant::now() > deadline {
                self.child.kill()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
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
