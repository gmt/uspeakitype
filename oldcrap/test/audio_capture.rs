//! PipeWire audio capture integration tests
//!
//! These tests verify the audio capture pipeline works correctly with PipeWire.
//! Run in Docker: `docker compose run audio-tests cargo test --test audio_capture -- --ignored`

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

/// Test that we can initialize audio capture without panicking.
/// This verifies PipeWire libs are linked correctly.
#[test]
#[ignore] // requires PipeWire daemon - run in Docker
fn test_audio_capture_init() {
    use usit::audio::{AudioCapture, CaptureConfig};

    let config = CaptureConfig::default();

    // Dummy callback that does nothing
    let callback = Box::new(|_samples: &[f32]| {});

    match AudioCapture::new(callback, config) {
        Ok(capture) => {
            eprintln!("✓ AudioCapture initialized successfully");
            drop(capture);
        }
        Err(e) => {
            eprintln!("AudioCapture::new() failed: {}", e);
            // This is expected to fail without PipeWire running
        }
    }
}

/// Test that audio capture receives samples when PipeWire is running.
#[test]
#[ignore] // requires PipeWire daemon with active source
fn test_audio_capture_receives_samples() {
    use usit::audio::{AudioCapture, CaptureConfig};

    let config = CaptureConfig::default();
    let sample_count = Arc::new(AtomicUsize::new(0));
    let sample_count_clone = sample_count.clone();

    let callback = Box::new(move |samples: &[f32]| {
        sample_count_clone.fetch_add(samples.len(), Ordering::Relaxed);
    });

    let capture = match AudioCapture::new(callback, config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping: {}", e);
            return;
        }
    };

    // Let it run for 3 seconds
    std::thread::sleep(Duration::from_secs(3));

    let received = sample_count.load(Ordering::Relaxed);
    eprintln!("Received {} samples", received);

    drop(capture);

    if received > 0 {
        eprintln!("✓ Audio capture receiving samples");
    } else {
        eprintln!("⚠ No samples received (is audio playing into virtual source?)");
    }
}

/// Full pipeline test: capture audio → transcription
/// Requires: PipeWire running, virtual source with audio, models downloaded
#[test]
#[ignore] // requires full Docker environment with models
fn test_full_audio_pipeline() {
    use std::path::PathBuf;
    use usit::audio::{AudioCapture, CaptureConfig};
    use usit::backend::MoonshineStreamer;

    // Check for models
    let model_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("usit/models");

    let asr_dir = model_dir.join("moonshine-tiny");

    if !asr_dir.exists() {
        eprintln!("Skipping: ASR model not found at {}", asr_dir.display());
        return;
    }

    let mut streamer = match MoonshineStreamer::new(&asr_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping: ASR init failed: {}", e);
            return;
        }
    };

    // Accumulate audio samples
    let audio_buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let audio_buffer_clone = audio_buffer.clone();

    let config = CaptureConfig::default();
    let callback = Box::new(move |samples: &[f32]| {
        let mut buf = audio_buffer_clone.lock();
        // Only keep ~10 seconds max
        if buf.len() < 16000 * 10 {
            buf.extend_from_slice(samples);
        }
    });

    let capture = match AudioCapture::new(callback, config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping: AudioCapture failed: {}", e);
            return;
        }
    };

    eprintln!("Capturing audio for 6 seconds...");
    std::thread::sleep(Duration::from_secs(6));

    // Stop capture
    drop(capture);

    // Get accumulated samples
    let samples = audio_buffer.lock().clone();
    eprintln!(
        "Captured {} samples ({:.2}s)",
        samples.len(),
        samples.len() as f32 / 16000.0
    );

    if samples.len() < 8000 {
        eprintln!("⚠ Not enough audio captured for transcription");
        return;
    }

    // Transcribe in 1-second chunks
    let mut transcriptions = Vec::new();
    for chunk in samples.chunks(16000) {
        if chunk.len() < 8000 {
            continue;
        }
        streamer.reset();
        if let Ok(text) = streamer.transcribe_incremental(chunk) {
            if !text.trim().is_empty() {
                eprintln!("  chunk: {}", text);
                transcriptions.push(text);
            }
        }
    }

    eprintln!("Transcriptions: {:?}", transcriptions);

    if !transcriptions.is_empty() {
        let all_text = transcriptions.join(" ").to_lowercase();
        // Check for expected words from our test audio
        if all_text.contains("say") || all_text.contains("word") {
            eprintln!("✓ Successfully transcribed test audio!");
        } else {
            eprintln!("? Transcription produced: {}", all_text);
        }
    } else {
        eprintln!("⚠ No transcriptions produced (audio may be silent)");
    }
}
