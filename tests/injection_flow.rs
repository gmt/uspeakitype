//! Integration tests for the injection channel flow
//!
//! These tests verify that text flows correctly from the transcription
//! layer through the injection channel to the injector backend.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use usit::input::TextInjector;

/// Mock injector that records all injected text
struct MockInjector {
    injected: Arc<Mutex<Vec<String>>>,
}

impl MockInjector {
    fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
        let injected = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                injected: injected.clone(),
            },
            injected,
        )
    }
}

impl TextInjector for MockInjector {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn inject(&mut self, text: &str) -> anyhow::Result<()> {
        self.injected.lock().unwrap().push(text.to_string());
        Ok(())
    }
}

/// Test that simulates the injection flow from main.rs:
/// 1. Injection channel is created
/// 2. Injector thread spawns and waits on receiver
/// 3. Text is sent through the channel
/// 4. Injector receives and processes the text
#[test]
fn test_injection_channel_flow() {
    let (injection_tx, injection_rx) = mpsc::channel::<String>();

    let (mut mock_injector, injected_texts) = MockInjector::new();

    // Spawn injector thread (mirrors main.rs pattern)
    let injector_handle = std::thread::spawn(move || {
        // This matches the loop in main.rs line ~469
        while let Ok(text) = injection_rx.recv() {
            if let Err(e) = mock_injector.inject(&text) {
                eprintln!("Injection error: {}", e);
            }
        }
    });

    // Send text through the channel (mirrors main.rs line ~583)
    injection_tx.send("Hello world".to_string()).unwrap();
    injection_tx.send("Testing injection".to_string()).unwrap();

    // Give the injector thread time to process
    std::thread::sleep(Duration::from_millis(50));

    // Drop sender to signal shutdown
    drop(injection_tx);

    // Wait for injector thread to exit
    injector_handle.join().expect("Injector thread panicked");

    // Verify the mock injector received the text
    let texts = injected_texts.lock().unwrap();
    assert_eq!(
        texts.len(),
        2,
        "Expected 2 injected texts, got {}",
        texts.len()
    );
    assert_eq!(texts[0], "Hello world");
    assert_eq!(texts[1], "Testing injection");
}

/// Test that simulates the full flow with cloned sender (like injection_tx_for_worker)
#[test]
fn test_injection_with_cloned_sender() {
    let (injection_tx, injection_rx) = mpsc::channel::<String>();
    let injection_tx_for_worker = injection_tx.clone();

    let (mut mock_injector, injected_texts) = MockInjector::new();

    // Spawn injector thread
    let injector_handle = std::thread::spawn(move || {
        while let Ok(text) = injection_rx.recv() {
            let _ = mock_injector.inject(&text);
        }
    });

    // Simulate streaming worker sending text (uses cloned sender)
    let worker_handle = std::thread::spawn(move || {
        injection_tx_for_worker
            .send("From worker".to_string())
            .unwrap();
    });

    worker_handle.join().unwrap();

    // Give time to process
    std::thread::sleep(Duration::from_millis(50));

    // Drop original sender
    drop(injection_tx);

    injector_handle.join().expect("Injector thread panicked");

    let texts = injected_texts.lock().unwrap();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0], "From worker");
}

/// Disabled injection must select no backend without accessing the desktop.
#[test]
fn test_injection_channel_with_real_backend_selection() {
    use usit::input::select_backend;

    let disabled = vec![
        "input_method".to_string(),
        "fcitx5_bridge".to_string(),
        "wrtype".to_string(),
        "ydotool".to_string(),
    ];
    assert!(select_backend(&disabled, false).is_none());
}

/// Test that simulates the exact streaming worker flow from main.rs
/// This mirrors lines 569-587 of main.rs where StreamEvent::Commit triggers injection
#[test]
fn test_streaming_worker_injection_flow() {
    let (injection_tx, injection_rx) = mpsc::channel::<String>();

    let (mut mock_injector, injected_texts) = MockInjector::new();

    // Spawn injector thread
    let injector_handle = std::thread::spawn(move || {
        while let Ok(text) = injection_rx.recv() {
            let _ = mock_injector.inject(&text);
        }
    });

    // Create shared audio state (like main.rs line 365)
    let audio_state = usit::ui::new_shared_state();

    // Clone sender for worker (like main.rs line 517)
    let injection_tx_for_worker = injection_tx.clone();
    let audio_state_for_worker = audio_state.clone();

    // Simulate streaming worker thread (mirrors main.rs lines 569-587)
    let worker_handle = std::thread::spawn(move || {
        // Simulate a commit event
        let text = "Transcribed text".to_string();

        let mut state = audio_state_for_worker.write();

        // This mirrors main.rs lines 578-584
        state.set_partial(text.clone());
        state.commit();

        // This is the critical check from line 582
        if !state.is_paused && state.injection_enabled {
            let send_result = injection_tx_for_worker.send(text);
            eprintln!("Send result: {:?}", send_result);
        } else {
            eprintln!(
                "Injection skipped: is_paused={}, injection_enabled={}",
                state.is_paused, state.injection_enabled
            );
        }
    });

    worker_handle.join().unwrap();
    std::thread::sleep(Duration::from_millis(50));

    drop(injection_tx);
    injector_handle.join().expect("Injector thread panicked");

    let texts = injected_texts.lock().unwrap();
    assert_eq!(
        texts.len(),
        1,
        "Expected 1 injected text, got {}. Check is_paused/injection_enabled defaults!",
        texts.len()
    );
    assert_eq!(texts[0], "Transcribed text");
}

/// Test that AudioState defaults allow injection
#[test]
fn test_audio_state_defaults_allow_injection() {
    let state = usit::ui::AudioState::new();

    // These defaults MUST allow injection
    assert!(!state.is_paused, "Default is_paused should be false");
    assert!(
        state.injection_enabled,
        "Default injection_enabled should be true"
    );

    // The condition from main.rs line 582
    let should_inject = !state.is_paused && state.injection_enabled;
    assert!(should_inject, "Default AudioState should allow injection");
}

/// Test that ctrlc handler setup doesn't interfere with channel communication
/// This mirrors the exact pattern from main.rs lines 354-363
#[test]
fn test_ctrlc_handler_with_injection_flow() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let (injection_tx, injection_rx) = mpsc::channel::<String>();
    let (mut mock_injector, injected_texts) = MockInjector::new();

    let running = Arc::new(AtomicBool::new(true));

    // Set up signal handler (mirrors main.rs)
    // Note: ctrlc::set_handler can only be called once per process,
    // so we skip this in tests to avoid conflicts
    // {
    //     let running = running.clone();
    //     ctrlc::set_handler(move || {
    //         running.store(false, Ordering::SeqCst);
    //     }).ok();
    // }

    // Spawn injector thread
    let injector_handle = std::thread::spawn(move || {
        while let Ok(text) = injection_rx.recv() {
            let _ = mock_injector.inject(&text);
        }
    });

    // Send text while "running"
    assert!(running.load(Ordering::Relaxed));
    injection_tx.send("Test while running".to_string()).unwrap();

    std::thread::sleep(Duration::from_millis(50));

    // Simulate shutdown
    running.store(false, Ordering::SeqCst);
    drop(injection_tx);

    injector_handle.join().expect("Injector thread panicked");

    let texts = injected_texts.lock().unwrap();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0], "Test while running");
}

/// Create an input method only on the isolated canonical compositor.
#[test]
fn test_input_method_injector_creation() {
    if std::env::var("USIT_CANONICAL_TEST_ENV").as_deref() != Ok("1") {
        eprintln!("Skipping: requires isolated Docker compositor");
        return;
    }
    use usit::input::InputMethodInjector;

    match InputMethodInjector::new() {
        Ok(injector) => {
            // Verify basic properties
            assert_eq!(injector.name(), "input_method");
            eprintln!("✓ InputMethodInjector created successfully");

            // Check that it can be used in a Box<dyn TextInjector>
            let boxed: Box<dyn TextInjector> = Box::new(injector);
            assert_eq!(boxed.name(), "input_method");
        }
        Err(e) => {
            panic!("Isolated compositor must support input method: {}", e);
        }
    }
}
