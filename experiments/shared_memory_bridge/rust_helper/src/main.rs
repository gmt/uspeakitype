use std::ffi::CString;
use std::mem::size_of;
use std::ptr::NonNull;
use std::slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use libc::{
    c_void, close, ftruncate, mmap, munmap, shm_open, MAP_FAILED, MAP_SHARED, O_CREAT, O_RDWR,
    PROT_READ, PROT_WRITE,
};

const MAGIC: &[u8; 8] = b"USITSHM\0";
const VERSION: u32 = 1;
const REGION_SIZE: usize = 576;
const DEFAULT_SHM_NAME: &str = "/usit-shm-demo";
const COMMAND_NONE: u32 = 0;
const COMMAND_TOGGLE_PAUSE: u32 = 1;
const COMMAND_TOGGLE_INJECTION: u32 = 2;
const COMMAND_SET_GAIN: u32 = 3;
const COMMAND_QUIT: u32 = 4;

#[repr(C)]
struct BridgeLayout {
    magic: [u8; 8],
    version: u32,
    reserved0: u32,
    snapshot_seq: u64,
    command_seq: u64,
    last_applied_command_seq: u64,
    level: f32,
    peak: f32,
    gain: f32,
    paused: u8,
    injection_enabled: u8,
    quit_requested: u8,
    reserved1: u8,
    pending_command: u32,
    pending_value: f32,
    committed: [u8; 128],
    partial: [u8; 128],
    source_label: [u8; 64],
    model_label: [u8; 64],
    error_label: [u8; 128],
}

const _: [(); REGION_SIZE] = [(); size_of::<BridgeLayout>()];

fn main() {
    if let Err(error) = run() {
        eprintln!("usit-shm-helper: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let shm_name = parse_arg("--shm-name").unwrap_or_else(|| DEFAULT_SHM_NAME.to_string());
    let running = Arc::new(AtomicBool::new(true));

    {
        let running = running.clone();
        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
        })
        .map_err(|error| format!("installing ctrl-c handler: {error}"))?;
    }

    let mapping = SharedMapping::create(&shm_name)?;
    let bridge = mapping.bridge_mut();
    initialize_bridge(bridge);

    let start = Instant::now();
    let mut t = 0.0f32;

    while running.load(Ordering::Relaxed) && bridge.quit_requested == 0 {
        apply_command(bridge);

        let amplitude = if bridge.paused == 0 { 0.7 } else { 0.18 };
        let level = ((t * 1.7).sin().abs() * 0.6 + (t * 0.35).cos().abs() * 0.4) * amplitude;
        bridge.level = level.clamp(0.0, 1.0);
        bridge.peak = bridge.peak.max(bridge.level * 0.97 + 0.03);

        let elapsed = start.elapsed().as_secs_f32();
        let phrase = if elapsed < 3.0 {
            ("warming up the shared memory shell", "hello from rust")
        } else if elapsed < 6.0 {
            ("qt is polling a pod region instead of json", "level meter demo")
        } else {
            (
                "commands feel mailbox-y when the wire is just shared memory",
                "rough draft only",
            )
        };

        write_text(&mut bridge.committed, phrase.0);
        write_text(&mut bridge.partial, phrase.1);
        write_text(
            &mut bridge.source_label,
            if bridge.paused == 0 {
                "Source: Fake loopback"
            } else {
                "Source: Fake loopback (paused)"
            },
        );
        write_text(&mut bridge.model_label, "Model: moonshine-base (pretend)");
        write_text(&mut bridge.error_label, "");
        bridge.snapshot_seq = bridge.snapshot_seq.wrapping_add(1);

        t += 0.033 * bridge.gain.max(0.25);
        thread::sleep(Duration::from_millis(33));
    }

    Ok(())
}

fn parse_arg(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn initialize_bridge(bridge: &mut BridgeLayout) {
    bridge.magic = *MAGIC;
    bridge.version = VERSION;
    bridge.reserved0 = 0;
    bridge.snapshot_seq = 0;
    bridge.command_seq = 0;
    bridge.last_applied_command_seq = 0;
    bridge.level = 0.0;
    bridge.peak = 0.0;
    bridge.gain = 1.0;
    bridge.paused = 0;
    bridge.injection_enabled = 1;
    bridge.quit_requested = 0;
    bridge.reserved1 = 0;
    bridge.pending_command = COMMAND_NONE;
    bridge.pending_value = 1.0;
    write_text(&mut bridge.committed, "waiting for qt shell");
    write_text(&mut bridge.partial, "shared memory demo");
    write_text(&mut bridge.source_label, "Source: Fake loopback");
    write_text(&mut bridge.model_label, "Model: moonshine-base (pretend)");
    write_text(&mut bridge.error_label, "");
}

fn apply_command(bridge: &mut BridgeLayout) {
    if bridge.command_seq <= bridge.last_applied_command_seq {
        return;
    }

    match bridge.pending_command {
        COMMAND_TOGGLE_PAUSE => {
            bridge.paused = if bridge.paused == 0 { 1 } else { 0 };
        }
        COMMAND_TOGGLE_INJECTION => {
            bridge.injection_enabled = if bridge.injection_enabled == 0 { 1 } else { 0 };
        }
        COMMAND_SET_GAIN => {
            bridge.gain = bridge.pending_value.clamp(0.5, 2.0);
        }
        COMMAND_QUIT => {
            bridge.quit_requested = 1;
        }
        _ => {}
    }

    bridge.last_applied_command_seq = bridge.command_seq;
}

fn write_text<const N: usize>(slot: &mut [u8; N], text: &str) {
    slot.fill(0);
    let bytes = text.as_bytes();
    let len = bytes.len().min(N.saturating_sub(1));
    slot[..len].copy_from_slice(&bytes[..len]);
}

struct SharedMapping {
    ptr: NonNull<BridgeLayout>,
}

impl SharedMapping {
    fn create(name: &str) -> Result<Self, String> {
        let cname =
            CString::new(name).map_err(|_| "shared memory name contains interior nul".to_string())?;
        let fd = unsafe { shm_open(cname.as_ptr(), O_CREAT | O_RDWR, 0o600) };
        if fd < 0 {
            return Err(format!(
                "shm_open failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let result = unsafe { ftruncate(fd, REGION_SIZE as i64) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                close(fd);
            }
            return Err(format!("ftruncate failed: {error}"));
        }

        let raw = unsafe {
            mmap(
                std::ptr::null_mut(),
                REGION_SIZE,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                fd,
                0,
            )
        };
        let map_error = std::io::Error::last_os_error();
        unsafe {
            close(fd);
        }

        if raw == MAP_FAILED {
            return Err(format!("mmap failed: {map_error}"));
        }

        let ptr = NonNull::new(raw.cast::<BridgeLayout>())
            .ok_or_else(|| "mmap returned null".to_string())?;

        unsafe {
            let region = slice::from_raw_parts_mut(raw.cast::<u8>(), REGION_SIZE);
            region.fill(0);
        }

        Ok(Self { ptr })
    }

    fn bridge_mut(&self) -> &mut BridgeLayout {
        unsafe { self.ptr.as_ptr().as_mut().unwrap() }
    }
}

impl Drop for SharedMapping {
    fn drop(&mut self) {
        unsafe {
            munmap(self.ptr.as_ptr().cast::<c_void>(), REGION_SIZE);
        }
    }
}

mod ctrlc {
    pub use ctrlc::*;
}
