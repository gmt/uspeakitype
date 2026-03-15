use anyhow::Result;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

unsafe extern "C" {
    fn hellnuxit_run() -> i32;
    fn hellnuxit_set_level(level: f32);
    fn hellnuxit_set_status(text: *const std::os::raw::c_char);
    fn hellnuxit_request_quit();
}

fn main() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let worker_running = running.clone();
    let autostop_ms = std::env::var("HELLNUXIT_AUTOSTOP_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());

    let producer = thread::spawn(move || {
        let started = Instant::now();
        let mut frame = 0u64;

        while worker_running.load(Ordering::Relaxed) {
            let phase = frame as f32 / 9.0;
            let level = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let status = CString::new(format!(
                "raw C ABI bridge · {:.0}% level · fake-model-0",
                level * 100.0
            ))
            .expect("status text should be valid");

            unsafe {
                hellnuxit_set_level(level);
                hellnuxit_set_status(status.as_ptr());
            }

            if let Some(limit) = autostop_ms {
                if started.elapsed() >= Duration::from_millis(limit) {
                    unsafe { hellnuxit_request_quit() };
                    break;
                }
            }

            frame += 1;
            thread::sleep(Duration::from_millis(33));
        }
    });

    let exit_code = unsafe { hellnuxit_run() };
    running.store(false, Ordering::Relaxed);
    let _ = producer.join();

    if exit_code == 0 {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Qt app exited with status {exit_code}"))
    }
}
