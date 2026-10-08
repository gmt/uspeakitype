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
#[ignore = "requires isolated PipeWire daemon; do not use the desktop audio session"]
fn test_audio_capture_init() {
    use usit::audio::{AudioCapture, CaptureConfig};

    let config = CaptureConfig {
        source: Some(
            std::env::var("USIT_TEST_SOURCE")
                .expect("set USIT_TEST_SOURCE to an isolated virtual source"),
        ),
        ..CaptureConfig::default()
    };

    // Dummy callback that does nothing
    let callback = Box::new(|_samples: &[f32]| {});

    let capture =
        AudioCapture::new(callback, config).expect("initialize isolated PipeWire capture");
    drop(capture);
}

/// Test that audio capture receives samples when PipeWire is running.
#[test]
#[ignore = "requires isolated PipeWire daemon and continuously playing virtual source"]
fn test_audio_capture_receives_samples() {
    use usit::audio::{AudioCapture, CaptureConfig};

    let config = CaptureConfig {
        source: Some(
            std::env::var("USIT_TEST_SOURCE")
                .expect("set USIT_TEST_SOURCE to an isolated virtual source"),
        ),
        ..CaptureConfig::default()
    };
    let sample_count = Arc::new(AtomicUsize::new(0));
    let sample_count_clone = sample_count.clone();

    let callback = Box::new(move |samples: &[f32]| {
        sample_count_clone.fetch_add(samples.len(), Ordering::Relaxed);
    });

    let capture =
        AudioCapture::new(callback, config).expect("initialize isolated PipeWire capture");

    // Let it run for 3 seconds
    std::thread::sleep(Duration::from_secs(3));

    let received = sample_count.load(Ordering::Relaxed);
    eprintln!("Received {} samples", received);

    drop(capture);

    assert!(received > 0, "virtual source must deliver captured samples");
}

/// Full pipeline test: capture audio → transcription
/// Requires: PipeWire running, virtual source with audio, models downloaded
#[test]
#[ignore = "requires isolated PipeWire speech fixture source and cached moonshine-tiny model"]
fn test_full_audio_pipeline() {
    use std::path::PathBuf;
    use usit::audio::{AudioCapture, CaptureConfig};
    use usit::backend::MoonshineStreamer;

    // Check for models
    let model_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("usit/models");

    let asr_dir = model_dir.join("moonshine-tiny");

    assert!(asr_dir.is_dir(), "model missing: {}", asr_dir.display());

    let mut streamer = MoonshineStreamer::new(&asr_dir).expect("load Moonshine model");

    // Accumulate audio samples
    let audio_buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let audio_buffer_clone = audio_buffer.clone();

    let config = CaptureConfig {
        source: Some(
            std::env::var("USIT_TEST_SOURCE")
                .expect("set USIT_TEST_SOURCE to an isolated virtual source"),
        ),
        ..CaptureConfig::default()
    };
    let callback = Box::new(move |samples: &[f32]| {
        let mut buf = audio_buffer_clone.lock();
        // Only keep ~10 seconds max
        if buf.len() < 16000 * 10 {
            buf.extend_from_slice(samples);
        }
    });

    let capture =
        AudioCapture::new(callback, config).expect("initialize isolated PipeWire capture");

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

    assert!(
        samples.len() >= 8000,
        "virtual source must supply at least half a second"
    );

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

    let all_text = transcriptions.join(" ").to_lowercase();
    assert!(
        all_text.contains("say") || all_text.contains("word"),
        "expected speech_sample.wav words, got: {all_text}"
    );
}

/// Verify the real PipeWire callback respects PTT edges and manual pause.
#[test]
#[ignore = "requires isolated PipeWire daemon and continuously playing virtual source"]
fn test_push_to_talk_admits_only_held_audio() {
    use std::sync::mpsc::{self, Receiver};
    use std::time::Instant;
    use usit::audio::capture::{CaptureEvent, CaptureMessage};
    use usit::audio::{AudioCapture, CaptureConfig, CaptureControl};

    fn await_audio(receiver: &Receiver<CaptureMessage>) {
        match receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("held source must deliver audio")
            .event
        {
            CaptureEvent::Audio(frame) => {
                assert!(frame.push_to_talk);
                assert!(!frame.samples.is_empty());
            }
            other => panic!("expected held audio, got {other:?}"),
        }
    }

    fn drain_to_edge(receiver: &Receiver<CaptureMessage>, reset: bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("control edge must not be lost behind audio")
                .event
            {
                CaptureEvent::Audio(_) => {}
                CaptureEvent::Reset if reset => break,
                CaptureEvent::Finish if !reset => break,
                other => panic!("unexpected edge: {other:?}"),
            }
        }
        assert!(
            matches!(
                receiver.recv_timeout(Duration::from_millis(250)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "idle must admit no audio"
        );
    }

    let control = Arc::new(CaptureControl::new());
    let (sender, receiver) = mpsc::channel();
    control.set_event_sender(sender);
    control.set_push_to_talk(true);
    assert!(matches!(
        receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("missing control edge")
            .event,
        CaptureEvent::Reset
    ));
    let config = CaptureConfig {
        source: Some(
            std::env::var("USIT_TEST_SOURCE")
                .expect("set USIT_TEST_SOURCE to an isolated virtual source"),
        ),
        ..CaptureConfig::default()
    };
    let capture = AudioCapture::with_control(Box::new(|_| {}), config, control.clone())
        .expect("start isolated PipeWire capture");
    let deadline = Instant::now() + Duration::from_secs(3);
    while control.sample_rate() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(control.sample_rate(), 16000, "source must negotiate 16kHz");
    assert!(
        matches!(
            receiver.recv_timeout(Duration::from_millis(250)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "initial PTT idle must admit no audio"
    );

    assert!(control.set_ptt_held(true));
    assert!(matches!(
        receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("missing control edge")
            .event,
        CaptureEvent::Begin
    ));
    await_audio(&receiver);
    assert!(!control.set_ptt_held(false));
    drain_to_edge(&receiver, false);

    assert!(control.set_ptt_held(true));
    assert!(matches!(
        receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("missing control edge")
            .event,
        CaptureEvent::Begin
    ));
    await_audio(&receiver);
    control.pause();
    drain_to_edge(&receiver, true);
    assert!(!control.set_ptt_held(true), "manual pause takes precedence");
    control.resume();
    assert!(
        matches!(
            receiver.recv_timeout(Duration::from_millis(250)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "resume must not restore a stale hold"
    );

    assert!(control.set_ptt_held(true));
    assert!(matches!(
        receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("missing control edge")
            .event,
        CaptureEvent::Begin
    ));
    await_audio(&receiver);
    control.set_ptt_held(false);
    drain_to_edge(&receiver, false);
    drop(capture);
}
