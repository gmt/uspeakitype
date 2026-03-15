use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal;
use supports_unicode::Stream;
use terminal_size::{terminal_size, Height, Width};

use usit::audio::{self, AudioCapture, CaptureConfig, CaptureControl};
use usit::config::{AsrModelId, Config};
use usit::instance::{find_duplicate_tag, find_instances};
use usit::model_cache::{self, ActivationResult};
use usit::spectrum::get_color_scheme;
use usit::ui;
use usit::ui::spectrogram::SpectrogramMode;
use usit::ui::terminal::{
    clamp_terminal_surface_dimensions, TerminalConfig, TerminalMode, TerminalVisualizer,
};
use usit::{backend, download, streaming};

/// Commands sent to the download manager thread
#[derive(Debug)]
enum DownloadCommand {
    /// Request download/activation of a model
    Request(AsrModelId),
    /// Cancel a specific model's download
    Cancel(AsrModelId),
    /// Shutdown the download manager
    Shutdown,
}

/// Events sent from download manager back to main thread
#[derive(Debug)]
enum DownloadEvent {
    /// Download completed successfully for a model
    Completed(AsrModelId),
    /// Download failed for a model
    Failed(AsrModelId, String),
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum DemoOverlayState {
    Default,
    Display,
    Transcribe,
    Trusted,
    Downloading,
    Error,
}

/// Manages async model downloads with progress tracking
struct DownloadManager {
    /// Channel to receive download commands
    cmd_rx: std::sync::mpsc::Receiver<DownloadCommand>,
    /// Channel to send completion/failure events
    event_tx: std::sync::mpsc::Sender<DownloadEvent>,
    /// Shared audio state for progress updates
    audio_state: ui::SharedAudioState,
    /// Model directory
    model_dir: PathBuf,
    /// Per-model cancel tokens
    cancel_tokens: HashMap<AsrModelId, Arc<AtomicBool>>,
    /// Currently active downloads (model -> join handle)
    active_downloads: HashMap<AsrModelId, std::thread::JoinHandle<()>>,
}

impl DownloadManager {
    fn new(
        cmd_rx: std::sync::mpsc::Receiver<DownloadCommand>,
        event_tx: std::sync::mpsc::Sender<DownloadEvent>,
        audio_state: ui::SharedAudioState,
        model_dir: PathBuf,
    ) -> Self {
        Self {
            cmd_rx,
            event_tx,
            audio_state,
            model_dir,
            cancel_tokens: HashMap::new(),
            active_downloads: HashMap::new(),
        }
    }

    fn run(mut self) {
        loop {
            self.cleanup_finished_downloads();

            // Process commands (non-blocking poll with timeout)
            match self.cmd_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(DownloadCommand::Request(model_id)) => {
                    // If already downloading this model, ignore
                    if self.active_downloads.contains_key(&model_id) {
                        log::debug!("Download already in progress for {}", model_id);
                        continue;
                    }

                    // Set as requested model in shared state
                    self.audio_state.write().requested_model = Some(model_id);

                    // Check if already cached
                    let asr_dir = self.model_dir.join(model_id.dir_name());
                    match model_cache::prepare_for_activation(&asr_dir, model_id) {
                        ActivationResult::Success => {
                            // Already cached and valid, send completion
                            log::info!("Model {} already cached and valid", model_id);
                            let _ = self.event_tx.send(DownloadEvent::Completed(model_id));
                            continue;
                        }
                        ActivationResult::NeedsDownload | ActivationResult::Quarantined => {
                            // Start download
                            self.start_download(model_id);
                        }
                    }
                }
                Ok(DownloadCommand::Cancel(model_id)) => {
                    if let Some(token) = self.cancel_tokens.get(&model_id) {
                        token.store(true, Ordering::Relaxed);
                        log::info!("Cancelling download for {}", model_id);
                    }
                }
                Ok(DownloadCommand::Shutdown) => {
                    // Cancel all active downloads
                    for token in self.cancel_tokens.values() {
                        token.store(true, Ordering::Relaxed);
                    }
                    self.join_all_downloads();
                    log::debug!("Download manager shutting down");
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Normal timeout, continue loop
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Channel closed, exit
                    log::debug!("Download manager channel disconnected");
                    break;
                }
            }
        }
    }

    fn cleanup_finished_downloads(&mut self) {
        let finished: Vec<_> = self
            .active_downloads
            .iter()
            .filter_map(|(&model_id, handle)| handle.is_finished().then_some(model_id))
            .collect();

        for model_id in finished {
            if let Some(handle) = self.active_downloads.remove(&model_id) {
                if handle.join().is_err() {
                    log::error!("Download worker panicked for {}", model_id);
                }
            }
            self.cancel_tokens.remove(&model_id);
        }
    }

    fn join_all_downloads(&mut self) {
        let active = std::mem::take(&mut self.active_downloads);
        for (model_id, handle) in active {
            if handle.join().is_err() {
                log::error!("Download worker panicked for {}", model_id);
            }
        }
        self.cancel_tokens.clear();
    }

    fn start_download(&mut self, model_id: AsrModelId) {
        let cancel_token = Arc::new(AtomicBool::new(false));
        self.cancel_tokens.insert(model_id, cancel_token.clone());

        let model_dir = self.model_dir.clone();
        let audio_state = self.audio_state.clone();
        let event_tx = self.event_tx.clone();

        let handle = std::thread::spawn(move || {
            log::info!("Starting download for model: {}", model_id);

            // Progress callback that updates shared state
            let progress_state = audio_state.clone();
            let progress_callback: Box<dyn Fn(f64) + Send + Sync> =
                Box::new(move |progress: f64| {
                    let mut state = progress_state.write();
                    state
                        .download_progress_by_model
                        .insert(model_id, progress as f32);
                    // Also set legacy download_progress if this is the requested model
                    if state.requested_model == Some(model_id) {
                        state.download_progress = Some(progress as f32);
                    }
                });

            let result = download::ensure_models_exist_with_progress(
                &model_dir,
                model_id,
                Some(progress_callback),
                Some(&cancel_token),
            );

            // Clear progress
            {
                let mut state = audio_state.write();
                state.download_progress_by_model.remove(&model_id);
                if state.requested_model == Some(model_id) {
                    state.download_progress = None;
                }
            }

            match result {
                Ok(_) => {
                    // Seal the model after successful download
                    let asr_dir = model_dir.join(model_id.dir_name());
                    if let Err(e) = model_cache::seal_model(&asr_dir, model_id) {
                        log::warn!("Failed to seal downloaded model: {}", e);
                    }
                    log::info!("Download completed for model: {}", model_id);
                    let _ = event_tx.send(DownloadEvent::Completed(model_id));
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("cancelled") {
                        log::info!("Download cancelled for model: {}", model_id);
                    } else {
                        log::error!("Download failed for model {}: {}", model_id, e);
                        let _ = event_tx.send(DownloadEvent::Failed(model_id, msg));
                    }
                }
            }
        });

        self.active_downloads.insert(model_id, handle);
    }
}

/// Normalize backend names: lowercase, trim, filter unknown
fn normalize_backend_names(raw: &[String]) -> Vec<String> {
    const KNOWN: &[&str] = &["input_method", "wrtype", "ydotool"];
    raw.iter()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| {
            if KNOWN.contains(&s.as_str()) {
                true
            } else {
                eprintln!("[usit] Warning: unknown backend '{}', ignoring", s);
                false
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SpectrogramStyle {
    Bars,
    Waterfall,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ColorSchemeName {
    Flame,
    Ice,
    Mono,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AnsiCharset {
    Auto,
    Ascii,
    #[value(alias = "unicode")]
    Blocks,
}

#[derive(Parser)]
#[command(name = "usit")]
#[command(about = "Streaming speech-to-text with live revision")]
#[command(next_line_help = false, term_width = 0)]
#[command(after_help = "Keybindings: Space=pause, w=viz toggle, c=control panel, q/Esc=quit")]
struct Args {
    #[arg(long, help = "Text-only output, no visualization")]
    headless: bool,

    #[arg(long, help = "Use synthetic audio instead of microphone")]
    demo: bool,

    #[arg(
        long,
        value_enum,
        default_value = "default",
        hide = true,
        requires = "demo",
        help = "Synthetic helper state for visual regression coverage"
    )]
    demo_overlay_state: DemoOverlayState,

    #[arg(
        long,
        hide = true,
        requires = "demo",
        help = "Open the helper panel immediately for deterministic UI captures"
    )]
    demo_open_panel: bool,

    #[arg(long, help = "Enable automatic gain control")]
    auto_gain: bool,

    #[arg(long, help = "List available audio sources and exit")]
    list_sources: bool,

    #[arg(long, help = "Terminal UI mode (instead of graphical overlay)")]
    ansi: bool,

    #[arg(long, hide = true, help = "Use the legacy WGPU overlay instead of the Qt shell")]
    wgpu_legacy: bool,

    #[arg(long, help = "Spectrogram width in characters")]
    ansi_width: Option<usize>,

    #[arg(long, help = "Spectrogram height in characters")]
    ansi_height: Option<usize>,

    #[arg(
        long,
        value_enum,
        default_value = "auto",
        hide_possible_values = true,
        hide_default_value = true,
        help = "Character set: auto, ascii, blocks [default: auto]"
    )]
    ansi_charset: AnsiCharset,

    #[arg(long, help = "Frequency sweep demo (tests spectrogram)")]
    ansi_sweep: bool,

    #[arg(
        long,
        value_enum,
        default_value = "bars",
        hide_possible_values = true,
        hide_default_value = true,
        help = "Visualization: bars, waterfall [default: bars]"
    )]
    style: SpectrogramStyle,

    #[arg(
        long,
        value_enum,
        default_value = "flame",
        hide_possible_values = true,
        hide_default_value = true,
        help = "Color scheme: flame, ice, mono [default: flame]"
    )]
    color: ColorSchemeName,

    #[arg(
        long,
        value_enum,
        default_value = "moonshine-base",
        hide_possible_values = true,
        hide_default_value = true,
        help = "ASR model: moonshine-base, moonshine-tiny, moonshine-tiny-{ar,zh,ja,ko,uk,vi}, parakeet-tdt-0.6b-v3 [default: moonshine-base]"
    )]
    model: AsrModelId,

    #[arg(long, help = "Disable colors in terminal output")]
    no_color: bool,

    #[arg(long, help = "Audio source (see --list-sources)")]
    source: Option<String>,

    #[arg(long, help = "Model directory [default: ~/.cache/usit/models]")]
    model_dir: Option<PathBuf>,

    #[arg(
        long,
        default_value = "0.85",
        hide_default_value = true,
        help = "Window opacity 0.0-1.0 [default: 0.85]"
    )]
    opacity: f32,

    #[arg(long, hide = true, help = "Run visual test sequence in tmux")]
    test_fireworks: bool,

    #[arg(
        long,
        value_delimiter = ',',
        help = "Disable backends (input_method,wrtype,ydotool)"
    )]
    backend_disable: Vec<String>,

    #[arg(long, help = "Auto-start ydotoold daemon if needed")]
    autostart_ydotoold: bool,

    #[arg(long, help = "Tag for instance identification")]
    tag: Option<String>,

    #[arg(long, help = "Error if another instance has same tag")]
    no_duplicate_tag: bool,

    #[arg(long, help = "List running usit instances and exit")]
    list_instances: bool,

    #[arg(
        long,
        short = 'H',
        requires = "list_instances",
        help = "Human-readable format for --list-instances"
    )]
    human: bool,
}

/// Result of model activation attempt
struct ModelActivation {
    transcriber: Option<streaming::DefaultStreamingTranscriber>,
    #[allow(dead_code)] // May be used for UI display of active model
    active_model: Option<AsrModelId>,
    error: Option<String>,
}

/// Attempt to activate a model, with fallback to other cached/downloadable models.
///
/// Order of attempts:
/// 1. Selected model (from CLI/config)
/// 2. Other cached models (verified integrity)
/// 3. Download attempts in priority order (Base → Tiny → Parakeet)
///
/// Returns ModelActivation with transcriber if successful, or error message if all fail.
fn activate_model_with_fallback(model_dir: &Path, preferred_model: AsrModelId) -> ModelActivation {
    use audio::{SileroVad, VadConfig};
    use backend::{MoonshineStreamer, NemoTransducerStreamer};
    use streaming::{BoxedTranscriber, StreamingConfig, StreamingTranscriber};

    let mut attempted: std::collections::HashSet<AsrModelId> = std::collections::HashSet::new();
    let mut last_error: Option<String> = None;

    // Build ordered list: preferred first, then cached, then fallback order
    let mut candidates: Vec<AsrModelId> = vec![preferred_model];

    // Add cached models (except preferred, already first)
    for cached in model_cache::find_cached_models(model_dir) {
        if cached != preferred_model && !candidates.contains(&cached) {
            candidates.push(cached);
        }
    }

    // Add remaining models from fallback order
    for fallback in model_cache::fallback_order() {
        if !candidates.contains(&fallback) {
            candidates.push(fallback);
        }
    }

    for model_id in candidates {
        if attempted.contains(&model_id) {
            continue;
        }
        attempted.insert(model_id);

        let asr_dir = model_dir.join(model_id.dir_name());

        // Check integrity first
        let activation_result = model_cache::prepare_for_activation(&asr_dir, model_id);

        match activation_result {
            ActivationResult::Success => {
                // Model cache is valid, try to load it
                log::info!("Attempting to activate model: {}", model_id);
            }
            ActivationResult::NeedsDownload => {
                // Try to download
                log::info!("Model {} not cached, attempting download...", model_id);
                match download::ensure_models_exist(model_dir, model_id) {
                    Ok(_) => {
                        // Downloaded successfully, seal it
                        if let Err(e) = model_cache::seal_model(&asr_dir, model_id) {
                            log::warn!("Failed to seal downloaded model: {}", e);
                        }
                    }
                    Err(e) => {
                        last_error = Some(format!("Download failed for {}: {}", model_id, e));
                        log::warn!("{}", last_error.as_ref().unwrap());
                        continue;
                    }
                }
            }
            ActivationResult::Quarantined => {
                // Model was corrupt and quarantined, try to re-download
                log::warn!(
                    "Model {} was corrupt and quarantined, attempting re-download...",
                    model_id
                );
                match download::ensure_models_exist(model_dir, model_id) {
                    Ok(_) => {
                        if let Err(e) = model_cache::seal_model(&asr_dir, model_id) {
                            log::warn!("Failed to seal re-downloaded model: {}", e);
                        }
                    }
                    Err(e) => {
                        last_error = Some(format!("Re-download failed for {}: {}", model_id, e));
                        log::warn!("{}", last_error.as_ref().unwrap());
                        continue;
                    }
                }
            }
        }

        // Now try to load VAD and transcriber
        let silero_path = model_dir.join("silero_vad.onnx");

        // Check VAD integrity (it's shared)
        if !silero_path.exists() {
            // Need to download VAD
            if let Err(e) = download::ensure_models_exist(model_dir, model_id) {
                last_error = Some(format!("VAD download failed: {}", e));
                log::warn!("{}", last_error.as_ref().unwrap());
                continue;
            }
        }

        backend::init_ort();

        let vad = match SileroVad::new(&silero_path, VadConfig::default()) {
            Ok(v) => v,
            Err(e) => {
                last_error = Some(format!("VAD initialization failed: {}", e));
                log::error!("{}", last_error.as_ref().unwrap());
                // VAD is shared, if it fails we have a serious problem
                // Quarantine it and try re-download next iteration
                if let Err(qe) = std::fs::remove_file(&silero_path) {
                    log::warn!("Failed to remove corrupt VAD: {}", qe);
                }
                continue;
            }
        };

        let transcriber_result: Result<BoxedTranscriber, _> = if model_id.is_moonshine() {
            log::info!("Initializing Moonshine transcriber...");
            MoonshineStreamer::new(&asr_dir).map(|t| Box::new(t) as BoxedTranscriber)
        } else {
            log::info!("Initializing NeMo transducer transcriber...");
            NemoTransducerStreamer::new(&asr_dir).map(|t| Box::new(t) as BoxedTranscriber)
        };

        match transcriber_result {
            Ok(transcriber) => {
                log::info!("Model {} activated successfully", model_id);
                let streamer =
                    StreamingTranscriber::new(vad, transcriber, StreamingConfig::default());
                return ModelActivation {
                    transcriber: Some(streamer),
                    active_model: Some(model_id),
                    error: None,
                };
            }
            Err(e) => {
                last_error = Some(format!("Transcriber init failed for {}: {}", model_id, e));
                log::error!("{}", last_error.as_ref().unwrap());
                // Quarantine the corrupt model
                if let Err(qe) = model_cache::quarantine_model(&asr_dir, model_id) {
                    log::warn!("Failed to quarantine corrupt model: {}", qe);
                }
                continue;
            }
        }
    }

    // All attempts failed
    let error_msg = last_error.unwrap_or_else(|| "No models available".to_string());
    log::error!("All model activation attempts failed: {}", error_msg);

    ModelActivation {
        transcriber: None,
        active_model: None,
        error: Some(error_msg),
    }
}

/// Attempt to activate a single model without fallback.
/// Used for DD activation where we want quick success/failure.
fn activate_single_model(model_dir: &Path, model_id: AsrModelId) -> ModelActivation {
    use audio::{SileroVad, VadConfig};
    use backend::{MoonshineStreamer, NemoTransducerStreamer};
    use streaming::{BoxedTranscriber, StreamingConfig, StreamingTranscriber};

    let asr_dir = model_dir.join(model_id.dir_name());
    let silero_path = model_dir.join("silero_vad.onnx");

    // Check VAD exists
    if !silero_path.exists() {
        return ModelActivation {
            transcriber: None,
            active_model: None,
            error: Some("VAD not found".to_string()),
        };
    }

    backend::init_ort();

    let vad = match SileroVad::new(&silero_path, VadConfig::default()) {
        Ok(v) => v,
        Err(e) => {
            return ModelActivation {
                transcriber: None,
                active_model: None,
                error: Some(format!("VAD initialization failed: {}", e)),
            };
        }
    };

    let transcriber_result: Result<BoxedTranscriber, _> = if model_id.is_moonshine() {
        log::info!("Initializing Moonshine transcriber...");
        MoonshineStreamer::new(&asr_dir).map(|t| Box::new(t) as BoxedTranscriber)
    } else {
        log::info!("Initializing NeMo transducer transcriber...");
        NemoTransducerStreamer::new(&asr_dir).map(|t| Box::new(t) as BoxedTranscriber)
    };

    match transcriber_result {
        Ok(transcriber) => {
            log::info!("Model {} activated successfully", model_id);
            let streamer = StreamingTranscriber::new(vad, transcriber, StreamingConfig::default());
            ModelActivation {
                transcriber: Some(streamer),
                active_model: Some(model_id),
                error: None,
            }
        }
        Err(e) => ModelActivation {
            transcriber: None,
            active_model: None,
            error: Some(format!("Transcriber init failed: {}", e)),
        },
    }
}

/// RAII guard to restore tmux pane size on drop
struct RestoreGuard {
    width: String,
    height: String,
}

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        use std::process::Command;

        let _ = Command::new("tmux")
            .args(["resize-pane", "-x", &self.width, "-y", &self.height])
            .status();
    }
}

/// Visual test mode: cycles through terminal sizes in tmux
fn run_fireworks_test() -> anyhow::Result<()> {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    if std::env::var("TMUX").is_err() {
        eprintln!("Not in tmux session");
        return Ok(());
    }

    let output = Command::new("tmux")
        .args(["display", "-p", "#{pane_width}x#{pane_height}"])
        .output()?;
    let orig_size = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let parts: Vec<&str> = orig_size.split('x').collect();
    let orig_width = parts[0];
    let orig_height = parts[1];

    let _guard = RestoreGuard {
        width: orig_width.to_string(),
        height: orig_height.to_string(),
    };

    let test_sizes = [
        (20, 6, "Degenerate (20x6)"),
        (30, 8, "Minimal (30x8)"),
        (40, 12, "Compact (40x12)"),
        (50, 12, "Full (50x12)"),
        (80, 24, "Full (80x24)"),
    ];

    for (width, height, name) in &test_sizes {
        Command::new("tmux")
            .args([
                "resize-pane",
                "-x",
                &width.to_string(),
                "-y",
                &height.to_string(),
            ])
            .status()?;

        eprintln!("Testing: {}", name);

        let (surface_width, surface_height) =
            clamp_terminal_surface_dimensions(None, None, *width as usize, *height as usize);

        let config = TerminalConfig {
            width: surface_width,
            height: surface_height,
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: *width as usize,
            term_height: *height as usize,
        };

        let mut visualizer = TerminalVisualizer::new(config, None);
        visualizer.init_terminal()?;
        visualizer.process_and_render()?;

        thread::sleep(Duration::from_millis(500));

        TerminalVisualizer::cleanup_terminal()?;
        terminal::disable_raw_mode()?;
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let is_tui = args.headless || args.ansi || args.ansi_sweep;
    usit::logging::init(is_tui)?;
    let config = Config::load_or_default();

    if args.test_fireworks {
        return run_fireworks_test();
    }

    if args.list_instances {
        let instances = find_instances(None);
        if instances.is_empty() {
            if !args.human {
                // Machine: no output for empty
            } else {
                println!("No usit instances running");
            }
        } else if args.human {
            println!("{:<10} TAG", "PID");
            for inst in &instances {
                let tag_display = match &inst.tag {
                    None => "(untagged)",
                    Some(t) if t.is_empty() => "(empty)",
                    Some(t) => t.as_str(),
                };
                println!("{:<10} {}", inst.pid, tag_display);
            }
        } else {
            for inst in &instances {
                let tag = inst
                    .tag
                    .as_deref()
                    .map(|t| {
                        // Escape control characters
                        t.replace('\\', "\\\\")
                            .replace('\t', "\\t")
                            .replace('\n', "\\n")
                            .replace('\r', "\\r")
                    })
                    .unwrap_or_default(); // None and Some("") both become ""
                println!("{}\t{}", inst.pid, tag);
            }
        }
        std::process::exit(0);
    }

    let model_dir = args
        .model_dir
        .clone()
        .or(config.model_dir.clone())
        .unwrap_or_else(|| {
            dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("usit/models")
        });

    if args.list_sources {
        println!("Available audio sources:");
        match audio::list_audio_sources() {
            Ok(sources) => {
                for source in sources {
                    println!("  [{}] {} - {}", source.id, source.name, source.description);
                }
            }
            Err(e) => {
                eprintln!("Failed to list sources: {}", e);
            }
        }
        return Ok(());
    }

    // Check for duplicate tag
    if let Some(ref tag) = args.tag {
        if let Some(pid) = find_duplicate_tag(tag) {
            if args.no_duplicate_tag {
                eprintln!(
                    "Error: usit with tag '{}' already running (PID {})",
                    tag, pid
                );
                std::process::exit(1);
            } else {
                eprintln!(
                    "Warning: usit with tag '{}' may already be running (PID {})",
                    tag, pid
                );
            }
        }
    }

    log::info!("usit v{}", env!("CARGO_PKG_VERSION"));

    let running = Arc::new(AtomicBool::new(true));

    // Set up signal handler for graceful shutdown
    // This allows threads (especially the IME injector) to clean up properly
    {
        let running = running.clone();
        ctrlc::set_handler(move || {
            log::info!("Received shutdown signal");
            running.store(false, Ordering::SeqCst);
        })
        .expect("Failed to set signal handler");
    }

    let audio_state = ui::new_shared_state();

    // Apply config: injection_enabled
    audio_state.write().injection_enabled = config.injection_enabled;

    let runtime_source = args.source.clone().or(config.source.clone());
    let persisted_source_base = config.source.clone();
    let startup_sources = if args.demo || args.ansi_sweep {
        {
            let mut state = audio_state.write();
            state.available_sources = Vec::new();
            state.selected_source_id = runtime_source
                .as_deref()
                .and_then(|source| source.parse::<u32>().ok());
            state.selected_source_name = runtime_source.clone();
            state.session_source_name = runtime_source.clone();
            state.source_change_pending_restart = false;
        }
        Vec::new()
    } else {
        seed_audio_source_state(&audio_state, runtime_source.as_deref())
    };

    // Create download manager channels
    let (download_cmd_tx, download_cmd_rx): (
        std::sync::mpsc::Sender<DownloadCommand>,
        std::sync::mpsc::Receiver<DownloadCommand>,
    ) = std::sync::mpsc::channel();
    let (download_event_tx, download_event_rx): (
        std::sync::mpsc::Sender<DownloadEvent>,
        std::sync::mpsc::Receiver<DownloadEvent>,
    ) = std::sync::mpsc::channel();

    // Spawn download manager thread
    let download_manager = DownloadManager::new(
        download_cmd_rx,
        download_event_tx,
        audio_state.clone(),
        model_dir.clone(),
    );
    let download_manager_handle = std::thread::spawn(move || download_manager.run());

    // Model activation with fallback - non-fatal on failure
    // Phase 2: Non-blocking startup with designated driver
    let (streaming_transcriber, transcription_available): (
        Option<streaming::DefaultStreamingTranscriber>,
        bool,
    ) = if args.demo || args.ansi_sweep {
        // Demo mode: no transcription, just visualization
        (None, false)
    } else {
        log::info!("Loading models from {:?}...", model_dir);
        let resolved_model = if args.model != AsrModelId::default() {
            args.model
        } else {
            config.model
        };

        // Set as requested model
        audio_state.write().requested_model = Some(resolved_model);

        // Try to find a DD (designated driver) - a cached model we can use immediately
        let cached_models = model_cache::find_cached_models(&model_dir);
        let dd_candidate = if cached_models.contains(&resolved_model) {
            // Preferred model is cached, use it as DD
            Some(resolved_model)
        } else if !cached_models.is_empty() {
            // Use first available cached model as DD
            Some(cached_models[0])
        } else {
            None
        };

        // Try to activate DD immediately (non-blocking for downloads)
        let activation = if let Some(dd_model) = dd_candidate {
            let asr_dir = model_dir.join(dd_model.dir_name());
            match model_cache::prepare_for_activation(&asr_dir, dd_model) {
                ActivationResult::Success => {
                    // DD is valid, activate it
                    log::info!("Activating designated driver model: {}", dd_model);
                    let quick_activation = activate_single_model(&model_dir, dd_model);

                    if quick_activation.active_model.is_some() {
                        quick_activation
                    } else {
                        let reason = quick_activation
                            .error
                            .as_deref()
                            .unwrap_or("unknown activation error");
                        log::warn!(
                            "Designated driver {} failed fast activation ({}), trying fallback",
                            dd_model,
                            reason
                        );
                        activate_model_with_fallback(&model_dir, resolved_model)
                    }
                }
                _ => {
                    // DD needs download or was quarantined, try others
                    log::warn!("DD candidate {} not ready, trying fallback", dd_model);
                    activate_model_with_fallback(&model_dir, resolved_model)
                }
            }
        } else {
            // No cached models, try fallback (may block on downloads)
            // In future we could skip this and start async download instead
            activate_model_with_fallback(&model_dir, resolved_model)
        };

        // If requested model is different from active and needs download, start async download
        if activation.active_model != Some(resolved_model) {
            let asr_dir = model_dir.join(resolved_model.dir_name());
            match model_cache::prepare_for_activation(&asr_dir, resolved_model) {
                ActivationResult::NeedsDownload | ActivationResult::Quarantined => {
                    log::info!("Requesting async download for {}", resolved_model);
                    let _ = download_cmd_tx.send(DownloadCommand::Request(resolved_model));
                }
                ActivationResult::Success => {
                    // Requested model is already cached, will be swapped later
                }
            }
        }

        // Update shared state
        if let Some(active) = activation.active_model {
            audio_state.write().active_model = Some(active);
        }

        if let Some(error) = &activation.error {
            // Set error state for UI display
            audio_state.write().model_error = Some(error.clone());

            if args.headless {
                // In headless mode with no model, exit cleanly
                log::error!("No model available in headless mode, exiting");
                eprintln!("Error: {}", error);
                std::process::exit(0);
            }
        }

        if activation.transcriber.is_some() {
            audio_state.write().transcription_available = true;
        }

        let has_transcriber = activation.transcriber.is_some();
        (activation.transcriber, has_transcriber)
    };

    let (audio_tx, audio_rx): (
        std::sync::mpsc::SyncSender<Vec<f32>>,
        std::sync::mpsc::Receiver<Vec<f32>>,
    ) = std::sync::mpsc::sync_channel(100);

    if args.demo {
        apply_demo_overlay_state(&audio_state, args.demo_overlay_state);
    }

    // Injection channel for typing transcribed text into focused apps
    let (injection_tx, injection_rx): (
        std::sync::mpsc::Sender<String>,
        std::sync::mpsc::Receiver<String>,
    ) = std::sync::mpsc::channel();

    // Model hot-swap channel: TUI sends new AsrModelId, streaming thread receives
    let (model_swap_tx, model_swap_rx): (
        std::sync::mpsc::Sender<AsrModelId>,
        std::sync::mpsc::Receiver<AsrModelId>,
    ) = std::sync::mpsc::channel();

    let download_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Note: download_cancel_for_worker kept for compatibility with legacy sync download path
    let _download_cancel_for_worker = download_cancel.clone();
    let download_cancel_for_overlay = download_cancel.clone();

    // Extract before spawn (args used after spawn, can't move)
    let backend_disable = args.backend_disable.clone();
    let autostart_ydotoold = args.autostart_ydotoold;
    let is_tui = args.headless || args.ansi || args.ansi_sweep;

    // Spawn injector thread - handle stored so we can join on shutdown
    // to ensure proper Wayland IME cleanup (Drop runs before process exit)
    //
    // IMPORTANT: If transcription is not available, we do NOT register as an input device.
    // This ensures we never break the user's input system when models fail to load.
    let injector_handle = std::thread::spawn(move || {
        use usit::input::{find_ydotool_socket, select_backend, TextInjector};

        // If no transcription is available, skip input injection entirely.
        // This ensures we don't register as an input device when we can't transcribe.
        if !transcription_available {
            log::info!("Input injection: disabled (no transcription model available)");
            // Still consume the channel to prevent sender blocking
            while injection_rx.recv().is_ok() {}
            return;
        }

        // 1. Normalize backend-disable list
        let disabled = normalize_backend_names(&backend_disable);

        // 2. Handle --autostart-ydotoold BEFORE selection
        if autostart_ydotoold && find_ydotool_socket().is_none() {
            log::info!("Starting ydotoold daemon...");
            let _ = std::process::Command::new("ydotoold").spawn();
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Re-check socket existence once after wait
            if find_ydotool_socket().is_none() {
                log::warn!("ydotoold started but socket still not found");
                log::warn!("ydotool backend may fail during probe");
            }
        }

        // 3. Select backend with normalized disabled list
        let mut injector: Box<dyn TextInjector> = match select_backend(&disabled, is_tui) {
            Some(inj) => {
                log::info!("Input injection: {}", inj.name());
                inj
            }
            None => {
                log::info!("Input injection: unavailable (display-only mode)");
                return;
            }
        };

        // 4. Run injection loop - exits when channel closes (sender dropped)
        while let Ok(text) = injection_rx.recv() {
            if let Err(e) = injector.inject(&text) {
                log::error!("Injection error: {}", e);
            }
        }
        // injector drops here, triggering IME cleanup
        log::debug!("Injector thread exiting");
    });

    let (capture_control, _capture_handle): (Option<Arc<CaptureControl>>, Option<AudioCapture>) =
        if args.demo || args.ansi_sweep {
            if args.ansi_sweep {
                run_sweep_audio(audio_state.clone());
            } else {
                run_demo_audio(audio_state.clone(), injection_tx.clone());
            }
            (None, None)
        } else {
            let auto_gain = args.auto_gain || config.auto_gain;
            let capture_config = CaptureConfig {
                auto_gain_enabled: auto_gain,
                agc: Default::default(),
                source: runtime_source.clone(),
            };

            let state = audio_state.clone();
            let tx = audio_tx.clone();
            let capture = AudioCapture::new(
                Box::new(move |samples| {
                    state.write().update_samples(samples);
                    if tx.try_send(samples.to_vec()).is_err() {}
                }),
                capture_config,
            )?;

            let control = capture.control().clone();

            if auto_gain {
                audio_state.write().auto_gain_enabled = true;
            }

            // Keep capture alive - dropping it triggers shutdown cascade
            // (closes audio channel → worker exits → injection channel closes → injector exits)
            (Some(control), Some(capture))
        };

    // Clone download_cmd_tx for TUI use (the original will be moved to shutdown)
    let download_cmd_tx_for_tui = download_cmd_tx.clone();
    let download_cmd_tx_for_overlay = download_cmd_tx.clone();
    let model_swap_tx_for_overlay = model_swap_tx.clone();

    // Spawn streaming worker - handles audio processing and model swaps
    // Runs even without initial transcriber to handle download events and late activation
    {
        use streaming::StreamEvent;
        let audio_state_for_worker = audio_state.clone();
        let injection_tx_for_worker = injection_tx.clone();
        let model_dir_for_worker = model_dir.clone();

        // Helper to create a new streamer from a model
        fn create_streamer(
            model_dir: &Path,
            model_id: AsrModelId,
        ) -> Option<streaming::DefaultStreamingTranscriber> {
            use audio::{SileroVad, VadConfig};
            use streaming::{BoxedTranscriber, StreamingConfig, StreamingTranscriber};

            // Initialize ONNX Runtime before creating VAD or transcriber
            backend::init_ort();

            let asr_dir = model_dir.join(model_id.dir_name());
            let silero_path = model_dir.join("silero_vad.onnx");

            if !silero_path.exists() {
                log::warn!("VAD not found at {:?}", silero_path);
                return None;
            }

            let vad = match SileroVad::new(&silero_path, VadConfig::default()) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("VAD initialization failed: {}", e);
                    return None;
                }
            };

            let transcriber_result: Result<BoxedTranscriber, _> = if model_id.is_moonshine() {
                backend::MoonshineStreamer::new(&asr_dir).map(|t| Box::new(t) as BoxedTranscriber)
            } else {
                backend::NemoTransducerStreamer::new(&asr_dir)
                    .map(|t| Box::new(t) as BoxedTranscriber)
            };

            match transcriber_result {
                Ok(transcriber) => Some(StreamingTranscriber::new(
                    vad,
                    transcriber,
                    StreamingConfig::default(),
                )),
                Err(e) => {
                    log::error!("Transcriber init failed: {}", e);
                    None
                }
            }
        }

        std::thread::spawn(move || {
            // Start with optional transcriber
            let mut streamer = streaming_transcriber;

            while let Ok(samples) = audio_rx.recv() {
                // Check for model swap requests from TUI (non-blocking)
                if let Ok(new_variant) = model_swap_rx.try_recv() {
                    // Update requested model in shared state
                    audio_state_for_worker.write().requested_model = Some(new_variant);

                    // Check if this model is already cached and can be activated immediately
                    let asr_dir = model_dir_for_worker.join(new_variant.dir_name());
                    match model_cache::prepare_for_activation(&asr_dir, new_variant) {
                        ActivationResult::Success => {
                            // Model is cached and valid, activate/swap
                            if let Some(ref mut s) = streamer {
                                // Swap transcriber in existing streamer
                                let new_transcriber = if new_variant.is_moonshine() {
                                    backend::MoonshineStreamer::new(&asr_dir)
                                        .map(|t| Box::new(t) as streaming::BoxedTranscriber)
                                } else {
                                    backend::NemoTransducerStreamer::new(&asr_dir)
                                        .map(|t| Box::new(t) as streaming::BoxedTranscriber)
                                };

                                match new_transcriber {
                                    Ok(new_transcriber) => {
                                        s.swap_transcriber(new_transcriber);
                                        let mut state = audio_state_for_worker.write();
                                        state.active_model = Some(new_variant);
                                        state.transcription_available = true;
                                        state.model_error = None;
                                        log::info!("Model swapped to {}", new_variant);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to swap model: {}", e);
                                    }
                                }
                            } else {
                                // No streamer yet, create one
                                if let Some(new_streamer) =
                                    create_streamer(&model_dir_for_worker, new_variant)
                                {
                                    streamer = Some(new_streamer);
                                    let mut state = audio_state_for_worker.write();
                                    state.active_model = Some(new_variant);
                                    state.transcription_available = true;
                                    state.model_error = None;
                                    log::info!("Model activated: {}", new_variant);
                                }
                            }
                        }
                        ActivationResult::NeedsDownload | ActivationResult::Quarantined => {
                            // Download manager will handle this via download_cmd_tx
                            log::info!(
                                "Model {} needs download, waiting for download manager",
                                new_variant
                            );
                        }
                    }
                }

                // Check for download completion events (non-blocking)
                if let Ok(event) = download_event_rx.try_recv() {
                    match event {
                        DownloadEvent::Completed(completed_model) => {
                            // Only activate if this matches the current requested model
                            // AND it's not already the active model (avoid duplicate activation)
                            let (requested, active) = {
                                let state = audio_state_for_worker.read();
                                (state.requested_model, state.active_model)
                            };
                            if requested == Some(completed_model) && active != Some(completed_model)
                            {
                                log::info!(
                                    "Download completed for requested model {}, activating",
                                    completed_model
                                );
                                let asr_dir = model_dir_for_worker.join(completed_model.dir_name());

                                if let Some(ref mut s) = streamer {
                                    // Swap transcriber in existing streamer
                                    let new_transcriber = if completed_model.is_moonshine() {
                                        backend::MoonshineStreamer::new(&asr_dir)
                                            .map(|t| Box::new(t) as streaming::BoxedTranscriber)
                                    } else {
                                        backend::NemoTransducerStreamer::new(&asr_dir)
                                            .map(|t| Box::new(t) as streaming::BoxedTranscriber)
                                    };

                                    match new_transcriber {
                                        Ok(new_transcriber) => {
                                            s.swap_transcriber(new_transcriber);
                                            let mut state = audio_state_for_worker.write();
                                            state.active_model = Some(completed_model);
                                            state.transcription_available = true;
                                            state.model_error = None;
                                            log::info!("Model swapped to {}", completed_model);
                                        }
                                        Err(e) => {
                                            log::error!(
                                                "Failed to activate downloaded model: {}",
                                                e
                                            );
                                        }
                                    }
                                } else {
                                    // No streamer yet, create one
                                    if let Some(new_streamer) =
                                        create_streamer(&model_dir_for_worker, completed_model)
                                    {
                                        streamer = Some(new_streamer);
                                        let mut state = audio_state_for_worker.write();
                                        state.active_model = Some(completed_model);
                                        state.transcription_available = true;
                                        state.model_error = None;
                                        log::info!(
                                            "Model activated after download: {}",
                                            completed_model
                                        );
                                    }
                                }
                            } else if active == Some(completed_model) {
                                // Model is already active (likely activated via model_swap_rx)
                                log::debug!(
                                    "Download completed for {} but it's already active, skipping",
                                    completed_model
                                );
                            } else {
                                log::info!("Download completed for {} but requested model is {:?}, not activating",
                                    completed_model, requested);
                            }
                        }
                        DownloadEvent::Failed(failed_model, error) => {
                            log::error!("Download failed for {}: {}", failed_model, error);
                            // Set error if this was the requested model
                            let requested = audio_state_for_worker.read().requested_model;
                            if requested == Some(failed_model) {
                                audio_state_for_worker.write().model_error = Some(error);
                            }
                        }
                    }
                }

                // Process audio if we have a streamer
                if let Some(ref mut s) = streamer {
                    match s.process(&samples) {
                        Ok(events) => {
                            let mut state = audio_state_for_worker.write();
                            state.is_speaking = s.is_speaking();
                            for event in events {
                                match event {
                                    StreamEvent::Partial(text) => {
                                        state.set_partial(text);
                                    }
                                    StreamEvent::Commit(text) => {
                                        state.set_partial(text.clone());
                                        state.commit();

                                        if !state.is_paused && state.injection_enabled {
                                            let _ = injection_tx_for_worker.send(text);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Streaming transcription error: {:#}", e);
                        }
                    }
                }
            }
        });
    }

    if args.headless || args.ansi {
        run_terminal_loop(
            audio_state,
            running,
            &args,
            &config,
            capture_control.as_ref(),
            model_swap_tx,
            download_cancel,
            download_cmd_tx_for_tui,
        )?;
    } else {
        let style = args.style;
        let mode = match style {
            SpectrogramStyle::Bars => SpectrogramMode::BarMeter,
            SpectrogramStyle::Waterfall => SpectrogramMode::Waterfall,
        };
        let color_name = match args.color {
            ColorSchemeName::Flame => "flame",
            ColorSchemeName::Ice => "ice",
            ColorSchemeName::Mono => "mono",
        };
        let mut overlay_panel = build_initial_control_panel(
            &config,
            mode,
            color_name,
            args.opacity.clamp(0.0, 1.0),
            startup_sources.clone(),
            audio_state.read().selected_source_id,
        );
        if args.demo_open_panel {
            overlay_panel.open_for_surface(true);
        }

        let overlay_options = ui::app::OverlayRunOptions {
            control_panel: overlay_panel,
            config: ui::app::OverlayConfigContext {
                path: Config::config_path(),
                source_override: persisted_source_base.clone(),
                model_dir: args.model_dir.clone().or(config.model_dir.clone()),
            },
            model_command: Arc::new(move |command| match command {
                ui::app::OverlayModelCommand::Request(model) => {
                    let _ = model_swap_tx_for_overlay.send(model);
                    let _ = download_cmd_tx_for_overlay.send(DownloadCommand::Request(model));
                }
                ui::app::OverlayModelCommand::Cancel(model) => {
                    download_cancel_for_overlay.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = download_cmd_tx_for_overlay.send(DownloadCommand::Cancel(model));
                }
            }),
        };

        if args.wgpu_legacy {
            ui::app::run(
                audio_state,
                running,
                capture_control,
                overlay_options,
                args.tag.clone(),
            );
        } else {
            ui::run(
                audio_state,
                running,
                capture_control,
                overlay_options,
                args.tag.clone(),
            );
        }
    }

    // Graceful shutdown cascade:
    // 1. Shutdown download manager
    // 2. Stop audio capture → closes its tx clone
    // 3. Drop audio_tx → worker's recv() returns Err → worker exits → drops injection_tx clone
    // 4. Drop injection_tx → injector's recv() returns Err → injector exits (IME cleanup runs)
    // 5. Join injector to ensure cleanup completes before process exit
    log::debug!("Shutting down download manager...");
    let _ = download_cmd_tx.send(DownloadCommand::Shutdown);
    if download_manager_handle.join().is_err() {
        log::warn!("Download manager thread panicked");
    }

    drop(_capture_handle);
    drop(audio_tx);
    drop(injection_tx);
    log::debug!("Waiting for injector thread...");
    if injector_handle.join().is_err() {
        log::warn!("Injector thread panicked");
    }

    Ok(())
}

fn run_demo_audio(
    audio_state: ui::SharedAudioState,
    injection_tx: std::sync::mpsc::Sender<String>,
) {
    std::thread::spawn(move || {
        let mut t = 0.0f32;

        loop {
            std::thread::sleep(Duration::from_millis(16));

            let mut samples = vec![0.0f32; 512];
            for (i, s) in samples.iter_mut().enumerate() {
                let freq = 2.0 + (t * 0.5).sin() * 1.5;
                let amp = 0.3 + (t * 0.2).sin().abs() * 0.5;
                *s = (i as f32 * freq * 0.1 + t * 10.0).sin() * amp;
            }

            {
                let mut state = audio_state.write();
                state.update_samples(&samples);
            }

            t += 0.016;

            if (2.0..2.1).contains(&t) {
                audio_state.write().set_partial("Listening...".to_string());
            }
            if (4.0..4.1).contains(&t) {
                audio_state.write().set_partial("Hello world".to_string());
            }
            if (5.0..5.1).contains(&t) {
                let state = audio_state.read();
                if !state.is_paused && state.injection_enabled {
                    let _ = injection_tx.send("Hello world".to_string());
                }
                drop(state);
                audio_state.write().commit();
            }
            if (6.0..6.1).contains(&t) {
                audio_state
                    .write()
                    .set_partial("this is streaming".to_string());
            }
            if (7.0..7.1).contains(&t) {
                let state = audio_state.read();
                if !state.is_paused && state.injection_enabled {
                    let _ = injection_tx.send("this is streaming".to_string());
                }
                drop(state);
                audio_state.write().commit();
                audio_state.write().set_partial("transcription".to_string());
            }
        }
    });
}

fn run_sweep_audio(audio_state: ui::SharedAudioState) {
    std::thread::spawn(move || {
        let sample_rate = 16000.0f32;
        let mut t = 0.0f32;
        let mut phase = 0.0f32;

        loop {
            std::thread::sleep(Duration::from_millis(16));

            let mut samples = vec![0.0f32; 512];
            let sweep_duration = 6.0f32;
            let progress = (t / sweep_duration) % 1.0;
            let start_freq = 60.0f32;
            let end_freq = 7000.0f32;
            let freq = start_freq * (end_freq / start_freq).powf(progress);
            let phase_step = 2.0 * std::f32::consts::PI * freq / sample_rate;

            for sample in &mut samples {
                phase += phase_step;
                if phase > 2.0 * std::f32::consts::PI {
                    phase -= 2.0 * std::f32::consts::PI;
                }
                *sample = phase.sin() * 0.7;
            }

            {
                let mut state = audio_state.write();
                state.update_samples(&samples);
            }

            t += 0.016;
        }
    });
}

fn apply_demo_overlay_state(
    audio_state: &ui::SharedAudioState,
    demo_overlay_state: DemoOverlayState,
) {
    use DemoOverlayState::{Default, Display, Downloading, Error, Transcribe, Trusted};

    let mut state = audio_state.write();
    match demo_overlay_state {
        Default => {}
        Display => {
            state.transcription_available = false;
            state.injection_enabled = false;
            state.requested_model = None;
            state.active_model = None;
            state.download_progress = None;
            state.model_error = None;
        }
        Transcribe => {
            state.transcription_available = true;
            state.injection_enabled = false;
            state.requested_model = Some(AsrModelId::MoonshineBase);
            state.active_model = Some(AsrModelId::MoonshineBase);
            state.download_progress = None;
            state.model_error = None;
        }
        Trusted => {
            state.transcription_available = true;
            state.injection_enabled = true;
            state.requested_model = Some(AsrModelId::MoonshineBase);
            state.active_model = Some(AsrModelId::MoonshineBase);
            state.download_progress = None;
            state.model_error = None;
        }
        Downloading => {
            state.transcription_available = true;
            state.injection_enabled = true;
            state.requested_model = Some(AsrModelId::MoonshineTiny);
            state.active_model = Some(AsrModelId::MoonshineBase);
            state.download_progress = Some(0.42);
            state.model_error = None;
        }
        Error => {
            state.transcription_available = false;
            state.injection_enabled = false;
            state.requested_model = Some(AsrModelId::MoonshineTiny);
            state.active_model = None;
            state.download_progress = None;
            state.model_error = Some("Synthetic helper-state error".to_string());
        }
    }
}

/// Flush batched resize events, returning the final size
fn flush_resize_events(first: (u16, u16)) -> (u16, u16) {
    use crossterm::event::{poll, read, Event};
    use std::time::Duration;

    let mut last = first;
    while let Ok(true) = poll(Duration::from_millis(50)) {
        if let Ok(Event::Resize(x, y)) = read() {
            last = (x, y);
        } else {
            break; // Non-resize event, stop flushing
        }
    }
    last
}

fn build_config_from_state(
    panel: &ui::control_panel::ControlPanelState,
    audio_state: &ui::SharedAudioState,
    source_override: Option<String>,
    model_dir: Option<PathBuf>,
) -> Config {
    ui::build_runtime_config(panel, audio_state, source_override, model_dir)
}

fn seed_audio_source_state(
    audio_state: &ui::SharedAudioState,
    configured_source: Option<&str>,
) -> Vec<ui::AudioSourceInfo> {
    let discovered_sources = match audio::list_audio_sources() {
        Ok(sources) => sources,
        Err(error) => {
            log::warn!("Failed to enumerate audio sources for UI state: {}", error);
            Vec::new()
        }
    };

    let available_sources = discovered_sources
        .iter()
        .map(|source| ui::AudioSourceInfo {
            id: source.id,
            name: source.name.clone(),
            description: source.description.clone(),
        })
        .collect::<Vec<_>>();

    let selected_source = configured_source.and_then(|requested| {
        if let Ok(id) = requested.parse::<u32>() {
            available_sources.iter().find(|source| source.id == id)
        } else {
            available_sources
                .iter()
                .find(|source| source.name == requested || source.description == requested)
        }
    });

    {
        let mut state = audio_state.write();
        state.available_sources = available_sources.clone();
        state.selected_source_id = selected_source.map(|source| source.id);
        state.selected_source_name = selected_source
            .map(|source| source.name.clone())
            .or_else(|| configured_source.map(str::to_string));
        state.session_source_name = state.selected_source_name.clone();
        state.source_change_pending_restart = false;
    }

    available_sources
}

fn build_initial_control_panel(
    config: &Config,
    mode: SpectrogramMode,
    color_name: &'static str,
    opacity: f32,
    available_sources: Vec<ui::AudioSourceInfo>,
    selected_source_id: Option<u32>,
) -> ui::control_panel::ControlPanelState {
    let mut control_panel = ui::control_panel::ControlPanelState::new();
    control_panel.color_scheme_name = color_name;
    control_panel.viz_mode = mode;
    control_panel.gain_value = config.gain;
    control_panel.auto_save = config.auto_save;
    control_panel.model = config.model;
    control_panel.agc_enabled = config.auto_gain;
    control_panel.opacity = opacity.clamp(0.0, 1.0);
    control_panel.update_device_list(available_sources);
    control_panel.set_device(selected_source_id);
    control_panel
}

fn maybe_auto_save(
    panel: &ui::control_panel::ControlPanelState,
    audio_state: &ui::SharedAudioState,
    config_path: &Path,
    source_override: Option<String>,
    model_dir: Option<PathBuf>,
    last_save_time: &mut Option<Instant>,
) {
    if !panel.auto_save {
        return;
    }

    let now = Instant::now();
    if let Some(last) = last_save_time {
        if now.duration_since(*last) < Duration::from_millis(500) {
            return;
        }
    }

    let config = build_config_from_state(panel, audio_state, source_override, model_dir);
    if let Err(e) = config.save(config_path) {
        log::warn!("Auto-save failed: {}", e);
    }
    *last_save_time = Some(now);
}

fn save_on_exit(
    panel: &ui::control_panel::ControlPanelState,
    audio_state: &ui::SharedAudioState,
    config_path: &Path,
    source_override: Option<String>,
    model_dir: Option<PathBuf>,
) {
    if !panel.auto_save {
        return;
    }
    let config = build_config_from_state(panel, audio_state, source_override, model_dir);
    if let Err(e) = config.save(config_path) {
        log::warn!("Save on exit failed: {}", e);
    }
}

#[allow(clippy::too_many_arguments)]
fn run_terminal_loop(
    audio_state: ui::SharedAudioState,
    running: Arc<AtomicBool>,
    args: &Args,
    config: &Config,
    capture_control: Option<&Arc<CaptureControl>>,
    model_swap_tx: std::sync::mpsc::Sender<AsrModelId>,
    download_cancel: Arc<std::sync::atomic::AtomicBool>,
    download_cmd_tx: std::sync::mpsc::Sender<DownloadCommand>,
) -> anyhow::Result<()> {
    if !args.ansi {
        return run_headless_text(audio_state, running);
    }

    let (term_width, term_height) = terminal_size()
        .map(|(Width(w), Height(h))| (w as usize, h as usize))
        .unwrap_or((80, 24));

    let (width, height) = clamp_terminal_surface_dimensions(
        args.ansi_width,
        args.ansi_height,
        term_width,
        term_height,
    );

    let use_unicode = match args.ansi_charset {
        AnsiCharset::Ascii => false,
        AnsiCharset::Blocks => true,
        AnsiCharset::Auto => supports_unicode::on(Stream::Stdout),
    };

    let mode = match args.style {
        SpectrogramStyle::Bars => TerminalMode::BarMeter,
        SpectrogramStyle::Waterfall => TerminalMode::Waterfall,
    };

    let terminal_config = TerminalConfig {
        width,
        height,
        requested_width: args.ansi_width,
        requested_height: args.ansi_height,
        mode,
        use_color: !args.no_color,
        use_unicode,
        term_width,
        term_height,
    };

    let mut visualizer = TerminalVisualizer::new(terminal_config, args.tag.clone());

    let color_name = match args.color {
        ColorSchemeName::Flame => "flame",
        ColorSchemeName::Ice => "ice",
        ColorSchemeName::Mono => "mono",
    };
    visualizer.set_color_scheme(get_color_scheme(color_name));

    let selected_source_id = audio_state.read().selected_source_id;
    let mut control_panel = build_initial_control_panel(
        config,
        match mode {
            TerminalMode::BarMeter => ui::spectrogram::SpectrogramMode::BarMeter,
            TerminalMode::Waterfall => ui::spectrogram::SpectrogramMode::Waterfall,
        },
        color_name,
        args.opacity,
        audio_state.read().available_sources.clone(),
        selected_source_id,
    );
    if args.demo_open_panel {
        control_panel.open_for_surface(false);
        visualizer.set_panel_open(true);
    }

    let config_path = Config::config_path();
    let persisted_source = config.source.clone();
    let persisted_model_dir = args.model_dir.clone().or(config.model_dir.clone());
    let mut last_save_time: Option<Instant> = None;

    terminal::enable_raw_mode()?;
    execute!(std::io::stdout(), EnableMouseCapture)?;
    visualizer.init_terminal()?;

    let result = (|| -> anyhow::Result<()> {
        while running.load(Ordering::Relaxed) {
            if event::poll(Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            if control_panel.is_open {
                                match key.code {
                                    KeyCode::Char('q') => {
                                        save_on_exit(
                                            &control_panel,
                                            &audio_state,
                                            &config_path,
                                            persisted_source.clone(),
                                            persisted_model_dir.clone(),
                                        );
                                        break;
                                    }
                                    KeyCode::Char(' ') => {
                                        control_panel.toggle_pause();
                                        if let Some(ctrl) = capture_control {
                                            control_panel.apply_pause(ctrl);
                                        }
                                    }
                                    KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => {
                                        control_panel.close();
                                        visualizer.set_panel_open(control_panel.is_open);
                                    }
                                    KeyCode::Up => {
                                        control_panel.focus_previous(false);
                                    }
                                    KeyCode::Down => {
                                        control_panel.focus_next(false);
                                    }
                                    KeyCode::Enter => match control_panel.focused_control {
                                        Some(ui::control_panel::Control::DeviceSelector) => {
                                            let mut state = audio_state.write();
                                            control_panel.cycle_device(&mut state);
                                        }
                                        Some(ui::control_panel::Control::AgcCheckbox) => {
                                            control_panel.toggle_agc();
                                            let mut state = audio_state.write();
                                            control_panel.apply_agc(&mut state);
                                        }
                                        Some(ui::control_panel::Control::InjectionToggle) => {
                                            let mut state = audio_state.write();
                                            control_panel.toggle_injection(&mut state);
                                        }
                                        Some(ui::control_panel::Control::PauseButton) => {
                                            control_panel.toggle_pause();
                                            if let Some(ctrl) = capture_control {
                                                control_panel.apply_pause(ctrl);
                                            }
                                        }
                                        Some(ui::control_panel::Control::VizToggle) => {
                                            control_panel.toggle_viz_mode();
                                            visualizer.toggle_mode();
                                        }
                                        Some(ui::control_panel::Control::ColorPicker) => {
                                            let next_scheme = match control_panel.color_scheme_name
                                            {
                                                "flame" => "ice",
                                                "ice" => "mono",
                                                _ => "flame",
                                            };
                                            control_panel.set_color_scheme(next_scheme);
                                            visualizer
                                                .set_color_scheme(get_color_scheme(next_scheme));
                                        }
                                        Some(ui::control_panel::Control::GainSlider) => {
                                            let new_gain = if control_panel.gain_value >= 2.0 {
                                                0.5
                                            } else {
                                                (control_panel.gain_value + 0.5).min(2.0)
                                            };
                                            control_panel.set_gain(new_gain);
                                            let mut state = audio_state.write();
                                            control_panel.apply_gain(&mut state);
                                        }
                                        Some(ui::control_panel::Control::ModelSelector) => {
                                            let is_downloading =
                                                audio_state.read().download_progress.is_some();
                                            if is_downloading {
                                                // Cancel current download via both mechanisms
                                                download_cancel.store(
                                                    true,
                                                    std::sync::atomic::Ordering::Relaxed,
                                                );
                                                let _ = download_cmd_tx.send(
                                                    DownloadCommand::Cancel(control_panel.model),
                                                );
                                            } else {
                                                control_panel.toggle_model();
                                                // Send to streaming worker for immediate swap if cached
                                                let _ = model_swap_tx.send(control_panel.model);
                                                // Also request download via download manager (handles caching check)
                                                let _ = download_cmd_tx.send(
                                                    DownloadCommand::Request(control_panel.model),
                                                );
                                            }
                                        }
                                        Some(ui::control_panel::Control::AutoSaveToggle) => {
                                            control_panel.toggle_auto_save();
                                        }
                                        Some(ui::control_panel::Control::QuitButton) => {
                                            save_on_exit(
                                                &control_panel,
                                                &audio_state,
                                                &config_path,
                                                persisted_source.clone(),
                                                persisted_model_dir.clone(),
                                            );
                                            break;
                                        }
                                        _ => {}
                                    },
                                    _ => {}
                                }
                                maybe_auto_save(
                                    &control_panel,
                                    &audio_state,
                                    &config_path,
                                    persisted_source.clone(),
                                    persisted_model_dir.clone(),
                                    &mut last_save_time,
                                );
                            } else {
                                match key.code {
                                    KeyCode::Char('q') => {
                                        save_on_exit(
                                            &control_panel,
                                            &audio_state,
                                            &config_path,
                                            persisted_source.clone(),
                                            persisted_model_dir.clone(),
                                        );
                                        break;
                                    }
                                    KeyCode::Char(' ') => {
                                        control_panel.toggle_pause();
                                        if let Some(ctrl) = capture_control {
                                            control_panel.apply_pause(ctrl);
                                        }
                                    }
                                    KeyCode::Char('w') | KeyCode::Char('W') => {
                                        visualizer.toggle_mode();
                                        control_panel.toggle_viz_mode();
                                    }
                                    KeyCode::Char('c') | KeyCode::Char('C') => {
                                        if visualizer.layout_mode()
                                            != ui::terminal::LayoutMode::Degenerate
                                        {
                                            control_panel.toggle_open_for_surface(false);
                                            visualizer.set_panel_open(control_panel.is_open);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    Event::Resize(width, height) => {
                        let (final_w, final_h) = flush_resize_events((width, height));
                        visualizer.resize(final_w, final_h);
                    }
                    Event::Mouse(mouse) => {
                        let action = match mouse.kind {
                            MouseEventKind::Down(MouseButton::Left) => visualizer
                                .mouse_action_for_panel(
                                    &control_panel,
                                    mouse.column,
                                    mouse.row,
                                    false,
                                    false,
                                ),
                            MouseEventKind::ScrollUp => visualizer.mouse_action_for_panel(
                                &control_panel,
                                mouse.column,
                                mouse.row,
                                true,
                                false,
                            ),
                            MouseEventKind::ScrollDown => visualizer.mouse_action_for_panel(
                                &control_panel,
                                mouse.column,
                                mouse.row,
                                false,
                                true,
                            ),
                            _ => ui::terminal::TerminalMouseAction::None,
                        };

                        match action {
                            ui::terminal::TerminalMouseAction::None => {}
                            ui::terminal::TerminalMouseAction::ClosePanel => {
                                control_panel.close();
                                visualizer.set_panel_open(control_panel.is_open);
                            }
                            ui::terminal::TerminalMouseAction::FocusPrevious => {
                                control_panel.focus_previous(false);
                            }
                            ui::terminal::TerminalMouseAction::FocusNext => {
                                control_panel.focus_next(false);
                            }
                            ui::terminal::TerminalMouseAction::Activate(control) => {
                                control_panel.set_focused(Some(control));
                                match control {
                                    ui::control_panel::Control::DeviceSelector => {
                                        let mut state = audio_state.write();
                                        control_panel.cycle_device(&mut state);
                                    }
                                    ui::control_panel::Control::AgcCheckbox => {
                                        control_panel.toggle_agc();
                                        let mut state = audio_state.write();
                                        control_panel.apply_agc(&mut state);
                                    }
                                    ui::control_panel::Control::InjectionToggle => {
                                        let mut state = audio_state.write();
                                        control_panel.toggle_injection(&mut state);
                                    }
                                    ui::control_panel::Control::PauseButton => {
                                        control_panel.toggle_pause();
                                        if let Some(ctrl) = capture_control {
                                            control_panel.apply_pause(ctrl);
                                        }
                                    }
                                    ui::control_panel::Control::VizToggle => {
                                        control_panel.toggle_viz_mode();
                                        visualizer.toggle_mode();
                                    }
                                    ui::control_panel::Control::ColorPicker => {
                                        let next_scheme = match control_panel.color_scheme_name {
                                            "flame" => "ice",
                                            "ice" => "mono",
                                            _ => "flame",
                                        };
                                        control_panel.set_color_scheme(next_scheme);
                                        visualizer.set_color_scheme(get_color_scheme(next_scheme));
                                    }
                                    ui::control_panel::Control::GainSlider => {
                                        let new_gain = if control_panel.gain_value >= 2.0 {
                                            0.5
                                        } else {
                                            (control_panel.gain_value + 0.5).min(2.0)
                                        };
                                        control_panel.set_gain(new_gain);
                                        let mut state = audio_state.write();
                                        control_panel.apply_gain(&mut state);
                                    }
                                    ui::control_panel::Control::ModelSelector => {
                                        let is_downloading =
                                            audio_state.read().download_progress.is_some();
                                        if is_downloading {
                                            download_cancel
                                                .store(true, std::sync::atomic::Ordering::Relaxed);
                                            let _ = download_cmd_tx
                                                .send(DownloadCommand::Cancel(control_panel.model));
                                        } else {
                                            control_panel.toggle_model();
                                            let _ = model_swap_tx.send(control_panel.model);
                                            let _ = download_cmd_tx.send(DownloadCommand::Request(
                                                control_panel.model,
                                            ));
                                        }
                                    }
                                    ui::control_panel::Control::AutoSaveToggle => {
                                        control_panel.toggle_auto_save();
                                    }
                                    ui::control_panel::Control::QuitButton => {
                                        save_on_exit(
                                            &control_panel,
                                            &audio_state,
                                            &config_path,
                                            persisted_source.clone(),
                                            persisted_model_dir.clone(),
                                        );
                                        break;
                                    }
                                    ui::control_panel::Control::OpacitySlider => {}
                                }

                                maybe_auto_save(
                                    &control_panel,
                                    &audio_state,
                                    &config_path,
                                    persisted_source.clone(),
                                    persisted_model_dir.clone(),
                                    &mut last_save_time,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }

            let (
                samples,
                committed,
                partial,
                is_speaking,
                injection_enabled,
                transcription_available,
                selected_source_name,
                source_change_pending_restart,
                download_progress,
                model_error,
                requested_model,
                active_model,
            ) = {
                let state = audio_state.read();
                (
                    state.samples.clone(),
                    state.committed.clone(),
                    state.partial.clone(),
                    state.is_speaking,
                    state.injection_enabled,
                    state.transcription_available,
                    state.selected_source_name.clone(),
                    state.source_change_pending_restart,
                    state.download_progress,
                    state.model_error.clone(),
                    state.requested_model,
                    state.active_model,
                )
            };

            if samples.is_empty() {
                continue;
            }

            visualizer.set_transcript(committed, partial);
            visualizer.set_download_progress(download_progress);
            visualizer.set_model_error(model_error);

            let status_info = match capture_control {
                Some(ctrl) => {
                    let rate = ctrl.sample_rate();
                    let ch = ctrl.channels();
                    if rate > 0 {
                        ui::terminal::StatusInfo::Live {
                            sample_rate: rate,
                            channels: ch as u16,
                        }
                    } else {
                        ui::terminal::StatusInfo::Demo
                    }
                }
                None => ui::terminal::StatusInfo::Demo,
            };
            visualizer.set_status_info(status_info);

            visualizer.push_samples(&samples);

            visualizer.set_paused(control_panel.is_paused);
            visualizer.set_speaking(is_speaking);
            visualizer.set_injection_enabled(injection_enabled);
            visualizer.set_transcription_available(transcription_available);
            visualizer.set_source_status(selected_source_name, source_change_pending_restart);
            visualizer.set_model_status(requested_model, active_model);

            // Render visualization (unified ratatui draw loop)
            visualizer.process_and_render_ratatui(&control_panel)?;
        }
        Ok(())
    })();

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    TerminalVisualizer::cleanup_terminal()?;
    terminal::disable_raw_mode()?;

    result
}

fn run_headless_text(
    audio_state: ui::SharedAudioState,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    println!("Headless mode - press Ctrl+C to exit");
    println!("Commands: [p]ause, [g]ain toggle, [q]uit");

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
        let state = audio_state.read();
        if !state.display().is_empty() {
            println!("Transcript: {}", state.display());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_backend_names_recognizes_input_method() {
        let raw = vec!["input_method".to_string()];
        let normalized = normalize_backend_names(&raw);
        assert_eq!(normalized, vec!["input_method"]);
    }

    #[test]
    fn test_normalize_backend_names_rejects_unknown() {
        let raw = vec!["unknown_backend".to_string()];
        let normalized = normalize_backend_names(&raw);
        assert!(normalized.is_empty());
    }

    #[test]
    fn test_normalize_backend_names_all_known_backends() {
        // Verify all known backends are recognized
        let raw = vec![
            "input_method".to_string(),
            "wrtype".to_string(),
            "ydotool".to_string(),
        ];
        let normalized = normalize_backend_names(&raw);
        assert_eq!(normalized.len(), 3);
        assert!(normalized.contains(&"input_method".to_string()));
        assert!(normalized.contains(&"wrtype".to_string()));
        assert!(normalized.contains(&"ydotool".to_string()));
    }

    #[test]
    fn test_build_config_from_state_uses_runtime_panel_opacity() {
        let mut panel = ui::control_panel::ControlPanelState::new();
        panel.opacity = 0.61;
        panel.model = AsrModelId::MoonshineTiny;
        panel.agc_enabled = true;
        panel.gain_value = 1.25;

        let audio_state = ui::new_shared_state();
        audio_state.write().injection_enabled = false;

        let args = Args {
            headless: false,
            demo: false,
            demo_overlay_state: DemoOverlayState::Default,
            demo_open_panel: false,
            auto_gain: false,
            list_sources: false,
            ansi: false,
            wgpu_legacy: false,
            ansi_width: None,
            ansi_height: None,
            ansi_charset: AnsiCharset::Auto,
            ansi_sweep: false,
            style: SpectrogramStyle::Bars,
            color: ColorSchemeName::Flame,
            model: AsrModelId::MoonshineBase,
            no_color: false,
            source: None,
            model_dir: None,
            opacity: 0.2,
            test_fireworks: false,
            backend_disable: Vec::new(),
            autostart_ydotoold: false,
            tag: None,
            no_duplicate_tag: false,
            list_instances: false,
            human: false,
        };

        let config = build_config_from_state(
            &panel,
            &audio_state,
            args.source.clone(),
            args.model_dir.clone(),
        );
        assert_eq!(config.model, AsrModelId::MoonshineTiny);
        assert_eq!(config.opacity, 0.61);
        assert!(config.auto_gain);
        assert!(!config.injection_enabled);
    }
}
