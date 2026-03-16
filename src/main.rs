use anyhow::Result;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

mod ansi;
#[path = "audio/capture.rs"]
mod capture;
mod config;
mod cpl;
mod download;
mod inject;
mod logging;
mod model_cache;
#[path = "backend/moonshine.rs"]
mod moonshine;
#[path = "spectrum.rs"]
mod spectrum;
mod streaming;
#[path = "audio/vad.rs"]
mod vad;

use capture::{AudioCapture, CaptureConfig};
use config::{AsrModelId, Config};
use cpl::{install_qt_controls, RuntimeControls};
use inject::{Fcitx5BridgeInjector, TextInjector};
use moonshine::MoonshineStreamer;
use spectrum::{SpectrumAnalyzer, SpectrumConfig};
use streaming::{DefaultStreamingTranscriber, StreamEvent, StreamingConfig};
use vad::{SileroVad, VadConfig};

const BIN_COUNT: usize = 96;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct FrameSnapshot {
    level: f32,
    peak: f32,
    bins: [f32; BIN_COUNT],
}

impl Default for FrameSnapshot {
    fn default() -> Self {
        Self {
            level: 0.0,
            peak: 0.0,
            bins: [0.0; BIN_COUNT],
        }
    }
}

type FrameSink = Arc<dyn Fn(FrameSnapshot, &str) + Send + Sync + 'static>;

#[derive(Clone, Default)]
pub(crate) struct TranscriptSnapshot {
    pub(crate) committed: String,
    pub(crate) partial: String,
}

#[derive(Default)]
pub(crate) struct TranscriptState {
    inner: parking_lot::Mutex<TranscriptSnapshot>,
}

impl TranscriptState {
    pub(crate) fn snapshot(&self) -> TranscriptSnapshot {
        self.inner.lock().clone()
    }

    fn set_partial(&self, text: String) {
        self.inner.lock().partial = text;
    }

    fn commit(&self, text: String) {
        let mut inner = self.inner.lock();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            if !inner.committed.is_empty() {
                inner.committed.push(' ');
            }
            inner.committed.push_str(trimmed);
        }
        inner.partial.clear();
    }

    fn set_error(&self, error: impl Into<String>) {
        let mut inner = self.inner.lock();
        inner.partial = format!("ERR: {}", error.into());
    }
}

unsafe extern "C" {
    fn usit_qt_run() -> i32;
    fn usit_qt_set_status(text: *const std::os::raw::c_char);
    fn usit_qt_set_transcript(
        committed: *const std::os::raw::c_char,
        partial: *const std::os::raw::c_char,
    );
    fn usit_qt_publish_frame(frame: *const FrameSnapshot);
    fn usit_qt_request_quit();
}

fn set_status(text: &str) {
    let status = CString::new(text).expect("status text should be valid");
    unsafe {
        usit_qt_set_status(status.as_ptr());
    }
}

fn publish_frame(frame: &FrameSnapshot) {
    unsafe {
        usit_qt_publish_frame(frame as *const FrameSnapshot);
    }
}

fn set_transcript(committed: &str, partial: &str) {
    let committed = CString::new(committed).expect("committed text should be valid");
    let partial = CString::new(partial).expect("partial text should be valid");
    unsafe {
        usit_qt_set_transcript(committed.as_ptr(), partial.as_ptr());
    }
}

fn fill_bins(frame_number: u64, bins: &mut [f32; BIN_COUNT]) {
    for (index, bin) in bins.iter_mut().enumerate() {
        let x = index as f32 / BIN_COUNT as f32;
        let wave = ((frame_number as f32 / 18.0) + x * 10.0).sin() * 0.24 + 0.34;
        let shimmer = ((frame_number as f32 / 11.0) + x * 33.0).cos() * 0.14 + 0.16;
        let ridge = (((frame_number as f32 / 43.0) + x * 4.0).sin() * 0.5 + 0.5).powf(3.2);
        let pulse = (((frame_number as f32 / 27.0) + x * 7.0).cos() * 0.5 + 0.5).powf(5.0) * 0.3;
        *bin = (wave + shimmer + ridge * 0.38 + pulse).clamp(0.02, 1.0);
    }
}

fn spawn_demo_producer(
    running: Arc<AtomicBool>,
    controls: Arc<RuntimeControls>,
    sink: FrameSink,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut frame_number = 0u64;
        let mut snapshot = FrameSnapshot {
            level: 0.0,
            peak: 0.0,
            bins: [0.0; BIN_COUNT],
        };

        while running.load(Ordering::Relaxed) {
            let paused = controls.capture().is_paused();
            let manual_gain = controls.capture().get_manual_gain();
            fill_bins(frame_number, &mut snapshot.bins);
            let phase = frame_number as f32 / 17.0;
            let level = ((phase.sin() * 0.5 + 0.5) * (manual_gain / 2.0)).clamp(0.0, 1.0);
            snapshot.level = if paused { 0.0 } else { level };
            snapshot.peak = if paused {
                0.0
            } else {
                snapshot.bins.iter().copied().fold(0.0f32, f32::max)
            };
            if paused {
                snapshot.bins.fill(0.0);
            }

            let mode = if paused {
                "demo paused"
            } else if (frame_number / 80) % 2 == 0 {
                "demo sweep"
            } else {
                "warm glass bars"
            };
            let status = format!(
                "{mode} · {:.0}% level · {:.1}x gain",
                snapshot.level * 100.0,
                manual_gain
            );
            sink(snapshot, &status);

            frame_number += 1;
            thread::sleep(Duration::from_millis(33));
        }
    })
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let mean_square =
        samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32;
    mean_square.sqrt()
}

fn start_live_capture(
    source: Option<String>,
    controls: Arc<RuntimeControls>,
    sink: FrameSink,
    audio_tx: Option<mpsc::Sender<Vec<f32>>>,
) -> Result<AudioCapture> {
    let status_label = source
        .as_deref()
        .map(|name| format!("live capture · requested source {name}"))
        .unwrap_or_else(|| "live capture · default source".to_string());
    sink(
        FrameSnapshot::default(),
        &format!("{status_label} · waiting for audio"),
    );

    let analyzer = Arc::new(std::sync::Mutex::new(SpectrumAnalyzer::new(
        SpectrumConfig {
            num_bands: BIN_COUNT,
            smoothing: 0.2,
            ..Default::default()
        },
    )));
    let live_label = status_label.clone();

    AudioCapture::with_control(
        Box::new(move |samples| {
            let level = (rms(samples) * 5.5).clamp(0.0, 1.0);
            if let Some(audio_tx) = audio_tx.as_ref() {
                let _ = audio_tx.send(samples.to_vec());
            }
            let Ok(mut analyzer) = analyzer.lock() else {
                return;
            };
            analyzer.push_samples(samples);
            if !analyzer.process() {
                return;
            }
            let data = analyzer.data();
            let mut snapshot = FrameSnapshot {
                level,
                peak: data.peak,
                bins: [0.0; BIN_COUNT],
            };
            let copy_len = data.bands.len().min(BIN_COUNT);
            snapshot.bins[..copy_len].copy_from_slice(&data.bands[..copy_len]);
            let status = format!(
                "{live_label} · {:.0}% level · {:.0}% peak",
                snapshot.level * 100.0,
                snapshot.peak * 100.0
            );
            sink(snapshot, &status);
        }),
        CaptureConfig {
            auto_gain_enabled: controls.capture().is_auto_gain_enabled(),
            gain: controls.capture().get_manual_gain(),
            source,
            ..Default::default()
        },
        controls.capture(),
    )
}

fn spawn_autostop(
    limit_ms: u64,
    running: Arc<AtomicBool>,
    on_timeout: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(limit_ms));
        running.store(false, Ordering::Relaxed);
        if let Some(on_timeout) = on_timeout {
            on_timeout();
        }
    })
}

fn parse_source(args: &[String]) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == "--source").then(|| window[1].clone()))
}

fn parse_gain(args: &[String]) -> Option<f32> {
    args.windows(2)
        .find_map(|window| (window[0] == "--gain").then(|| window[1].parse::<f32>().ok()))
        .flatten()
}

fn parse_model(args: &[String]) -> Option<AsrModelId> {
    let raw = args
        .windows(2)
        .find_map(|window| (window[0] == "--model").then(|| window[1].clone()))?;
    AsrModelId::all()
        .iter()
        .copied()
        .find(|model| model.dir_name() == raw)
}

fn parse_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

fn parse_config_path(args: &[String]) -> Option<std::path::PathBuf> {
    args.windows(2)
        .find_map(|window| (window[0] == "--config").then(|| window[1].clone()))
        .map(Into::into)
}

fn parse_model_dir(args: &[String]) -> Option<std::path::PathBuf> {
    args.windows(2)
        .find_map(|window| (window[0] == "--model-dir").then(|| window[1].clone()))
        .map(Into::into)
}

fn qt_sink() -> FrameSink {
    Arc::new(|frame, status| {
        set_status(status);
        publish_frame(&frame);
    })
}

fn emit_startup_probe(enabled: bool, started_at: Instant, label: &str) {
    if !enabled {
        return;
    }

    let elapsed_ms = started_at.elapsed().as_millis();
    eprintln!("usit startup probe: +{elapsed_ms}ms {label}");
}

fn prepare_requested_model(
    model_dir: &std::path::Path,
    model: AsrModelId,
    sink: &FrameSink,
    startup_probe_enabled: bool,
    startup_started_at: Instant,
) -> Result<()> {
    let asr_dir = model_dir.join(model.dir_name());
    sink(
        FrameSnapshot::default(),
        &format!("model prep · checking cache for {}", model),
    );
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "checking model cache",
    );

    match model_cache::prepare_for_activation(&asr_dir, model) {
        model_cache::ActivationResult::Success => {
            emit_startup_probe(
                startup_probe_enabled,
                startup_started_at,
                "model cache ready",
            );
            Ok(())
        }
        model_cache::ActivationResult::NeedsDownload
        | model_cache::ActivationResult::Quarantined => {
            sink(
                FrameSnapshot::default(),
                &format!("model prep · downloading {}", model),
            );
            emit_startup_probe(
                startup_probe_enabled,
                startup_started_at,
                "starting model download",
            );
            download::ensure_models_exist(model_dir, model)?;
            model_cache::seal_model(&asr_dir, model)?;
            emit_startup_probe(
                startup_probe_enabled,
                startup_started_at,
                "model download complete",
            );
            Ok(())
        }
    }
}

fn load_requested_model(
    model_dir: &std::path::Path,
    model: AsrModelId,
    sink: &FrameSink,
    startup_probe_enabled: bool,
    startup_started_at: Instant,
) -> Result<MoonshineStreamer> {
    let asr_dir = model_dir.join(model.dir_name());
    sink(
        FrameSnapshot::default(),
        &format!("model load · initializing {}", model),
    );
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "starting moonshine runtime init",
    );
    moonshine::init_ort();
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "moonshine runtime ready",
    );

    sink(
        FrameSnapshot::default(),
        &format!("model load · opening {}", model),
    );
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "starting moonshine model load",
    );
    let model = MoonshineStreamer::new(&asr_dir)?;
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "moonshine model ready",
    );
    Ok(model)
}

fn load_requested_vad(
    model_dir: &std::path::Path,
    sink: &FrameSink,
    startup_probe_enabled: bool,
    startup_started_at: Instant,
) -> Result<SileroVad> {
    let vad_path = model_dir.join("silero_vad.onnx");
    sink(FrameSnapshot::default(), "vad load · opening silero");
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "starting silero load",
    );
    let vad = SileroVad::new(&vad_path, VadConfig::default())?;
    emit_startup_probe(startup_probe_enabled, startup_started_at, "silero ready");
    Ok(vad)
}

fn spawn_transcription_worker(
    running: Arc<AtomicBool>,
    model: MoonshineStreamer,
    vad: SileroVad,
    controls: Arc<RuntimeControls>,
    transcript: Arc<TranscriptState>,
    injection_tx: Option<mpsc::Sender<String>>,
) -> (mpsc::Sender<Vec<f32>>, thread::JoinHandle<()>) {
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();

    let handle = thread::spawn(move || {
        let mut transcriber =
            DefaultStreamingTranscriber::new(vad, Box::new(model), StreamingConfig::default());

        while running.load(Ordering::Relaxed) {
            let Ok(samples) = audio_rx.recv_timeout(Duration::from_millis(100)) else {
                continue;
            };

            match transcriber.process(&samples) {
                Ok(events) => {
                    for event in events {
                        match event {
                            StreamEvent::Partial(text) => {
                                transcript.set_partial(text);
                                let snapshot = transcript.snapshot();
                                set_transcript(&snapshot.committed, &snapshot.partial);
                            }
                            StreamEvent::Commit(text) => {
                                transcript.commit(text.clone());
                                let snapshot = transcript.snapshot();
                                set_transcript(&snapshot.committed, &snapshot.partial);
                                if !controls.capture().is_paused() {
                                    if let Some(injection_tx) = injection_tx.as_ref() {
                                        let _ = injection_tx.send(text);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    log::warn!("transcription worker error: {error}");
                    transcript.set_error(error.to_string());
                    let snapshot = transcript.snapshot();
                    set_transcript(&snapshot.committed, &snapshot.partial);
                }
            }
        }
    });

    (audio_tx, handle)
}

fn spawn_fcitx5_injector(
    startup_probe_enabled: bool,
    startup_started_at: Instant,
) -> Result<(mpsc::Sender<String>, thread::JoinHandle<()>)> {
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "starting fcitx5 bridge probe",
    );
    let injector = Fcitx5BridgeInjector::new_passive()?;
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "fcitx5 bridge ready",
    );

    let (injection_tx, injection_rx) = mpsc::channel::<String>();
    let handle = thread::spawn(move || {
        let mut injector: Box<dyn TextInjector> = Box::new(injector);
        while let Ok(text) = injection_rx.recv() {
            if let Err(error) = injector.inject(&text) {
                log::error!("fcitx5 injection error: {}", error);
            }
        }
    });

    Ok((injection_tx, handle))
}

fn main() -> Result<()> {
    let startup_probe_enabled = std::env::var_os("USIT_STARTUP_PROBE").is_some();
    let startup_started_at = Instant::now();
    emit_startup_probe(startup_probe_enabled, startup_started_at, "entered main");

    logging::init(false)?;
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "logging initialized",
    );

    let running = Arc::new(AtomicBool::new(true));
    let autostop_ms = std::env::var("USIT_AUTOSTOP_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            std::env::var("NUXGLIT_AUTOSTOP_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
        });
    let args: Vec<String> = std::env::args().skip(1).collect();
    let demo_mode = parse_flag(&args, "--demo");
    let ansi_mode = parse_flag(&args, "--ansi");
    let config_path = parse_config_path(&args).unwrap_or_else(Config::config_path);
    let file_config = Config::load(&config_path)?;
    emit_startup_probe(startup_probe_enabled, startup_started_at, "config loaded");

    let source = parse_source(&args).or(file_config.source.clone());
    let auto_gain = if parse_flag(&args, "--auto-gain") {
        true
    } else if parse_flag(&args, "--no-auto-gain") {
        false
    } else {
        file_config.auto_gain
    };
    let gain = parse_gain(&args)
        .unwrap_or(file_config.gain)
        .clamp(0.1, 10.0);
    let requested_model = parse_model(&args).unwrap_or(file_config.model);
    let requested_model = if requested_model.is_moonshine() {
        requested_model
    } else {
        log::warn!(
            "Parakeet reintroduction is not wired in this tranche yet; falling back to {}",
            AsrModelId::MoonshineBase
        );
        AsrModelId::MoonshineBase
    };
    let model_dir = parse_model_dir(&args)
        .or(file_config.model_dir.clone())
        .unwrap_or_else(Config::default_model_dir);
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        "arguments parsed",
    );
    let source_label = source
        .as_deref()
        .map(|value| format!("requested source: {value}"))
        .unwrap_or_else(|| "requested source: default".to_string());
    let controls = RuntimeControls::new(source_label, auto_gain, gain);
    let transcript = Arc::new(TranscriptState::default());
    set_transcript("", "");

    let (sink, frontend) = if ansi_mode {
        let state = Arc::new(ansi::AnsiState::default());
        (
            {
                let state = state.clone();
                Arc::new(move |frame, status: &str| state.publish(frame, status)) as FrameSink
            },
            Frontend::Ansi(state, controls.clone(), transcript.clone()),
        )
    } else {
        install_qt_controls(controls.clone());
        (qt_sink(), Frontend::Qt)
    };

    let autostop = autostop_ms.map(|limit_ms| {
        let on_timeout = match frontend {
            Frontend::Qt => Some(Arc::new(|| unsafe {
                usit_qt_request_quit();
            }) as Arc<dyn Fn() + Send + Sync + 'static>),
            Frontend::Ansi(_, _, _) => None,
        };
        spawn_autostop(limit_ms, running.clone(), on_timeout)
    });

    let mut producer = if demo_mode {
        let demo_status = if ansi_mode {
            format!("demo mode · ansi visualizer · gain {:.2}x", gain)
        } else {
            format!("demo mode · synthesizing bars · gain {:.2}x", gain)
        };
        sink(FrameSnapshot::default(), &demo_status);
        Some(spawn_demo_producer(
            running.clone(),
            controls.clone(),
            sink.clone(),
        ))
    } else {
        None
    };

    let mut loaded_model = None;
    let mut loaded_vad = None;
    let mut transcription_worker = None;
    let mut injector_handle = None;
    let mut audio_tx = None;
    let mut capture = if demo_mode {
        None
    } else {
        if let Err(error) = prepare_requested_model(
            &model_dir,
            requested_model,
            &sink,
            startup_probe_enabled,
            startup_started_at,
        ) {
            log::warn!("model prep unavailable ({error}); continuing as visualizer");
            sink(
                FrameSnapshot::default(),
                "model prep failed · continuing as visualizer",
            );
        } else {
            match load_requested_model(
                &model_dir,
                requested_model,
                &sink,
                startup_probe_enabled,
                startup_started_at,
            ) {
                Ok(model) => {
                    sink(
                        FrameSnapshot::default(),
                        &format!("model ready · {}", requested_model),
                    );
                    loaded_model = Some(model);
                }
                Err(error) => {
                    log::warn!("model load unavailable ({error}); continuing as visualizer");
                    sink(
                        FrameSnapshot::default(),
                        "model load failed · continuing as visualizer",
                    );
                }
            }

            match load_requested_vad(&model_dir, &sink, startup_probe_enabled, startup_started_at) {
                Ok(vad) => {
                    sink(FrameSnapshot::default(), "vad ready · silero");
                    loaded_vad = Some(vad);
                }
                Err(error) => {
                    log::warn!("vad load unavailable ({error}); continuing without VAD");
                    sink(
                        FrameSnapshot::default(),
                        "vad load failed · continuing without VAD",
                    );
                }
            }

            if let (Some(model), Some(vad)) = (loaded_model.take(), loaded_vad.take()) {
                let injection_tx =
                    match spawn_fcitx5_injector(startup_probe_enabled, startup_started_at) {
                        Ok((tx, handle)) => {
                            injector_handle = Some(handle);
                            sink(
                                FrameSnapshot::default(),
                                "transcription ready · fcitx5 bridge active",
                            );
                            Some(tx)
                        }
                        Err(error) => {
                            log::warn!(
                                "fcitx5 bridge unavailable ({error}); continuing without injection"
                            );
                            sink(
                                FrameSnapshot::default(),
                                "transcription ready · display-only mode",
                            );
                            None
                        }
                    };
                let (tx, handle) = spawn_transcription_worker(
                    running.clone(),
                    model,
                    vad,
                    controls.clone(),
                    transcript.clone(),
                    injection_tx,
                );
                audio_tx = Some(tx);
                transcription_worker = Some(handle);
            }
        }

        emit_startup_probe(
            startup_probe_enabled,
            startup_started_at,
            "starting live capture bootstrap",
        );
        match start_live_capture(
            source.clone(),
            controls.clone(),
            sink.clone(),
            audio_tx.clone(),
        ) {
            Ok(capture) => Some(capture),
            Err(error) => {
                log::warn!("live capture unavailable ({error}); falling back to demo");
                sink(
                    FrameSnapshot::default(),
                    "live capture failed · falling back to demo",
                );
                producer = Some(spawn_demo_producer(running.clone(), controls, sink));
                None
            }
        }
    };
    emit_startup_probe(
        startup_probe_enabled,
        startup_started_at,
        if ansi_mode {
            "entering ansi event loop"
        } else {
            "entering Qt event loop"
        },
    );

    run_frontend(
        frontend,
        running,
        producer,
        autostop,
        capture.take(),
        loaded_model,
        loaded_vad,
        transcription_worker,
        injector_handle,
    )
}

#[derive(Clone)]
enum Frontend {
    Qt,
    Ansi(
        Arc<ansi::AnsiState>,
        Arc<RuntimeControls>,
        Arc<TranscriptState>,
    ),
}

fn run_frontend(
    frontend: Frontend,
    running: Arc<AtomicBool>,
    producer: Option<thread::JoinHandle<()>>,
    autostop: Option<thread::JoinHandle<()>>,
    mut capture: Option<AudioCapture>,
    _loaded_model: Option<MoonshineStreamer>,
    _loaded_vad: Option<SileroVad>,
    transcription_worker: Option<thread::JoinHandle<()>>,
    injector_handle: Option<thread::JoinHandle<()>>,
) -> Result<()> {
    let result = match frontend {
        Frontend::Qt => {
            let exit_code = unsafe { usit_qt_run() };
            if exit_code == 0 {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Qt app exited with status {exit_code}"))
            }
        }
        Frontend::Ansi(state, controls, transcript) => {
            ansi::run(state, controls, transcript, running.clone())
        }
    };

    running.store(false, Ordering::Relaxed);
    if let Some(mut capture) = capture.take() {
        capture.stop();
    }
    if let Some(producer) = producer {
        let _ = producer.join();
    }
    if let Some(autostop) = autostop {
        let _ = autostop.join();
    }
    if let Some(worker) = transcription_worker {
        let _ = worker.join();
    }
    if let Some(handle) = injector_handle {
        let _ = handle.join();
    }

    result
}
