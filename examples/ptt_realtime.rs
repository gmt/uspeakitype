//! Replay a long push-to-talk hold in real time through the capture gate and the
//! worker's merge-then-decode loop, then report preview cadence and final text.
//!
//! cargo run --release --example ptt_realtime -- MODELS_DIR MODEL WAV [REPEATS]
//!
//! MODEL is `moonshine-base` or `parakeet-tdt-0.6b-v3`. Audio arrives in 21 ms
//! frames, PipeWire's default quantum, while decoding runs on a worker thread.

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::ValueEnum;
use usit::audio::capture::{merge_queued_audio, CaptureEvent};
use usit::audio::{CaptureControl, SileroVad, VadConfig};
use usit::backend::{MoonshineStreamer, NemoTransducerStreamer};
use usit::config::AsrModelId;
use usit::streaming::{BoxedTranscriber, StreamEvent, StreamingConfig, StreamingTranscriber};

const FRAME: usize = 341;

/// Samples decoded, (hold position, cost) per preview, last preview, typed text.
type Replay = (usize, Vec<(f64, Duration)>, String, String);

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let models = args.next().context("models directory")?;
    let model = AsrModelId::from_str(&args.next().context("model id")?, true)
        .map_err(|error| anyhow::anyhow!(error))?;
    let wav = args.next().context("16 kHz mono WAV")?;
    let repeats: usize = args.next().map_or(Ok(6), |value| value.parse())?;
    let mut reader = hound::WavReader::open(&wav)?;
    let max = 2_f32.powi(i32::from(reader.spec().bits_per_sample) - 1);
    let clip: Vec<f32> = reader
        .samples::<i32>()
        .map(|sample| sample.map(|value| value as f32 / max))
        .collect::<Result<_, _>>()?;
    let audio: Vec<f32> = clip
        .iter()
        .copied()
        .cycle()
        .take(clip.len() * repeats)
        .collect();

    let models = Path::new(&models);
    let vad = SileroVad::new(&models.join("silero_vad.onnx"), VadConfig::default())?;
    let directory = models.join(model.dir_name());
    let transcriber: BoxedTranscriber = if model.is_moonshine() {
        Box::new(MoonshineStreamer::new(&directory)?)
    } else {
        Box::new(NemoTransducerStreamer::new(&directory)?)
    };
    let mut streaming =
        StreamingTranscriber::new(vad, transcriber, StreamingConfig::for_model(model));

    let control = CaptureControl::new();
    let (tx, rx) = mpsc::channel();
    control.set_event_sender(tx);
    control.set_push_to_talk(true);

    let worker = thread::spawn(move || -> Result<Replay> {
        let mut decoded = 0;
        let mut last_preview = String::new();
        let mut committed: Vec<String> = Vec::new();
        let mut previews = Vec::new();
        let mut next = None;
        loop {
            let mut message = match next.take() {
                Some(message) => message,
                None => rx.recv()?,
            };
            next = merge_queued_audio(&mut message, &rx);
            match message.event {
                CaptureEvent::Begin => streaming.begin_push_to_talk(),
                CaptureEvent::Reset => streaming.reset(),
                CaptureEvent::Audio(frame) => {
                    decoded += frame.samples.len();
                    let started = Instant::now();
                    let events = streaming.process_push_to_talk(&frame.samples)?;
                    if !events.is_empty() {
                        previews.push((decoded as f64 / 16_000.0, started.elapsed()));
                    }
                    for event in events {
                        match event {
                            StreamEvent::Partial(text) => last_preview = text,
                            StreamEvent::Commit(text) => {
                                println!("segment at {:5.1}s: {text}", decoded as f64 / 16_000.0);
                                committed.push(text);
                            }
                        }
                    }
                }
                CaptureEvent::Finish => {
                    for event in streaming.finish_push_to_talk()? {
                        if let StreamEvent::Commit(text) = event {
                            println!("segment at release: {text}");
                            committed.push(text);
                        }
                    }
                    let text = committed.join(" ");
                    return Ok((decoded, previews, last_preview, text));
                }
            }
        }
    });

    control.set_ptt_held(true);
    let started = Instant::now();
    for (index, frame) in audio.chunks(FRAME).enumerate() {
        control.admit_samples(frame);
        let due = Duration::from_secs_f64(((index + 1) * FRAME) as f64 / 16_000.0);
        if let Some(wait) = due.checked_sub(started.elapsed()) {
            thread::sleep(wait);
        }
    }
    let released = Instant::now();
    control.set_ptt_held(false);
    let (decoded, previews, last_preview, text) = worker.join().unwrap()?;
    let latency = released.elapsed();

    println!(
        "{model}: {:.1}s held, {:.1}s reached the decoder ({} samples lost)",
        audio.len() as f64 / 16_000.0,
        decoded as f64 / 16_000.0,
        audio.len() - decoded
    );
    println!("{} live previews; cost by hold position:", previews.len());
    for (at, cost) in previews.iter().step_by((previews.len() / 8).max(1)) {
        println!("  at {at:5.1}s: {:.0} ms", cost.as_secs_f64() * 1000.0);
    }
    println!("release to text: {:.0} ms", latency.as_secs_f64() * 1000.0);
    println!("last preview: {last_preview}");
    println!("text: {text}");
    Ok(())
}
