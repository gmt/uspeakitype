//! Qt Quick frontend over newline-delimited JSON on the child process's stdio.
//!
//! Rust owns capture, recognition, configuration and injection. The graphical
//! process receives snapshots and sends explicit user commands.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::app::{OverlayModelCommand, OverlayRunOptions};
use super::control_panel::ControlPanelState;
use super::spectrogram::SpectrogramMode;
use super::{
    build_runtime_config, helper_mode_label, helper_model_label, helper_source_label,
    helper_status_short_summary, SharedAudioState,
};
use crate::audio::CaptureControl;
use crate::config::AsrModelId;
use crate::spectrum::{SpectrumAnalyzer, SpectrumConfig};

const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Debug, Deserialize)]
struct QtCommand {
    #[serde(rename = "type")]
    command_type: String,
    value: Option<QtCommandValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum QtCommandValue {
    Number(f32),
    Text(String),
}

impl QtCommand {
    fn number(&self) -> Option<f32> {
        match &self.value {
            Some(QtCommandValue::Number(number)) => Some(*number),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
struct QtModel {
    id: usize,
    label: String,
}

#[derive(Debug, Serialize)]
struct QtSnapshot {
    committed: String,
    partial: String,
    paused: bool,
    push_to_talk: bool,
    ptt_shortcut: String,
    ptt_held: bool,
    ptt_error: Option<String>,
    injection_enabled: bool,
    transcription_available: bool,
    auto_gain_enabled: bool,
    auto_save: bool,
    gain: f32,
    viz_mode: &'static str,
    color_scheme: &'static str,
    opacity: f32,
    helper_summary: String,
    helper_mode: String,
    source_label: String,
    model_label: String,
    models: Vec<QtModel>,
    selected_model: usize,
    error: Option<String>,
    download_progress: Option<f32>,
    spectrum: Vec<f32>,
}

/// Run the Qt Quick helper, shutting down the session on closure or failure.
pub fn run(
    audio_state: SharedAudioState,
    running: Arc<AtomicBool>,
    capture_control: Option<Arc<CaptureControl>>,
    options: OverlayRunOptions,
    tag: Option<String>,
) -> Result<()> {
    let result = run_inner(
        &audio_state,
        &running,
        capture_control.as_deref(),
        options,
        tag,
    );
    running.store(false, Ordering::Relaxed);
    if let Some(control) = capture_control {
        control.stop();
    }
    result
}

fn run_inner(
    audio_state: &SharedAudioState,
    running: &AtomicBool,
    capture_control: Option<&CaptureControl>,
    options: OverlayRunOptions,
    tag: Option<String>,
) -> Result<()> {
    let executable =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/qt-quick/usit-qt-overlay");
    anyhow::ensure!(
        executable.is_file(),
        "Qt Quick helper missing at {}. Run scripts/build-qt-overlay.sh before using --qt.",
        executable.display()
    );
    let mut command = Command::new(&executable);
    if let Some(tag) = tag {
        command.arg("--tag").arg(tag);
    }
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting Qt Quick helper {}", executable.display()))?;
    let mut process = QtProcess::new(child)?;
    let mut panel = options.control_panel.clone();
    if let Some(requested) = audio_state.read().requested_model {
        panel.model = requested;
    }
    let mut spectrum = SpectrumAnalyzer::new(SpectrumConfig {
        num_bands: 96,
        ..SpectrumConfig::default()
    });
    let mut save_error = None;

    while running.load(Ordering::Relaxed) {
        if let Some(status) = process
            .child
            .try_wait()
            .context("checking Qt Quick helper")?
        {
            anyhow::ensure!(status.success(), "Qt Quick helper exited with {status}");
            break;
        }
        // A bounded command queue prevents a misbehaving child from starving shutdown.
        for _ in 0..64 {
            let Ok(command) = process.commands.try_recv() else {
                break;
            };
            if let Err(error) = apply_command(
                &command,
                audio_state,
                capture_control,
                &mut panel,
                &options,
                running,
            ) {
                log::error!("Qt command failed: {error:#}");
                save_error = Some(format!("Settings: {error:#}"));
            } else {
                save_error = None;
            }
        }
        if !running.load(Ordering::Relaxed) {
            break;
        }
        if let Some(control) = capture_control {
            panel.is_paused = control.is_paused();
            panel.agc_enabled = control.is_auto_gain_enabled();
            panel.gain_value = control.get_manual_gain();
            let mut state = audio_state.write();
            state.is_paused = panel.is_paused;
            state.ptt_held = control.is_ptt_held();
            state.auto_gain_enabled = panel.agc_enabled;
            state.current_gain = control.get_current_gain();
        }
        let mut snapshot = build_snapshot(audio_state, &panel, &mut spectrum);
        if save_error.is_some() {
            snapshot.error = save_error.clone();
        }
        process.send(&snapshot)?;
        thread::sleep(SNAPSHOT_INTERVAL);
    }
    Ok(())
}

/// Own both pipes and the child so every error path terminates and reaps it.
struct QtProcess {
    child: Child,
    commands: Receiver<QtCommand>,
    snapshots: Option<SyncSender<String>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
}

impl QtProcess {
    fn new(child: Child) -> Result<Self> {
        let (command_tx, commands) = mpsc::sync_channel(64);
        let (snapshots, snapshot_rx) = mpsc::sync_channel::<String>(1);
        let mut process = Self {
            child,
            commands,
            snapshots: Some(snapshots),
            reader: None,
            writer: None,
        };
        let stdout = process
            .child
            .stdout
            .take()
            .context("Qt stdout unavailable")?;
        let mut stdin = process.child.stdin.take().context("Qt stdin unavailable")?;
        process.reader = Some(thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str(&line) {
                    Ok(command) => {
                        // Never block cleanup behind a full command queue.
                        let _ = command_tx.try_send(command);
                    }
                    Err(error) => log::warn!("Ignoring malformed Qt command: {error}"),
                }
            }
        }));
        process.writer = Some(thread::spawn(move || {
            while let Ok(line) = snapshot_rx.recv() {
                if stdin
                    .write_all(line.as_bytes())
                    .and_then(|_| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        }));
        Ok(process)
    }

    fn send(&self, snapshot: &QtSnapshot) -> Result<()> {
        let mut line = serde_json::to_string(snapshot).context("serializing Qt snapshot")?;
        line.push('\n');
        match self.snapshots.as_ref().unwrap().try_send(line) {
            Ok(()) | Err(TrySendError::Full(_)) => Ok(()),
            Err(TrySendError::Disconnected(_)) => anyhow::bail!("Qt Quick snapshot pipe closed"),
        }
    }
}

impl Drop for QtProcess {
    fn drop(&mut self) {
        // Killing first unblocks a pipe write even if the GUI stopped reading.
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.snapshots.take();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn build_snapshot(
    audio_state: &SharedAudioState,
    panel: &ControlPanelState,
    spectrum: &mut SpectrumAnalyzer,
) -> QtSnapshot {
    let state = audio_state.read();
    if state.is_paused || state.samples.is_empty() {
        spectrum.reset();
    } else {
        spectrum.push_samples(&state.samples);
        spectrum.process();
    }
    QtSnapshot {
        committed: state.committed.clone(),
        partial: state.partial.clone(),
        paused: state.is_paused,
        push_to_talk: state.push_to_talk,
        ptt_shortcut: state.ptt_shortcut.clone(),
        ptt_held: state.ptt_held,
        ptt_error: state.ptt_error.clone(),
        injection_enabled: state.injection_enabled,
        transcription_available: state.transcription_available,
        auto_gain_enabled: panel.agc_enabled,
        auto_save: panel.auto_save,
        gain: if panel.agc_enabled {
            state.current_gain
        } else {
            panel.gain_value
        },
        viz_mode: match panel.viz_mode {
            SpectrogramMode::BarMeter => "bars",
            SpectrogramMode::Waterfall => "waterfall",
        },
        color_scheme: panel.color_scheme_name,
        opacity: panel.opacity,
        helper_summary: helper_status_short_summary(&state),
        helper_mode: helper_mode_label(&state).to_string(),
        source_label: helper_source_label(&state),
        model_label: helper_model_label(&state),
        models: AsrModelId::all()
            .iter()
            .enumerate()
            .map(|(id, model)| QtModel {
                id,
                label: model.to_string(),
            })
            .collect(),
        selected_model: AsrModelId::all()
            .iter()
            .position(|model| *model == panel.model)
            .unwrap_or(0),
        error: state.model_error.clone(),
        download_progress: state.download_progress,
        spectrum: spectrum.data().bands.clone(),
    }
}

fn apply_command(
    command: &QtCommand,
    audio_state: &SharedAudioState,
    capture_control: Option<&CaptureControl>,
    panel: &mut ControlPanelState,
    options: &OverlayRunOptions,
    running: &AtomicBool,
) -> Result<()> {
    let mut persist = true;
    match command.command_type.as_str() {
        "toggle_pause" => {
            panel.toggle_pause();
            audio_state.write().is_paused = panel.is_paused;
            persist = false;
        }
        "toggle_ptt" => panel.toggle_ptt(&mut audio_state.write()),
        "set_ptt_shortcut" => {
            let Some(QtCommandValue::Text(value)) = &command.value else {
                return Ok(());
            };
            let shortcut = value.trim();
            anyhow::ensure!(
                !shortcut.is_empty() && shortcut.len() <= 128,
                "Talk shortcut must contain 1 to 128 characters"
            );
            let mut state = audio_state.write();
            state.set_ptt_held(false);
            state.ptt_shortcut = shortcut.to_string();
            state.ptt_error = None;
        }
        "toggle_injection" => panel.toggle_injection(&mut audio_state.write()),
        "toggle_viz" => panel.toggle_viz_mode(),
        "toggle_agc" => {
            panel.toggle_agc();
            panel.apply_agc(&mut audio_state.write());
        }
        "toggle_auto_save" => {
            // Persist disabling as well as enabling auto-save.
            panel.toggle_auto_save();
            save_config(panel, audio_state, options)?;
            persist = false;
        }
        "cycle_device" => panel.cycle_device(&mut audio_state.write()),
        "cycle_model" => {
            let state = audio_state.read();
            let downloading = state.download_progress.is_some();
            let requested = state.requested_model.unwrap_or(panel.model);
            drop(state);
            if downloading {
                (options.model_command)(OverlayModelCommand::Cancel(requested));
                persist = false;
            } else {
                panel.toggle_model();
                (options.model_command)(OverlayModelCommand::Request(panel.model));
            }
        }
        "select_model" => {
            let Some(index) = command.number().filter(|value| {
                value.is_finite()
                    && *value >= 0.0
                    && value.fract() == 0.0
                    && *value < AsrModelId::all().len() as f32
            }) else {
                return Ok(());
            };
            panel.model = AsrModelId::all()[index as usize];
            (options.model_command)(OverlayModelCommand::Request(panel.model));
        }
        "cancel_model" => {
            let state = audio_state.read();
            if state.download_progress.is_some() {
                let requested = state.requested_model.unwrap_or(panel.model);
                drop(state);
                (options.model_command)(OverlayModelCommand::Cancel(requested));
            }
            persist = false;
        }
        "set_gain" => {
            if let Some(value) = command.number().filter(|v| v.is_finite()) {
                panel.set_gain(value);
                panel.apply_gain(&mut audio_state.write());
            } else {
                return Ok(());
            }
        }
        "set_opacity" => {
            if let Some(value) = command.number().filter(|v| v.is_finite()) {
                panel.opacity = value.clamp(0.1, 1.0);
            } else {
                return Ok(());
            }
        }
        "cycle_color" => panel.set_color_scheme(match panel.color_scheme_name {
            "flame" => "ice",
            "ice" => "mono",
            _ => "flame",
        }),
        "save" => {
            save_config(panel, audio_state, options)?;
            persist = false;
        }
        "quit" => {
            running.store(false, Ordering::Relaxed);
            persist = false;
        }
        other => {
            log::debug!("Ignoring unsupported Qt command: {other}");
            return Ok(());
        }
    }
    if let Some(control) = capture_control {
        panel.apply_capture_settings(control);
    }
    // Explicit changes only: the continuously changing AGC estimate never enters config.
    if persist && panel.auto_save {
        save_config(panel, audio_state, options)?;
    }
    Ok(())
}

fn save_config(
    panel: &ControlPanelState,
    audio_state: &SharedAudioState,
    options: &OverlayRunOptions,
) -> Result<()> {
    build_runtime_config(
        panel,
        audio_state,
        options.config.source_override.clone(),
        options.config.model_dir.clone(),
    )
    .save(&options.config.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{new_shared_state, AudioSourceInfo};

    fn options(path: PathBuf) -> OverlayRunOptions {
        OverlayRunOptions {
            control_panel: ControlPanelState::new(),
            config: super::super::app::OverlayConfigContext {
                path,
                source_override: None,
                model_dir: None,
            },
            model_command: Arc::new(|_| {}),
        }
    }

    fn command(name: &str, value: Option<f32>) -> QtCommand {
        QtCommand {
            command_type: name.to_string(),
            value: value.map(QtCommandValue::Number),
        }
    }

    #[test]
    fn commands_reach_capture_without_saving_automatic_gain() {
        let dir = tempfile::tempdir().unwrap();
        let options = options(dir.path().join("usit.toml"));
        let state = new_shared_state();
        let control = CaptureControl::new();
        let mut panel = options.control_panel.clone();
        let running = AtomicBool::new(true);
        let apply = |name, value, panel: &mut ControlPanelState| {
            apply_command(
                &command(name, value),
                &state,
                Some(&control),
                panel,
                &options,
                &running,
            )
            .unwrap();
        };
        apply("set_gain", Some(1.5), &mut panel);
        assert_eq!(control.get_manual_gain(), 1.5);
        apply("toggle_agc", None, &mut panel);
        assert!(control.is_auto_gain_enabled());
        state.write().current_gain = 42.0;
        apply("set_gain", Some(0.5), &mut panel);
        assert_eq!(control.get_manual_gain(), 1.5, "AGC disables manual edits");
        apply("save", None, &mut panel);
        let saved = std::fs::read_to_string(&options.config.path).unwrap();
        assert!(saved.contains("gain = 1.5"));
        apply("toggle_agc", None, &mut panel);
        assert_eq!(control.get_manual_gain(), 1.5);
        apply("toggle_pause", None, &mut panel);
        assert!(control.is_paused());
        assert!(state.read().is_paused);
        apply("quit", None, &mut panel);
        assert!(!running.load(Ordering::Relaxed));
    }

    #[test]
    fn ptt_commands_persist_shortcut_and_keep_numeric_commands_strict() {
        let dir = tempfile::tempdir().unwrap();
        let options = options(dir.path().join("usit.toml"));
        let state = new_shared_state();
        let mut panel = options.control_panel.clone();
        let running = AtomicBool::new(true);
        apply_command(
            &command("toggle_ptt", None),
            &state,
            None,
            &mut panel,
            &options,
            &running,
        )
        .unwrap();
        assert!(state.read().push_to_talk);
        state.write().set_ptt_held(true);
        let shortcut: QtCommand =
            serde_json::from_str(r#"{"type":"set_ptt_shortcut","value":"Ctrl+F8"}"#).unwrap();
        apply_command(&shortcut, &state, None, &mut panel, &options, &running).unwrap();
        assert_eq!(state.read().ptt_shortcut, "Ctrl+F8");
        assert!(
            !state.read().ptt_held,
            "a changed shortcut must release the old gate"
        );
        let config = crate::config::Config::load(&options.config.path).unwrap();
        assert!(config.push_to_talk);
        assert_eq!(config.ptt_shortcut, "Ctrl+F8");
        let invalid_gain: QtCommand =
            serde_json::from_str(r#"{"type":"set_gain","value":"1.5"}"#).unwrap();
        apply_command(&invalid_gain, &state, None, &mut panel, &options, &running).unwrap();
        assert_eq!(
            panel.gain_value, 1.0,
            "numeric controls must reject text values"
        );
        let invalid_shortcut: QtCommand =
            serde_json::from_str(r#"{"type":"set_ptt_shortcut","value":" "}"#).unwrap();
        assert!(apply_command(
            &invalid_shortcut,
            &state,
            None,
            &mut panel,
            &options,
            &running
        )
        .is_err());
        assert_eq!(state.read().ptt_shortcut, "Ctrl+F8");
        state.write().ptt_error = Some("Shortcut already in use".to_string());
        let mut spectrum = SpectrumAnalyzer::new(SpectrumConfig::default());
        let snapshot = build_snapshot(&state, &panel, &mut spectrum);
        assert!(snapshot.push_to_talk);
        assert_eq!(snapshot.ptt_shortcut, "Ctrl+F8");
        assert_eq!(
            snapshot.ptt_error.as_deref(),
            Some("Shortcut already in use")
        );
    }

    #[test]
    fn direct_model_selection_only_requests_valid_explicit_choice() {
        let dir = tempfile::tempdir().unwrap();
        let mut options = options(dir.path().join("usit.toml"));
        let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let observed = requests.clone();
        options.model_command = Arc::new(move |command| observed.lock().push(command));
        let state = new_shared_state();
        let mut panel = options.control_panel.clone();
        panel.auto_save = false;
        let running = AtomicBool::new(true);
        for value in [
            None,
            Some(-1.0),
            Some(0.5),
            Some(f32::NAN),
            Some(f32::INFINITY),
            Some(AsrModelId::all().len() as f32),
        ] {
            apply_command(
                &command("select_model", value),
                &state,
                None,
                &mut panel,
                &options,
                &running,
            )
            .unwrap();
        }
        assert!(requests.lock().is_empty());
        let index = AsrModelId::all()
            .iter()
            .position(|model| *model == AsrModelId::ParakeetTdt06bV3)
            .unwrap();
        apply_command(
            &command("select_model", Some(index as f32)),
            &state,
            None,
            &mut panel,
            &options,
            &running,
        )
        .unwrap();
        assert_eq!(panel.model, AsrModelId::ParakeetTdt06bV3);
        assert!(matches!(
            requests.lock().as_slice(),
            [OverlayModelCommand::Request(AsrModelId::ParakeetTdt06bV3)]
        ));
        let mut spectrum = SpectrumAnalyzer::new(SpectrumConfig::default());
        let snapshot = build_snapshot(&state, &panel, &mut spectrum);
        assert_eq!(snapshot.selected_model, index);
        assert_eq!(snapshot.models[index].id, index);
        assert_eq!(snapshot.models[index].label, panel.model.to_string());
        // Cancel the actual download, not a newer selection awaiting activation.
        state.write().requested_model = Some(AsrModelId::MoonshineTiny);
        state.write().download_progress = Some(0.25);
        apply_command(
            &command("cancel_model", None),
            &state,
            None,
            &mut panel,
            &options,
            &running,
        )
        .unwrap();
        assert!(matches!(
            requests.lock().last(),
            Some(OverlayModelCommand::Cancel(AsrModelId::MoonshineTiny))
        ));
    }

    #[test]
    fn snapshots_expose_real_spectrum_and_pending_source() {
        let state = new_shared_state();
        {
            let mut state = state.write();
            state.samples = (0..2048)
                .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 16000.0).sin() * 0.25)
                .collect();
            state.available_sources.push(AudioSourceInfo {
                id: 12,
                name: "next-mic".to_string(),
                description: "Next mic".to_string(),
            });
            state.selected_source_id = Some(12);
            state.source_change_pending_restart = true;
            state.model_error = Some("Model unavailable".to_string());
        }
        let mut spectrum = SpectrumAnalyzer::new(SpectrumConfig {
            num_bands: 96,
            ..SpectrumConfig::default()
        });
        let panel = ControlPanelState::new();
        let snapshot = build_snapshot(&state, &panel, &mut spectrum);
        assert!(snapshot.source_label.contains("next launch"));
        assert_eq!(snapshot.helper_mode, "Display-only");
        assert!(!snapshot.transcription_available);
        assert_eq!(snapshot.error.as_deref(), Some("Model unavailable"));
        assert_eq!(snapshot.spectrum.len(), 96);
        assert!(snapshot.spectrum.iter().all(|v| (0.0..=1.0).contains(v)));
        let peak = snapshot.spectrum.iter().copied().fold(0.0, f32::max);
        assert!(peak > 0.0);
        assert!(
            snapshot.spectrum.iter().any(|v| *v < peak * 0.1),
            "a tone must occupy frequency bands rather than duplicate waveform peaks"
        );
        state.write().is_paused = true;
        let snapshot = build_snapshot(&state, &panel, &mut spectrum);
        assert!(snapshot.spectrum.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn dropping_helper_unblocks_full_pipe_and_reaps_child() {
        let child = Command::new("/usr/bin/sleep")
            .arg("30")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let process = QtProcess::new(child).unwrap();
        // sleep deliberately never reads stdin: the writer must not trap shutdown.
        process
            .snapshots
            .as_ref()
            .unwrap()
            .send("x".repeat(1024 * 1024))
            .unwrap();
        let start = std::time::Instant::now();
        drop(process);
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}
