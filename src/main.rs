use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal;
use supports_unicode::Stream;
use terminal_size::{terminal_size, Height, Width};

use barbara::audio::{self, AudioCapture, CaptureConfig, CaptureControl};
use barbara::spectrum::get_color_scheme;
use barbara::ui;
use barbara::ui::spectrogram::SpectrogramMode;
use barbara::ui::terminal::{TerminalConfig, TerminalMode, TerminalVisualizer};
use barbara::{backend, download, streaming};

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

#[derive(Parser)]
#[command(name = "barbara")]
#[command(about = "Streaming speech-to-text with live revision")]
#[command(long_about = "Streaming speech-to-text with live revision\n\n\
Keybindings:\n  \
Space - Pause/resume recording\n  \
w - Toggle between bar meter and waterfall visualization\n  \
c - Open control panel\n  \
q/Esc - Quit (GUI mode)\n  \
q - Quit (TUI mode)")]
struct Args {
    #[arg(long)]
    headless: bool,

    #[arg(long)]
    demo: bool,

    #[arg(long)]
    auto_gain: bool,

    #[arg(long)]
    list_sources: bool,

    #[arg(long)]
    ansi: bool,

    #[arg(long)]
    ansi_width: Option<usize>,

    #[arg(long)]
    ansi_height: Option<usize>,

    #[arg(long, default_value = "auto")]
    ansi_charset: String,

    #[arg(long)]
    ansi_sweep: bool,

    #[arg(long, value_enum, default_value = "bars")]
    style: SpectrogramStyle,

    #[arg(long, value_enum, default_value = "flame")]
    color: ColorSchemeName,

    #[arg(long)]
    no_color: bool,

    #[arg(long)]
    source: Option<String>,

    /// Path to model directory (default: ~/.cache/barbara/models)
    #[arg(long)]
    model_dir: Option<PathBuf>,

    /// Run visual test sequence in tmux (cycles through terminal sizes)
    #[arg(long)]
    test_fireworks: bool,
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

        let mut visualizer = TerminalVisualizer::new(config);
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

    if args.test_fireworks {
        return run_fireworks_test();
    }

    let model_dir = args.model_dir.clone().unwrap_or_else(|| {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("barbara/models")
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

    println!("barbara v{}", env!("CARGO_PKG_VERSION"));

    let running = Arc::new(AtomicBool::new(true));
    let audio_state = ui::new_shared_state();

    let streaming_transcriber: Option<streaming::DefaultStreamingTranscriber> =
        if args.demo || args.ansi_sweep {
            None
        } else {
            use audio::{SileroVad, VadConfig};
            use backend::MoonshineStreamer;
            use streaming::{StreamingConfig, StreamingTranscriber};

            println!("Loading models from {:?}...", model_dir);
            let model_paths = download::ensure_models_exist(&model_dir)?;

            println!("Initializing VAD...");
            let vad = SileroVad::new(&model_paths.silero_vad, VadConfig::default())?;

            println!("Initializing Moonshine transcriber...");
            let transcriber = MoonshineStreamer::new(&model_paths.moonshine_dir)?;

            println!("Creating streaming coordinator...");
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

    let capture_control: Option<Arc<CaptureControl>> = if args.demo || args.ansi_sweep {
        if args.ansi_sweep {
            run_sweep_audio(audio_state.clone());
        } else {
            run_demo_audio(audio_state.clone());
        }
        None
    } else {
        let config = CaptureConfig {
            auto_gain_enabled: args.auto_gain,
            agc: Default::default(),
            source: args.source.clone(),
        };

        let state = audio_state.clone();
        let tx = audio_tx.clone();
        let capture = AudioCapture::new(
            Box::new(move |samples| {
                state.write().update_samples(samples);
                if tx.try_send(samples.to_vec()).is_err() {}
            }),
            config,
        )?;

        let control = capture.control().clone();

        if args.auto_gain {
            audio_state.write().auto_gain_enabled = true;
        }

        std::mem::forget(capture);
        Some(control)
    };

    if let Some(mut streamer) = streaming_transcriber {
        use streaming::StreamEvent;
        let audio_state_for_worker = audio_state.clone();

        std::thread::spawn(move || {
            while let Ok(samples) = audio_rx.recv() {
                match streamer.process(&samples) {
                    Ok(events) => {
                        for event in events {
                            match event {
                                StreamEvent::Partial(text) => {
                                    audio_state_for_worker.write().set_partial(text);
                                }
                                StreamEvent::Commit(text) => {
                                    audio_state_for_worker.write().set_partial(text);
                                    audio_state_for_worker.write().commit();
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Streaming transcription error: {}", e);
                    }
                }
            }
        });
    }

    if args.headless || args.ansi {
        run_terminal_loop(audio_state, running, &args, capture_control.as_ref())?;
    } else {
        let mode = match args.style {
            SpectrogramStyle::Bars => SpectrogramMode::BarMeter,
            SpectrogramStyle::Waterfall => SpectrogramMode::Waterfall,
        };
        ui::run(audio_state, running, capture_control, mode);
    }

    Ok(())
}

fn run_demo_audio(audio_state: ui::SharedAudioState) {
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
                audio_state.write().commit();
            }
            if (6.0..6.1).contains(&t) {
                audio_state
                    .write()
                    .set_partial("this is streaming".to_string());
            }
            if (7.0..7.1).contains(&t) {
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

fn run_terminal_loop(
    audio_state: ui::SharedAudioState,
    running: Arc<AtomicBool>,
    args: &Args,
    capture_control: Option<&Arc<CaptureControl>>,
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

    let use_unicode = match args.ansi_charset.to_lowercase().as_str() {
        "ascii" => false,
        "blocks" | "unicode" => true,
        _ => supports_unicode::on(Stream::Stdout),
    };

    let mode = match args.style {
        SpectrogramStyle::Bars => TerminalMode::BarMeter,
        SpectrogramStyle::Waterfall => TerminalMode::Waterfall,
    };

    let config = TerminalConfig {
        width,
        height,
        mode,
        use_color: !args.no_color,
        use_unicode,
        term_width,
        term_height,
    };

    let mut visualizer = TerminalVisualizer::new(config);

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
                                    KeyCode::Char('q') => break,
                                    KeyCode::Char(' ') => {
                                        control_panel.toggle_pause();
                                        if let Some(ctrl) = capture_control {
                                            control_panel.apply_pause(ctrl);
                                        }
                                    }
                                    KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => {
                                        let was_open = control_panel.is_open;
                                        control_panel.toggle_open();
                                        visualizer.set_panel_open(control_panel.is_open);
                                        if was_open && !control_panel.is_open {
                                            visualizer.clear_panel_area();
                                        }
                                    }
                                    KeyCode::Up => {
                                        let controls = [
                                            ui::control_panel::Control::DeviceSelector,
                                            ui::control_panel::Control::GainSlider,
                                            ui::control_panel::Control::AgcCheckbox,
                                            ui::control_panel::Control::PauseButton,
                                            ui::control_panel::Control::VizToggle,
                                            ui::control_panel::Control::ColorPicker,
                                        ];
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
                                        let controls = [
                                            ui::control_panel::Control::DeviceSelector,
                                            ui::control_panel::Control::GainSlider,
                                            ui::control_panel::Control::AgcCheckbox,
                                            ui::control_panel::Control::PauseButton,
                                            ui::control_panel::Control::VizToggle,
                                            ui::control_panel::Control::ColorPicker,
                                        ];
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
                                        _ => {}
                                    },
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('q') => break,
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

            let (samples, committed, partial) = {
                let state = audio_state.read();
                (
                    state.samples.clone(),
                    state.committed.clone(),
                    state.partial.clone(),
                )
            };

            if samples.is_empty() {
                continue;
            }

            visualizer.set_transcript(committed, partial);

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

            // Set pause state for degenerate mode indicator
            visualizer.set_paused(control_panel.is_paused);

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
