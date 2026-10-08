//! Compare cached ASR backends on a 16 kHz mono WAV without microphone capture.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use hound::WavReader;

use usit::backend::{MoonshineStreamer, NemoTransducerStreamer};
use usit::config::AsrModelId;
use usit::streaming::{BoxedTranscriber, Transcriber};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value_t = 16000)]
    sample_rate: u32,
    #[arg(long, default_value_t = 20)]
    frame_ms: u64,
    #[arg(long, conflicts_with = "whole_file")]
    realtime: bool,
    /// Directory containing the selected model's ONNX files.
    #[arg(long)]
    model_dir: PathBuf,
    #[arg(long, value_enum, default_value = "moonshine-base")]
    model: AsrModelId,
    /// Pass the complete recording to the backend in one call, preserving phrase context.
    /// The backend may still use internal decoding windows.
    #[arg(long)]
    whole_file: bool,
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

    match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("reading floating-point WAV samples"),
        hound::SampleFormat::Int => {
            let max = 2_f32.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / max))
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("reading integer WAV samples")
        }
    }
}

fn transcribe_audio(
    streamer: &mut dyn Transcriber,
    samples: &[f32],
    args: &Args,
    mut emit: impl FnMut(String),
) -> Result<Duration> {
    let mut inference_time = Duration::ZERO;
    let mut infer = |buffer: &[f32]| -> Result<()> {
        if !buffer.is_empty() {
            streamer.reset();
            let started = Instant::now();
            let text = streamer.transcribe_incremental(buffer)?;
            inference_time += started.elapsed();
            emit(text);
        }
        Ok(())
    };

    if args.whole_file {
        infer(samples)?;
    } else {
        let frame_size = u64::from(args.sample_rate)
            .checked_mul(args.frame_ms)
            .and_then(|size| usize::try_from(size / 1000).ok())
            .filter(|size| *size > 0)
            .context("frame-ms must produce a positive, representable frame size")?;
        let inference_chunk = args.sample_rate as usize;
        let mut buffer = Vec::with_capacity(inference_chunk);

        for frame in samples.chunks(frame_size) {
            let started = Instant::now();
            buffer.extend_from_slice(frame);
            if buffer.len() >= inference_chunk {
                infer(&buffer)?;
                buffer.clear();
            }
            if args.realtime {
                let frame_duration =
                    Duration::from_secs_f64(frame.len() as f64 / f64::from(args.sample_rate));
                std::thread::sleep(frame_duration.saturating_sub(started.elapsed()));
            }
        }
        infer(&buffer)?;
    }
    Ok(inference_time)
}

fn main() -> Result<()> {
    let args = Args::parse();
    anyhow::ensure!(args.sample_rate == 16000, "only 16000 Hz is supported");
    let samples = read_wav(&args.input)?;
    anyhow::ensure!(!samples.is_empty(), "WAV contains no audio samples");

    let started = Instant::now();
    let mut streamer: BoxedTranscriber = if args.model.is_moonshine() {
        Box::new(MoonshineStreamer::new(&args.model_dir)?)
    } else {
        Box::new(NemoTransducerStreamer::new(&args.model_dir)?)
    };
    let load_time = started.elapsed();
    let inference_time = transcribe_audio(streamer.as_mut(), &samples, &args, |text| {
        println!("{text}");
    })?;
    let audio_seconds = samples.len() as f64 / f64::from(args.sample_rate);
    eprintln!(
        "model={} audio={:.3}s load={:.3}s inference={:.3}s rtf={:.3}",
        args.model.dir_name(),
        audio_seconds,
        load_time.as_secs_f64(),
        inference_time.as_secs_f64(),
        inference_time.as_secs_f64() / audio_seconds,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingTranscriber {
        inputs: Vec<Vec<f32>>,
    }

    impl Transcriber for RecordingTranscriber {
        fn transcribe_incremental(&mut self, samples: &[f32]) -> Result<String> {
            self.inputs.push(samples.to_vec());
            Ok(String::new())
        }

        fn reset(&mut self) {}
    }

    fn args(extra: &[&str]) -> Args {
        Args::try_parse_from(
            [
                "wav_stream",
                "--input",
                "sample.wav",
                "--model-dir",
                "model",
            ]
            .into_iter()
            .chain(extra.iter().copied()),
        )
        .unwrap()
    }

    #[test]
    fn whole_file_preserves_recording_context() {
        let samples = vec![0.25; 20001];
        let mut transcriber = RecordingTranscriber::default();
        transcribe_audio(&mut transcriber, &samples, &args(&["--whole-file"]), |_| {}).unwrap();
        assert_eq!(transcriber.inputs, vec![samples]);
    }

    #[test]
    fn chunked_mode_keeps_trailing_audio() {
        let samples: Vec<_> = (0..20001).map(|sample| sample as f32).collect();
        let mut transcriber = RecordingTranscriber::default();
        transcribe_audio(&mut transcriber, &samples, &args(&[]), |_| {}).unwrap();
        assert_eq!(transcriber.inputs.len(), 2);
        assert_eq!(transcriber.inputs.concat(), samples);
    }

    #[test]
    fn zero_frame_duration_is_rejected() {
        let mut transcriber = RecordingTranscriber::default();
        assert!(transcribe_audio(
            &mut transcriber,
            &[0.0; 100],
            &args(&["--frame-ms", "0"]),
            |_| {}
        )
        .is_err());
    }
}
