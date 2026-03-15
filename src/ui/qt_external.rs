//! External Qt Widgets frontend bridge.
//!
//! Rust remains the source of truth for audio/model/input state. The Qt process
//! owns the graphical shell and exchanges JSON snapshots/commands over stdio.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::app::{OverlayConfigContext, OverlayModelCommand, OverlayRunOptions};
use super::control_panel::ControlPanelState;
use super::spectrogram::SpectrogramMode;
use super::{
    build_runtime_config, helper_mode_label, helper_model_label, helper_source_label,
    helper_status_short_summary, SharedAudioState,
};
use crate::audio::CaptureControl;

const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Debug, Deserialize)]
struct QtCommand {
    #[serde(rename = "type")]
    command_type: String,
    value: Option<f32>,
}

#[derive(Debug, Serialize)]
struct QtSnapshot {
    committed: String,
    partial: String,
    paused: bool,
    injection_enabled: bool,
    auto_gain_enabled: bool,
    auto_save: bool,
    gain: f32,
    viz_mode: &'static str,
    helper_summary: String,
    helper_mode: String,
    source_label: String,
    model_label: String,
    error: Option<String>,
    samples: Vec<f32>,
}

pub fn run(
    audio_state: SharedAudioState,
    running: Arc<AtomicBool>,
    capture_control: Option<Arc<CaptureControl>>,
    options: OverlayRunOptions,
    tag: Option<String>,
) {
    if let Err(error) = run_inner(
        audio_state.clone(),
        running.clone(),
        capture_control.clone(),
        options.clone(),
        tag.clone(),
    ) {
        log::error!("Qt overlay failed, falling back to legacy WGPU shell: {error:#}");
        super::app::run(audio_state, running, capture_control, options, tag);
    }
}

fn run_inner(
    audio_state: SharedAudioState,
    running: Arc<AtomicBool>,
    capture_control: Option<Arc<CaptureControl>>,
    options: OverlayRunOptions,
    tag: Option<String>,
) -> Result<()> {
    let executable = ensure_qt_overlay_built()?;
    let mut child = spawn_qt_overlay(&executable, tag.as_deref())?;
    let mut child_stdin = child.stdin.take().context("qt overlay stdin unavailable")?;
    let command_rx = spawn_command_reader(&mut child)?;

    let mut panel = options.control_panel.clone();
    let mut last_save_at = None;

    while running.load(Ordering::Relaxed) {
        while let Ok(command) = command_rx.try_recv() {
            apply_qt_command(
                &command,
                &audio_state,
                capture_control.as_ref(),
                &mut panel,
                &options,
                &running,
                &mut last_save_at,
            );
        }

        if let Some(status) = child.try_wait().context("checking qt overlay status")? {
            log::info!("Qt overlay exited with {}", status);
            running.store(false, Ordering::Relaxed);
            break;
        }

        sync_panel_from_runtime(&audio_state, capture_control.as_ref(), &mut panel);
        let snapshot = build_snapshot(&audio_state, &panel);
        send_snapshot(&mut child_stdin, &snapshot)?;

        thread::sleep(SNAPSHOT_INTERVAL);
    }

    let _ = child_stdin.flush();
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

fn ensure_qt_overlay_built() -> Result<PathBuf> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_dir = repo_root.join("qt_widgets_overlay");
    let build_dir = source_dir.join("build");
    let executable = build_dir.join("usit-qt-overlay");

    let needs_build = !executable.exists()
        || newest_mtime(&source_dir)?.is_some_and(|source_time| {
            fs::metadata(&executable)
                .and_then(|meta| meta.modified())
                .map(|built| built < source_time)
                .unwrap_or(true)
        });

    if needs_build {
        let script = repo_root.join("scripts/build-qt-overlay.sh");
        let status = Command::new(&script)
            .current_dir(&repo_root)
            .status()
            .with_context(|| format!("building qt overlay via {}", script.display()))?;
        anyhow::ensure!(
            status.success(),
            "qt overlay build failed with status {status}"
        );
    }

    Ok(executable)
}

fn newest_mtime(dir: &Path) -> Result<Option<SystemTime>> {
    let mut newest = None;
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == "build") {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        newest = Some(newest.map_or(modified, |current: SystemTime| current.max(modified)));
    }
    Ok(newest)
}

fn spawn_qt_overlay(executable: &Path, tag: Option<&str>) -> Result<Child> {
    let mut command = Command::new(executable);
    if let Some(tag) = tag {
        command.arg("--tag").arg(tag);
    }

    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawning qt overlay {}", executable.display()))
}

fn spawn_command_reader(child: &mut Child) -> Result<Receiver<QtCommand>> {
    let stdout = child
        .stdout
        .take()
        .context("qt overlay stdout unavailable")?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.trim().is_empty() => {
                    match serde_json::from_str::<QtCommand>(&line) {
                        Ok(command) => {
                            let _ = tx.send(command);
                        }
                        Err(error) => log::warn!("Ignoring malformed Qt command: {error}: {line}"),
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    log::warn!("Qt overlay command stream ended: {error}");
                    break;
                }
            }
        }
    });

    Ok(rx)
}

fn sync_panel_from_runtime(
    audio_state: &SharedAudioState,
    capture_control: Option<&Arc<CaptureControl>>,
    panel: &mut ControlPanelState,
) {
    if let Some(control) = capture_control {
        panel.is_paused = control.is_paused();
        panel.agc_enabled = control.is_auto_gain_enabled();
        panel.gain_value = control.get_current_gain();
    }

    let state = audio_state.read();
    panel.is_paused = state.is_paused;
}

fn build_snapshot(audio_state: &SharedAudioState, panel: &ControlPanelState) -> QtSnapshot {
    let state = audio_state.read();
    QtSnapshot {
        committed: state.committed.clone(),
        partial: state.partial.clone(),
        paused: state.is_paused,
        injection_enabled: state.injection_enabled,
        auto_gain_enabled: state.auto_gain_enabled,
        auto_save: panel.auto_save,
        gain: state.current_gain,
        viz_mode: match panel.viz_mode {
            SpectrogramMode::BarMeter => "bars",
            SpectrogramMode::Waterfall => "waterfall",
        },
        helper_summary: helper_status_short_summary(&state),
        helper_mode: helper_mode_label(&state).to_string(),
        source_label: helper_source_label(&state),
        model_label: helper_model_label(&state),
        error: state.model_error.clone(),
        samples: downsample_samples(&state.samples, 96),
    }
}

fn downsample_samples(samples: &[f32], target_bins: usize) -> Vec<f32> {
    if samples.is_empty() || target_bins == 0 {
        return Vec::new();
    }

    let chunk = (samples.len() as f32 / target_bins as f32).ceil() as usize;
    samples
        .chunks(chunk.max(1))
        .take(target_bins)
        .map(|chunk| {
            chunk
                .iter()
                .map(|sample| sample.abs())
                .fold(0.0f32, f32::max)
                .min(1.0)
        })
        .collect()
}

fn send_snapshot(stdin: &mut ChildStdin, snapshot: &QtSnapshot) -> Result<()> {
    let line = serde_json::to_string(snapshot).context("serializing qt snapshot")?;
    stdin
        .write_all(line.as_bytes())
        .context("writing qt snapshot payload")?;
    stdin
        .write_all(b"\n")
        .context("writing qt snapshot newline")?;
    stdin.flush().context("flushing qt snapshot")?;
    Ok(())
}

fn apply_qt_command(
    command: &QtCommand,
    audio_state: &SharedAudioState,
    capture_control: Option<&Arc<CaptureControl>>,
    panel: &mut ControlPanelState,
    options: &OverlayRunOptions,
    running: &Arc<AtomicBool>,
    last_save_at: &mut Option<Instant>,
) {
    match command.command_type.as_str() {
        "toggle_pause" => {
            panel.toggle_pause();
            if let Some(control) = capture_control {
                panel.apply_pause(control);
            }
        }
        "toggle_injection" => {
            let mut state = audio_state.write();
            panel.toggle_injection(&mut state);
        }
        "toggle_viz" => {
            panel.toggle_viz_mode();
        }
        "toggle_agc" => {
            panel.toggle_agc();
            let mut state = audio_state.write();
            panel.apply_agc(&mut state);
        }
        "toggle_auto_save" => {
            panel.toggle_auto_save();
        }
        "cycle_device" => {
            let mut state = audio_state.write();
            panel.cycle_device(&mut state);
        }
        "cycle_model" => {
            let is_downloading = audio_state.read().download_progress.is_some();
            if is_downloading {
                (options.model_command)(OverlayModelCommand::Cancel(panel.model));
            } else {
                panel.toggle_model();
                (options.model_command)(OverlayModelCommand::Request(panel.model));
            }
        }
        "set_gain" => {
            if let Some(value) = command.value {
                panel.set_gain(value.clamp(0.5, 2.0));
                let mut state = audio_state.write();
                panel.apply_gain(&mut state);
            }
        }
        "quit" => {
            running.store(false, Ordering::Relaxed);
            if let Some(control) = capture_control {
                control.stop();
            }
        }
        other => log::debug!("Ignoring unsupported Qt command: {other}"),
    }

    maybe_auto_save(panel, audio_state, &options.config, last_save_at);
}

fn maybe_auto_save(
    panel: &ControlPanelState,
    audio_state: &SharedAudioState,
    config: &OverlayConfigContext,
    last_save_at: &mut Option<Instant>,
) {
    if !panel.auto_save {
        return;
    }

    let now = Instant::now();
    if let Some(last) = last_save_at {
        if now.duration_since(*last) < Duration::from_millis(500) {
            return;
        }
    }

    let runtime_config = build_runtime_config(
        panel,
        audio_state,
        config.source_override.clone(),
        config.model_dir.clone(),
    );
    if let Err(error) = runtime_config.save(&config.path) {
        log::warn!("Qt overlay auto-save failed: {}", error);
    } else {
        *last_save_at = Some(now);
    }
}
