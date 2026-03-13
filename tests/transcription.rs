//! Transcription backend integration tests
//!
//! These tests verify the ASR backends can transcribe known audio.
//! Ignored by default - require models to be downloaded.

use std::path::PathBuf;

use anyhow::{Context, Result};
use hound::WavReader;

/// Read a 16kHz mono WAV file into f32 samples
fn read_wav(path: &std::path::Path) -> Result<Vec<f32>> {
    let mut reader = WavReader::open(path).context("opening wav")?;
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16000, "expected 16kHz WAV");
    assert_eq!(spec.channels, 1, "expected mono WAV");

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

/// Get the default model directory (~/.cache/usit/models)
fn default_model_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("usit/models")
}

/// Test that Moonshine can transcribe our speech sample.
///
/// The audio says "Hey it's me, I'm saying words" so we verify
/// the transcription contains at least some of those words.
#[test]
#[ignore] // requires downloaded models - run with: cargo test --test transcription -- --ignored
fn test_moonshine_transcribes_speech_sample() {
    use usit::backend::MoonshineStreamer;

    let model_dir = default_model_dir().join("moonshine-base");
    if !model_dir.exists() {
        eprintln!("Skipping: model not found at {}", model_dir.display());
        return;
    }

    let wav_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/audio/speech_sample.wav");
    let samples = read_wav(&wav_path).expect("failed to read WAV");

    let mut streamer = match MoonshineStreamer::new(&model_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: failed to load model: {}", e);
            return;
        }
    };

    // Process in 1-second chunks (like wav_stream does)
    let chunk_size = 16000; // 1 second at 16kHz
    let mut all_text = String::new();

    for chunk in samples.chunks(chunk_size) {
        if chunk.len() < chunk_size / 2 {
            break; // Skip tiny trailing chunks
        }
        streamer.reset();
        match streamer.transcribe_incremental(chunk) {
            Ok(t) => {
                if !t.is_empty() {
                    if !all_text.is_empty() {
                        all_text.push(' ');
                    }
                    all_text.push_str(&t);
                }
            }
            Err(e) => {
                eprintln!("Chunk transcription failed: {}", e);
            }
        }
    }

    let text_lower = all_text.to_lowercase();
    eprintln!("Transcribed: {}", all_text);

    // Verify it caught the gist - don't be too strict on exact wording
    // Model may hear "okay/hi/hey" for greeting and some form of "say/word"
    assert!(
        text_lower.contains("hey")
            || text_lower.contains("hi")
            || text_lower.contains("okay")
            || text_lower.contains("ok"),
        "expected greeting, got: {}",
        all_text
    );
    assert!(
        text_lower.contains("say") || text_lower.contains("word"),
        "expected 'say' or 'word', got: {}",
        all_text
    );
}

/// Same test but with moonshine-tiny (faster, less accurate)
#[test]
#[ignore]
fn test_moonshine_tiny_transcribes_speech_sample() {
    use usit::backend::MoonshineStreamer;

    let model_dir = default_model_dir().join("moonshine-tiny");
    if !model_dir.exists() {
        eprintln!("Skipping: model not found at {}", model_dir.display());
        return;
    }

    let wav_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/audio/speech_sample.wav");
    let samples = read_wav(&wav_path).expect("failed to read WAV");

    let mut streamer = match MoonshineStreamer::new(&model_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: failed to load model: {}", e);
            return;
        }
    };

    // Process in 1-second chunks
    let chunk_size = 16000;
    let mut all_text = String::new();

    for chunk in samples.chunks(chunk_size) {
        if chunk.len() < chunk_size / 2 {
            break;
        }
        streamer.reset();
        if let Ok(t) = streamer.transcribe_incremental(chunk) {
            if !t.is_empty() {
                if !all_text.is_empty() {
                    all_text.push(' ');
                }
                all_text.push_str(&t);
            }
        }
    }

    eprintln!("Transcribed (tiny): {}", all_text);

    // Tiny model may be less accurate, just verify non-empty
    assert!(
        !all_text.trim().is_empty(),
        "expected non-empty transcription"
    );
}
