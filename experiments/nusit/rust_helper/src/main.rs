use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Command {
    #[serde(rename = "type")]
    command_type: String,
}

#[derive(Debug, Serialize)]
struct Snapshot<'a> {
    level: f32,
    paused: bool,
    injection_enabled: bool,
    status: &'a str,
    transcript: &'a str,
}

fn main() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::channel::<Command>();
    let reader_running = running.clone();

    thread::spawn(move || {
        let stdin = io::stdin();
        let locked = stdin.lock();
        for line in locked.lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<Command>(&line) {
                Ok(command) => {
                    if tx.send(command).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    eprintln!("nusit helper: ignored malformed command: {error}");
                }
            }
        }
        reader_running.store(false, Ordering::Relaxed);
    });

    let mut paused = false;
    let mut injection_enabled = true;
    let mut tick: u64 = 0;
    let mut last_emit = Instant::now();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    while running.load(Ordering::Relaxed) {
        while let Ok(command) = rx.try_recv() {
            match command.command_type.as_str() {
                "toggle_pause" => paused = !paused,
                "toggle_injection" => injection_enabled = !injection_enabled,
                "quit" => running.store(false, Ordering::Relaxed),
                _ => {}
            }
        }

        if last_emit.elapsed() >= Duration::from_millis(33) {
            let phase = tick as f32 * 0.09;
            let base = if paused { 0.08 } else { 0.28 };
            let level = (base + phase.sin().abs() * 0.68).clamp(0.0, 1.0);
            let status = if paused { "Paused" } else { "Listening" };
            let transcript = if paused {
                "shell owns the frame; rust helper is idling"
            } else if injection_enabled {
                "rough draft words appear here while the shell stays in C++"
            } else {
                "display only: helper still streams state but trusts nothing"
            };

            let snapshot = Snapshot {
                level,
                paused,
                injection_enabled,
                status,
                transcript,
            };
            serde_json::to_writer(&mut out, &snapshot)?;
            out.write_all(b"\n")?;
            out.flush()?;

            tick = tick.wrapping_add(1);
            last_emit = Instant::now();
        }

        thread::sleep(Duration::from_millis(8));
    }

    Ok(())
}
