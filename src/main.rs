use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal;
use supports_unicode::Stream;
use terminal_size::{terminal_size, Height, Width};

use usit::audio::{self, AudioCapture, CaptureConfig, CaptureControl};
use usit::config::{AsrModelId, Config};
use usit::instance::{find_duplicate_tag, find_instances};
use usit::spectrum::get_color_scheme;
use usit::ui;
use usit::ui::spectrogram::SpectrogramMode;
use usit::ui::terminal::{TerminalConfig, TerminalMode, TerminalVisualizer};
use usit::{backend, download, streaming};

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

    #[arg(long, help = "Enable automatic gain control")]
    auto_gain: bool,

    #[arg(long, help = "List available audio sources and exit")]
    list_sources: bool,

    #[arg(long, help = "Terminal UI mode (instead of graphical overlay)")]
    ansi: bool,

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
        help = "ASR model: moonshine-base, moonshine-tiny, parakeet-tdt-0.6b-v3 [default: moonshine-base]"
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

        let config = TerminalConfig {
            width: *width as usize,
            height: *height as usize,
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
            println!("{:<10} {}", "PID", "TAG");
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

    let streaming_transcriber: Option<streaming::DefaultStreamingTranscriber> =
        if args.demo || args.ansi_sweep {
            None
        } else {
            use audio::{SileroVad, VadConfig};
            use backend::{MoonshineStreamer, NemoTransducerStreamer};
            use streaming::{BoxedTranscriber, StreamingConfig, StreamingTranscriber};

            log::info!("Loading models from {:?}...", model_dir);
            let resolved_model = if args.model != AsrModelId::default() {
                args.model
            } else {
                config.model
            };
            let model_paths = download::ensure_models_exist(&model_dir, resolved_model)?;

            backend::init_ort();

            log::info!("Initializing VAD...");
            let vad = SileroVad::new(&model_paths.silero_vad, VadConfig::default())?;

            let transcriber: BoxedTranscriber = match resolved_model {
                AsrModelId::MoonshineBase | AsrModelId::MoonshineTiny => {
                    log::info!("Initializing Moonshine transcriber...");
                    Box::new(MoonshineStreamer::new(&model_paths.asr_dir)?)
                }
                AsrModelId::ParakeetTdt06bV3 => {
                    log::info!("Initializing NeMo transducer transcriber...");
                    Box::new(NemoTransducerStreamer::new(&model_paths.asr_dir)?)
                }
            };

            log::info!("Creating streaming coordinator...");
            Some(StreamingTranscriber::new(
                vad,
                transcriber,
                StreamingConfig::default(),
            ))
        };

    let (audio_tx, audio_rx): (
        std::sync::mpsc::SyncSender<Vec<f32>>,
        std::sync::mpsc::Receiver<Vec<f32>>,
    ) = std::sync::mpsc::sync_channel(100);

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
    let download_cancel_for_worker = download_cancel.clone();

    // Extract before spawn (args used after spawn, can't move)
    let backend_disable = args.backend_disable.clone();
    let autostart_ydotoold = args.autostart_ydotoold;
    let is_tui = args.headless || args.ansi || args.ansi_sweep;

    // Spawn injector thread - handle stored so we can join on shutdown
    // to ensure proper Wayland IME cleanup (Drop runs before process exit)
    let injector_handle = std::thread::spawn(move || {
        use usit::input::{find_ydotool_socket, select_backend, TextInjector};

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
        let source = args.source.clone().or(config.source.clone());
        let capture_config = CaptureConfig {
            auto_gain_enabled: auto_gain,
            agc: Default::default(),
            source,
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

    if let Some(mut streamer) = streaming_transcriber {
        use streaming::StreamEvent;
        let audio_state_for_worker = audio_state.clone();
        let injection_tx_for_worker = injection_tx.clone();
        let model_dir_for_worker = model_dir.clone();

        std::thread::spawn(move || {
            while let Ok(samples) = audio_rx.recv() {
                // Check for model swap command (non-blocking, between audio chunks)
                if let Ok(new_variant) = model_swap_rx.try_recv() {
                    let progress_state = audio_state_for_worker.clone();
                    download_cancel_for_worker.store(false, std::sync::atomic::Ordering::Relaxed);
                    let progress_callback: Box<dyn Fn(f64) + Send + Sync> =
                        Box::new(move |progress: f64| {
                            progress_state.write().download_progress = Some(progress as f32);
                        });
                    match download::ensure_models_exist_with_progress(
                        &model_dir_for_worker,
                        new_variant,
                        Some(progress_callback),
                        Some(&download_cancel_for_worker),
                    ) {
                        Ok(model_paths) => {
                            audio_state_for_worker.write().download_progress = None;
                            let new_transcriber = match new_variant {
                                AsrModelId::MoonshineBase | AsrModelId::MoonshineTiny => {
                                    backend::MoonshineStreamer::new(&model_paths.asr_dir)
                                        .map(|t| Box::new(t) as streaming::BoxedTranscriber)
                                }
                                AsrModelId::ParakeetTdt06bV3 => {
                                    backend::NemoTransducerStreamer::new(&model_paths.asr_dir)
                                        .map(|t| Box::new(t) as streaming::BoxedTranscriber)
                                }
                            };

                            match new_transcriber {
                                Ok(new_transcriber) => {
                                    streamer.swap_transcriber(new_transcriber);
                                    log::info!("Model swapped to {}", new_variant);
                                }
                                Err(e) => {
                                    log::error!("Failed to swap model: {}", e);
                                }
                            };
                        }
                        Err(e) => {
                            audio_state_for_worker.write().download_progress = None;
                            let msg = e.to_string();
                            if !msg.contains("cancelled") {
                                log::error!("Failed to swap model: {}", e);
                            }
                        }
                    }
                }

                match streamer.process(&samples) {
                    Ok(events) => {
                        let mut state = audio_state_for_worker.write();
                        state.is_speaking = streamer.is_speaking();
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
                        log::error!("Streaming transcription error: {}", e);
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
        )?;
    } else {
        let style = args.style;
        let mode = match style {
            SpectrogramStyle::Bars => SpectrogramMode::BarMeter,
            SpectrogramStyle::Waterfall => SpectrogramMode::Waterfall,
        };
        let opacity = args.opacity.clamp(0.0, 1.0);
        ui::run(
            audio_state,
            running,
            capture_control,
            mode,
            opacity,
            args.tag.clone(),
        );
    }

    // Graceful shutdown cascade:
    // 1. Stop audio capture → closes its tx clone
    // 2. Drop audio_tx → worker's recv() returns Err → worker exits → drops injection_tx clone
    // 3. Drop injection_tx → injector's recv() returns Err → injector exits (IME cleanup runs)
    // 4. Join injector to ensure cleanup completes before process exit
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
    args: &Args,
    audio_state: &ui::SharedAudioState,
) -> Config {
    let state = audio_state.read();
    Config {
        model: panel.model,
        auto_gain: panel.agc_enabled,
        gain: panel.gain_value,
        style: match panel.viz_mode {
            SpectrogramMode::BarMeter => "bars".to_string(),
            SpectrogramMode::Waterfall => "waterfall".to_string(),
        },
        color: panel.color_scheme_name.to_string(),
        source: args.source.clone(),
        injection_enabled: state.injection_enabled,
        auto_save: panel.auto_save,
        model_dir: args.model_dir.clone(),
        opacity: args.opacity.clamp(0.0, 1.0),
    }
}

fn maybe_auto_save(
    panel: &ui::control_panel::ControlPanelState,
    args: &Args,
    audio_state: &ui::SharedAudioState,
    config_path: &Path,
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

    let config = build_config_from_state(panel, args, audio_state);
    if let Err(e) = config.save(config_path) {
        log::warn!("Auto-save failed: {}", e);
    }
    *last_save_time = Some(now);
}

fn save_on_exit(
    panel: &ui::control_panel::ControlPanelState,
    args: &Args,
    audio_state: &ui::SharedAudioState,
    config_path: &Path,
) {
    if !panel.auto_save {
        return;
    }
    let config = build_config_from_state(panel, args, audio_state);
    if let Err(e) = config.save(config_path) {
        log::warn!("Save on exit failed: {}", e);
    }
}

fn run_terminal_loop(
    audio_state: ui::SharedAudioState,
    running: Arc<AtomicBool>,
    args: &Args,
    config: &Config,
    capture_control: Option<&Arc<CaptureControl>>,
    model_swap_tx: std::sync::mpsc::Sender<AsrModelId>,
    download_cancel: Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<()> {
    if !args.ansi {
        return run_headless_text(audio_state, running);
    }

    let (term_width, term_height) = terminal_size()
        .map(|(Width(w), Height(h))| (w as usize, h as usize))
        .unwrap_or((80, 24));

    let width = args
        .ansi_width
        .unwrap_or(((term_width as f32) * 0.6).round() as usize)
        .max(1)
        .min(term_width.saturating_sub(2));
    let height = args
        .ansi_height
        .unwrap_or(6)
        .max(1)
        .min(term_height.saturating_sub(4));

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

    let mut control_panel = ui::control_panel::ControlPanelState::new();
    control_panel.color_scheme_name = color_name;
    control_panel.viz_mode = match mode {
        TerminalMode::BarMeter => ui::spectrogram::SpectrogramMode::BarMeter,
        TerminalMode::Waterfall => ui::spectrogram::SpectrogramMode::Waterfall,
    };
    control_panel.gain_value = config.gain;
    control_panel.auto_save = config.auto_save;
    control_panel.model = config.model;
    control_panel.agc_enabled = config.auto_gain;

    let config_path = Config::config_path();
    let mut last_save_time: Option<Instant> = None;

    terminal::enable_raw_mode()?;
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
                                            args,
                                            &audio_state,
                                            &config_path,
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
                                        control_panel.toggle_open();
                                        visualizer.set_panel_open(control_panel.is_open);
                                    }
                                    KeyCode::Up => {
                                        let controls: Vec<_> = ui::control_panel::Control::ALL
                                            .iter()
                                            .filter(|&&c| !c.is_wgpu_only())
                                            .copied()
                                            .collect();
                                        let current_idx = control_panel
                                            .focused_control
                                            .and_then(|c| controls.iter().position(|&x| x == c))
                                            .unwrap_or(0);
                                        let new_idx = if current_idx == 0 {
                                            controls.len() - 1
                                        } else {
                                            current_idx - 1
                                        };
                                        control_panel.set_focused(Some(controls[new_idx]));
                                    }
                                    KeyCode::Down => {
                                        let controls: Vec<_> = ui::control_panel::Control::ALL
                                            .iter()
                                            .filter(|&&c| !c.is_wgpu_only())
                                            .copied()
                                            .collect();
                                        let current_idx = control_panel
                                            .focused_control
                                            .and_then(|c| controls.iter().position(|&x| x == c))
                                            .unwrap_or(0);
                                        let new_idx = (current_idx + 1) % controls.len();
                                        control_panel.set_focused(Some(controls[new_idx]));
                                    }
                                    KeyCode::Enter => match control_panel.focused_control {
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
                                                download_cancel.store(
                                                    true,
                                                    std::sync::atomic::Ordering::Relaxed,
                                                );
                                            } else {
                                                control_panel.toggle_model();
                                                let _ = model_swap_tx.send(control_panel.model);
                                            }
                                        }
                                        Some(ui::control_panel::Control::AutoSaveToggle) => {
                                            control_panel.toggle_auto_save();
                                        }
                                        Some(ui::control_panel::Control::QuitButton) => {
                                            save_on_exit(
                                                &control_panel,
                                                args,
                                                &audio_state,
                                                &config_path,
                                            );
                                            break;
                                        }
                                        _ => {}
                                    },
                                    _ => {}
                                }
                                maybe_auto_save(
                                    &control_panel,
                                    args,
                                    &audio_state,
                                    &config_path,
                                    &mut last_save_time,
                                );
                            } else {
                                match key.code {
                                    KeyCode::Char('q') => {
                                        save_on_exit(
                                            &control_panel,
                                            args,
                                            &audio_state,
                                            &config_path,
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
                                            control_panel.toggle_open();
                                            visualizer.set_panel_open(control_panel.is_open);
                                            if control_panel.is_open
                                                && control_panel.focused_control.is_none()
                                            {
                                                control_panel.set_focused(Some(
                                                    ui::control_panel::Control::DeviceSelector,
                                                ));
                                            }
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
                    _ => {}
                }
            }

            let (samples, committed, partial, is_speaking, injection_enabled, download_progress) = {
                let state = audio_state.read();
                (
                    state.samples.clone(),
                    state.committed.clone(),
                    state.partial.clone(),
                    state.is_speaking,
                    state.injection_enabled,
                    state.download_progress,
                )
            };

            if samples.is_empty() {
                continue;
            }

            visualizer.set_transcript(committed, partial);
            visualizer.set_download_progress(download_progress);

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

            // Render visualization (unified ratatui draw loop)
            visualizer.process_and_render_ratatui(&control_panel)?;
        }
        Ok(())
    })();

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
}
