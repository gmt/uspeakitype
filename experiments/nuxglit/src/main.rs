use anyhow::Result;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const BIN_COUNT: usize = 96;

unsafe extern "C" {
    fn nuxglit_run() -> i32;
    fn nuxglit_set_level(level: f32);
    fn nuxglit_set_status(text: *const std::os::raw::c_char);
    fn nuxglit_set_bins(bins: *const f32, len: usize);
    fn nuxglit_request_quit();
}

fn fill_bins(frame: u64, bins: &mut [f32; BIN_COUNT]) {
    for (index, bin) in bins.iter_mut().enumerate() {
        let x = index as f32 / BIN_COUNT as f32;
        let wave = ((frame as f32 / 18.0) + x * 10.0).sin() * 0.24 + 0.34;
        let shimmer = ((frame as f32 / 11.0) + x * 33.0).cos() * 0.14 + 0.16;
        let ridge = (((frame as f32 / 43.0) + x * 4.0).sin() * 0.5 + 0.5).powf(3.2);
        let pulse = (((frame as f32 / 27.0) + x * 7.0).cos() * 0.5 + 0.5).powf(5.0) * 0.3;
        *bin = (wave + shimmer + ridge * 0.38 + pulse).clamp(0.02, 1.0);
    }
}

fn main() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let worker_running = running.clone();
    let autostop_ms = std::env::var("NUXGLIT_AUTOSTOP_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());

    let producer = thread::spawn(move || {
        let started = Instant::now();
        let mut frame = 0u64;
        let mut bins = [0.0f32; BIN_COUNT];

        while worker_running.load(Ordering::Relaxed) {
            fill_bins(frame, &mut bins);
            let phase = frame as f32 / 17.0;
            let level = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let mode = if (frame / 80) % 2 == 0 {
                "QOpenGLPaintDevice"
            } else {
                "warm glass bars"
            };
            let status = CString::new(format!(
                "{mode} · {:.0}% level · {} bins · Rust->C++ in-process",
                level * 100.0,
                BIN_COUNT
            ))
            .expect("status text should be valid");

            unsafe {
                nuxglit_set_level(level);
                nuxglit_set_status(status.as_ptr());
                nuxglit_set_bins(bins.as_ptr(), bins.len());
            }

            if let Some(limit) = autostop_ms {
                if started.elapsed() >= Duration::from_millis(limit) {
                    unsafe { nuxglit_request_quit() };
                    break;
                }
            }

            frame += 1;
            thread::sleep(Duration::from_millis(33));
        }
    });

    let exit_code = unsafe { nuxglit_run() };
    running.store(false, Ordering::Relaxed);
    let _ = producer.join();

    if exit_code == 0 {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Qt app exited with status {exit_code}"))
    }
}
