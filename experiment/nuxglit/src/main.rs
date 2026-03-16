use anyhow::Result;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[allow(dead_code)]
#[path = "../../../src/audio/capture.rs"]
mod capture;
#[allow(dead_code)]
#[path = "../../../src/spectrum.rs"]
mod spectrum;

use capture::{AudioCapture, CaptureConfig};
use spectrum::{SpectrumAnalyzer, SpectrumConfig};

const BIN_COUNT: usize = 96;

#[repr(C)]
#[derive(Clone, Copy)]
struct FrameSnapshot {
    level: f32,
    peak: f32,
    bins: [f32; BIN_COUNT],
}

unsafe extern "C" {
    fn nuxglit_run() -> i32;
    fn nuxglit_set_status(text: *const std::os::raw::c_char);
    fn nuxglit_publish_frame(frame: *const FrameSnapshot);
    fn nuxglit_request_quit();
}

fn set_status(text: &str) {
    let status = CString::new(text).expect("status text should be valid");
    unsafe {
        nuxglit_set_status(status.as_ptr());
    }
}

fn publish_frame(frame: &FrameSnapshot) {
    unsafe {
        nuxglit_publish_frame(frame as *const FrameSnapshot);
    }
}

fn fill_bins(frame_number: u64, bins: &mut [f32; BIN_COUNT]) {
    for (index, bin) in bins.iter_mut().enumerate() {
        let x = index as f32 / BIN_COUNT as f32;
        let wave = ((frame_number as f32 / 18.0) + x * 10.0).sin() * 0.24 + 0.34;
        let shimmer = ((frame_number as f32 / 11.0) + x * 33.0).cos() * 0.14 + 0.16;
        let ridge =
            (((frame_number as f32 / 43.0) + x * 4.0).sin() * 0.5 + 0.5).powf(3.2);
        let pulse =
            (((frame_number as f32 / 27.0) + x * 7.0).cos() * 0.5 + 0.5).powf(5.0) * 0.3;
        *bin = (wave + shimmer + ridge * 0.38 + pulse).clamp(0.02, 1.0);
    }
}

fn spawn_demo_producer(running: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut frame_number = 0u64;
        let mut snapshot = FrameSnapshot {
            level: 0.0,
            peak: 0.0,
            bins: [0.0; BIN_COUNT],
        };

        while running.load(Ordering::Relaxed) {
            fill_bins(frame_number, &mut snapshot.bins);
            let phase = frame_number as f32 / 17.0;
            let level = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            snapshot.level = level;
            snapshot.peak = snapshot
                .bins
                .iter()
                .copied()
                .fold(0.0f32, f32::max);

            let mode = if (frame_number / 80) % 2 == 0 {
                "demo sweep"
            } else {
                "warm glass bars"
            };
            set_status(&format!(
                "{mode} · {:.0}% level · {} bins · Rust->C++ in-process",
                level * 100.0,
                BIN_COUNT
            ));
            publish_frame(&snapshot);

            frame_number += 1;
            thread::sleep(Duration::from_millis(33));
        }
    })
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let mean_square =
        samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32;
    mean_square.sqrt()
}

fn start_live_capture(source: Option<String>) -> Result<AudioCapture> {
    let status_label = source
        .as_deref()
        .map(|name| format!("live capture · requested source {name}"))
        .unwrap_or_else(|| "live capture · default source".to_string());
    set_status(&format!("{status_label} · waiting for audio"));

    let analyzer = Arc::new(std::sync::Mutex::new(SpectrumAnalyzer::new(SpectrumConfig {
        num_bands: BIN_COUNT,
        smoothing: 0.2,
        ..Default::default()
    })));
    let live_label = status_label.clone();

    AudioCapture::new(
        Box::new(move |samples| {
            let level = (rms(samples) * 5.5).clamp(0.0, 1.0);
            let Ok(mut analyzer) = analyzer.lock() else {
                return;
            };
            analyzer.push_samples(samples);
            if !analyzer.process() {
                return;
            }
            let data = analyzer.data();
            let mut snapshot = FrameSnapshot {
                level,
                peak: data.peak,
                bins: [0.0; BIN_COUNT],
            };
            let copy_len = data.bands.len().min(BIN_COUNT);
            snapshot.bins[..copy_len].copy_from_slice(&data.bands[..copy_len]);
            set_status(&format!(
                "{live_label} · {:.0}% level · {:.0}% peak",
                snapshot.level * 100.0,
                snapshot.peak * 100.0
            ));
            publish_frame(&snapshot);
        }),
        CaptureConfig {
            auto_gain_enabled: false,
            source,
            ..Default::default()
        },
    )
}

fn spawn_autostop(limit_ms: u64) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(limit_ms));
        unsafe {
            nuxglit_request_quit();
        }
    })
}

fn main() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let autostop_ms = std::env::var("NUXGLIT_AUTOSTOP_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let demo_mode = args.iter().any(|arg| arg == "--demo");
    let source = args
        .windows(2)
        .find_map(|window| (window[0] == "--source").then(|| window[1].clone()));

    let autostop = autostop_ms.map(spawn_autostop);

    let mut producer = if demo_mode {
        set_status("demo mode · synthesizing bars");
        Some(spawn_demo_producer(running.clone()))
    } else {
        None
    };

    let mut capture = if demo_mode {
        None
    } else {
        match start_live_capture(source.clone()) {
            Ok(capture) => Some(capture),
            Err(error) => {
                eprintln!("nuxglit: live capture unavailable ({error}); falling back to demo");
                set_status("live capture failed · falling back to demo");
                producer = Some(spawn_demo_producer(running.clone()));
                None
            }
        }
    };

    run_ui_loop(running, producer, autostop, capture.take())
}

fn run_ui_loop(
    running: Arc<AtomicBool>,
    producer: Option<thread::JoinHandle<()>>,
    autostop: Option<thread::JoinHandle<()>>,
    mut capture: Option<AudioCapture>,
) -> Result<()> {
    let exit_code = unsafe { nuxglit_run() };
    running.store(false, Ordering::Relaxed);
    if let Some(mut capture) = capture.take() {
        capture.stop();
    }
    if let Some(producer) = producer {
        let _ = producer.join();
    }
    if let Some(autostop) = autostop {
        let _ = autostop.join();
    }

    if exit_code == 0 {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Qt app exited with status {exit_code}"))
    }
}
