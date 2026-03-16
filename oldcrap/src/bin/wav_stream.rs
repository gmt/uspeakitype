use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use hound::WavReader;

use usit::backend::MoonshineStreamer;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value_t = 16000)]
    sample_rate: u32,
    #[arg(long, default_value_t = 20)]
    frame_ms: u64,
    #[arg(long)]
    realtime: bool,
    #[arg(long)]
    model_dir: PathBuf,
}

fn read_wav(path: &PathBuf) -> Result<Vec<f32>> {
    let mut reader = WavReader::open(path).context("opening wav")?;
    let spec = reader.spec();
    if spec.sample_rate != 16000 {
        anyhow::bail!("expected 16kHz WAV, got {}", spec.sample_rate);
    }
    if spec.channels != 1 {
        anyhow::bail!("expected mono WAV, got {} channels", spec.channels);
    }

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let max = i16::MAX as f32;
            reader
                .samples::<i16>()
                .map(|s| s.unwrap_or(0) as f32 / max)
                .collect()
        }
    };

    Ok(samples)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let samples = read_wav(&args.input)?;

    let mut streamer = MoonshineStreamer::new(&args.model_dir)?;

    let frame_size = (args.sample_rate as u64 * args.frame_ms / 1000) as usize;
    // Moonshine decoders expect a reasonable chunk of audio. We still "stream" by reading
    // small frames, but we run inference on ~1s of audio at a time.
    let inference_chunk = args.sample_rate as usize;

    let mut buffer = Vec::with_capacity(inference_chunk);
    let mut start = 0;

    while start < samples.len() {
        let end = (start + frame_size).min(samples.len());
        buffer.extend_from_slice(&samples[start..end]);

        let now = Instant::now();
        if buffer.len() >= inference_chunk {
            streamer.reset();
            let text = streamer.transcribe_incremental(&buffer)?;
            println!("{}", text);
            buffer.clear();
        }
        if args.realtime {
            let elapsed = now.elapsed();
            let sleep_for = Duration::from_millis(args.frame_ms).saturating_sub(elapsed);
            std::thread::sleep(sleep_for);
        }

        start = end;
    }

    Ok(())
}
