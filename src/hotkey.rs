//! Global hold-to-talk shortcut host shared by every display mode.
//!
//! The Qt helper owns KDE shortcut registration. Rust owns capture admission and
//! finalization, and closes the held gate whenever the helper becomes unavailable.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::ui::SharedAudioState;

#[derive(Deserialize)]
struct HotkeyEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    available: bool,
    error: Option<String>,
}

fn handle_event(state: &SharedAudioState, event: HotkeyEvent) {
    let mut state = state.write();
    match event.kind.as_str() {
        "ptt_press" => {
            if state.push_to_talk && state.ptt_error.is_none() {
                state.set_ptt_held(true);
            }
        }
        "ptt_release" => state.set_ptt_held(false),
        "hotkey_status" => {
            if !event.available {
                state.set_ptt_held(false);
            }
            state.ptt_error = event.error.filter(|message| !message.is_empty());
        }
        _ => {}
    }
}

/// Own the shortcut process and its pipes for the full dictation session.
pub struct HotkeyService {
    child: Mutex<Child>,
    stopping: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    state: SharedAudioState,
}

/// Start the desktop shortcut listener independently of the selected UI.
pub fn start(state: SharedAudioState, running: Arc<AtomicBool>) -> Result<HotkeyService> {
    let executable = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/qt-quick/usit-qt-overlay");
    start_with_executable(&executable, state, running)
}

fn start_with_executable(
    executable: &Path,
    state: SharedAudioState,
    running: Arc<AtomicBool>,
) -> Result<HotkeyService> {
    let mut child = Command::new(executable)
        .arg("--hotkey-only")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("starting push-to-talk helper; build scripts/build-qt-overlay.sh")?;
    let mut stdin = child
        .stdin
        .take()
        .context("shortcut helper stdin missing")?;
    let stdout = child
        .stdout
        .take()
        .context("shortcut helper stdout missing")?;
    let stopping = Arc::new(AtomicBool::new(false));
    let reader_state = state.clone();
    let reader_stopping = stopping.clone();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if reader_stopping.load(Ordering::Relaxed) {
                break;
            }
            let Ok(line) = line else { break };
            match serde_json::from_str(&line) {
                Ok(event) => handle_event(&reader_state, event),
                Err(error) => log::warn!("Invalid shortcut helper event: {error}"),
            }
        }
        let mut state = reader_state.write();
        state.set_ptt_held(false);
        if !reader_stopping.load(Ordering::Relaxed) {
            state.ptt_error =
                Some("Global shortcut helper disconnected; restart usit to reconnect".into());
        }
    });
    let writer_state = state.clone();
    let writer_stopping = stopping.clone();
    let writer = thread::spawn(move || {
        let mut previous = None;
        while running.load(Ordering::Relaxed) && !writer_stopping.load(Ordering::Relaxed) {
            let settings = {
                let state = writer_state.read();
                (state.push_to_talk, state.ptt_shortcut.clone())
            };
            if previous.as_ref() != Some(&settings) {
                let snapshot = serde_json::json!({
                    "push_to_talk": settings.0,
                    "ptt_shortcut": settings.1,
                });
                if writeln!(stdin, "{snapshot}")
                    .and_then(|_| stdin.flush())
                    .is_err()
                {
                    break;
                }
                previous = Some(settings);
            }
            thread::sleep(Duration::from_millis(25));
        }
    });
    Ok(HotkeyService {
        child: Mutex::new(child),
        stopping,
        reader: Some(reader),
        writer: Some(writer),
        state,
    })
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        self.state.write().set_ptt_held(false);
        // The writer closes stdin on stop, allowing normal shortcut unregister.
        // Bound that grace period so a wedged helper cannot hold up shutdown.
        let child = self.child.get_mut().unwrap();
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            if !matches!(child.try_wait(), Ok(None)) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::new_shared_state;

    fn event(kind: &str) -> HotkeyEvent {
        HotkeyEvent {
            kind: kind.into(),
            available: true,
            error: None,
        }
    }

    #[test]
    fn press_requires_ptt_and_available_registration() {
        let state = new_shared_state();
        handle_event(&state, event("ptt_press"));
        assert!(!state.read().ptt_held);
        state.write().set_push_to_talk(true);
        state.write().ptt_error = Some("Shortcut is already taken".into());
        handle_event(&state, event("ptt_press"));
        assert!(!state.read().ptt_held);
        handle_event(&state, event("hotkey_status"));
        handle_event(&state, event("ptt_press"));
        assert!(state.read().ptt_held);
        handle_event(&state, event("ptt_release"));
        assert!(!state.read().ptt_held);
    }

    #[test]
    fn lost_registration_releases_hold_and_reports_failure() {
        let state = new_shared_state();
        state.write().set_push_to_talk(true);
        handle_event(&state, event("ptt_press"));
        handle_event(
            &state,
            HotkeyEvent {
                kind: "hotkey_status".into(),
                available: false,
                error: Some("KDE shortcuts unavailable".into()),
            },
        );
        assert!(!state.read().ptt_held);
        assert_eq!(
            state.read().ptt_error.as_deref(),
            Some("KDE shortcuts unavailable")
        );
    }
    #[test]
    fn helper_eof_releases_hold_and_shutdown_reaps_child() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("helper");
        std::fs::write(
            &script,
            r#"#!/bin/sh
printf '%s\n' '{"type":"hotkey_status","available":true}' '{"type":"ptt_press"}'
while read -r line; do :; done
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let state = new_shared_state();
        state.write().set_push_to_talk(true);
        let running = Arc::new(AtomicBool::new(true));
        let mut service = start_with_executable(&script, state.clone(), running).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !state.read().ptt_held && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(state.read().ptt_held);
        service.child.get_mut().unwrap().kill().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while state.read().ptt_held && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!state.read().ptt_held);
        assert!(state
            .read()
            .ptt_error
            .as_ref()
            .unwrap()
            .contains("disconnected"));
        let began = Instant::now();
        drop(service);
        assert!(began.elapsed() < Duration::from_secs(1));
    }
}
