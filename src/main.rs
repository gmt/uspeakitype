mod audio;
mod backend;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Parser;

use audio::{AudioCapture, CaptureConfig, CaptureControl};

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

    let capture_control: Option<Arc<CaptureControl>> = if args.demo {
        run_demo_audio(audio_state.clone());
        None
    } else {
        let config = CaptureConfig {
            auto_gain_enabled: args.auto_gain,
            agc: Default::default(),
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

    if args.headless {
        println!("Headless mode - press Ctrl+C to exit");
        println!("Commands: [p]ause, [g]ain toggle, [q]uit");

        while running.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let state = audio_state.read();
            if !state.display().is_empty() {
                println!("Transcript: {}", state.display());
            }
        }
    } else {
        ui::run(audio_state, running, capture_control);
    }

    Ok(())
}

fn run_demo_audio(audio_state: ui::SharedAudioState) {
    std::thread::spawn(move || {
        let mut t = 0.0f32;

        loop {
            std::thread::sleep(std::time::Duration::from_millis(16));

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

            if t > 2.0 && t < 2.1 {
                audio_state.write().set_partial("Listening...".to_string());
            }
            if t > 4.0 && t < 4.1 {
                audio_state.write().set_partial("Hello world".to_string());
            }
            if t > 5.0 && t < 5.1 {
                audio_state.write().commit();
            }
            if t > 6.0 && t < 6.1 {
                audio_state
                    .write()
                    .set_partial("this is streaming".to_string());
            }
            if t > 7.0 && t < 7.1 {
                audio_state.write().commit();
                audio_state.write().set_partial("transcription".to_string());
            }
        }
    });
}
