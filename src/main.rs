mod audio;
mod backend;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Parser;

use audio::AudioCapture;

#[derive(Parser)]
#[command(name = "barbara")]
#[command(about = "Streaming speech-to-text with live revision")]
struct Args {
    #[arg(long)]
    headless: bool,

    #[arg(long)]
    demo: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!("barbara v{}", env!("CARGO_PKG_VERSION"));

    let running = Arc::new(AtomicBool::new(true));
    let audio_state = ui::new_shared_state();

    if args.demo {
        run_demo_audio(audio_state.clone());
    } else {
        let state = audio_state.clone();
        let _capture = AudioCapture::new(Box::new(move |samples| {
            state.write().update_samples(samples);
        }))?;
    }

    if args.headless {
        println!("Headless mode - press Ctrl+C to exit");
        while running.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let state = audio_state.read();
            if !state.display().is_empty() {
                println!("Transcript: {}", state.display());
            }
        }
    } else {
        ui::run(audio_state, running);
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
