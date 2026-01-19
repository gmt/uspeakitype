use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use supports_unicode::Stream;
use terminal_size::{terminal_size, Height, Width};

use barbara::audio::{self, AudioCapture, CaptureConfig, CaptureControl};
use barbara::spectrum::get_color_scheme;
use barbara::ui;
use barbara::ui::terminal::{TerminalConfig, TerminalMode, TerminalVisualizer};

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
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

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
        let capture = AudioCapture::new(
            Box::new(move |samples| {
                state.write().update_samples(samples);
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

    if args.headless || args.ansi {
        run_terminal_loop(audio_state, running, &args)?;
    } else {
        ui::run(audio_state, running, capture_control);
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

fn run_terminal_loop(
    audio_state: ui::SharedAudioState,
    running: Arc<AtomicBool>,
    args: &Args,
) -> anyhow::Result<()> {
    if !args.ansi {
        return run_headless_text(audio_state, running);
    }

    let (term_width, term_height) = terminal_size()
        .map(|(Width(w), Height(h))| (w as usize, h as usize))
        .unwrap_or((80, 24));

    let width = args
        .ansi_width
        .unwrap_or(((term_width as f32) * 0.8).round() as usize)
        .max(1)
        .min(term_width);
    let height = args
        .ansi_height
        .unwrap_or(term_height.min(8))
        .max(1)
        .min(term_height);

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
    };

    let mut visualizer = TerminalVisualizer::new(config);

    let color_name = match args.color {
        ColorSchemeName::Flame => "flame",
        ColorSchemeName::Ice => "ice",
        ColorSchemeName::Mono => "mono",
    };
    visualizer.set_color_scheme(get_color_scheme(color_name));

    TerminalVisualizer::init_terminal()?;

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(16));

        let samples = {
            let state = audio_state.read();
            state.samples.clone()
        };

        if samples.is_empty() {
            continue;
        }

        visualizer.push_samples(&samples);
        visualizer.process_and_render()?;
    }

    TerminalVisualizer::cleanup_terminal()?;
    Ok(())
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
