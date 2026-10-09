//! Replay a held phrase through the production push-to-talk path and report
//! whether each live update finishes within its real-time audio budget.
//!
//! cargo run --release --example ptt_bench -- MODELS_DIR WAV [REPEATS]
//!
//! MODELS_DIR holds `parakeet-tdt-0.6b-v3/` and `silero_vad.onnx`. The clip is
//! looped REPEATS times to emulate a long hold. Set PTT_BENCH_TRACE=1 to print
//! the live partial text whenever it changes.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use usit::audio::vad::VadConfig;
use usit::audio::SileroVad;
use usit::backend::NemoTransducerStreamer;
use usit::config::AsrModelId;
use usit::streaming::{StreamEvent, StreamingConfig, StreamingTranscriber};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let models = args.next().context("models directory")?;
    let wav = args.next().context("16 kHz mono WAV")?;
    let repeats: usize = args.next().map_or(Ok(4), |value| value.parse())?;
    let mut reader = hound::WavReader::open(&wav)?;
    let max = 2_f32.powi(i32::from(reader.spec().bits_per_sample) - 1);
    let clip: Vec<f32> = match reader.spec().sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => reader
            .samples::<i32>()
            .map(|sample| sample.map(|value| value as f32 / max))
            .collect::<Result<_, _>>()?,
    };
    let audio: Vec<f32> = clip
        .iter()
        .copied()
        .cycle()
        .take(clip.len() * repeats)
        .collect();

    let models = Path::new(&models);
    let vad = SileroVad::new(&models.join("silero_vad.onnx"), VadConfig::default())?;
    let transcriber = NemoTransducerStreamer::new(models.join("parakeet-tdt-0.6b-v3"))?;
    let mut config = StreamingConfig::for_model(AsrModelId::ParakeetTdt06bV3);
    if let Some(ms) = std::env::var("PTT_BENCH_UPDATE_MS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        config.update_interval_samples = ms * 16;
    }
    let frame = config.update_interval_samples;
    let budget = Duration::from_secs_f64(frame as f64 / 16_000.0);
    let mut streaming = StreamingTranscriber::new(vad, transcriber, config);
    let trace = std::env::var_os("PTT_BENCH_TRACE").is_some();

    streaming.begin_push_to_talk();
    let mut costs = Vec::new();
    for (index, chunk) in audio.chunks(frame).enumerate() {
        let started = Instant::now();
        let events = streaming.process_push_to_talk(chunk)?;
        costs.push(started.elapsed());
        for event in events {
            if let (true, StreamEvent::Partial(text)) = (trace, event) {
                println!("{:5.1}s {text}", ((index + 1) * frame) as f64 / 16_000.0);
            }
        }
    }
    let started = Instant::now();
    let committed = streaming.finish_push_to_talk()?;
    let release = started.elapsed();

    let total: Duration = costs.iter().sum();
    let audio_secs = audio.len() as f64 / 16_000.0;
    let over = costs.iter().filter(|cost| **cost > budget).count();
    let mut sorted = costs.clone();
    sorted.sort();
    println!(
        "{audio_secs:.1}s held audio, {} updates of {:.0} ms budget",
        costs.len(),
        budget.as_secs_f64() * 1000.0
    );
    println!(
        "live {:.2}s total = {:.2}x real time; median {:.0} ms, p90 {:.0} ms, max {:.0} ms; {over} over budget",
        total.as_secs_f64(),
        total.as_secs_f64() / audio_secs,
        sorted[sorted.len() / 2].as_secs_f64() * 1000.0,
        sorted[sorted.len() * 9 / 10].as_secs_f64() * 1000.0,
        sorted.last().unwrap().as_secs_f64() * 1000.0,
    );
    println!("release {:.0} ms", release.as_secs_f64() * 1000.0);
    for event in committed {
        if let StreamEvent::Commit(text) = event {
            println!("committed: {text}");
        }
    }
    Ok(())
}
