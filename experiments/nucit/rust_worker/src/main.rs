use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Command {
    #[serde(rename = "type")]
    command_type: String,
    level: Option<f32>,
}

#[derive(Debug, Serialize)]
struct Snapshot<'a> {
    paused: bool,
    frames_seen: u64,
    analysis: &'a str,
    advice: &'a str,
}

fn main() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    let mut paused = false;
    let mut frames_seen = 0u64;
    let mut smoothed_level = 0.0f32;
    let mut last_emit = Instant::now();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let command: Command = match serde_json::from_str(&line) {
            Ok(command) => command,
            Err(error) => {
                eprintln!("nucit worker: ignored malformed command: {error}");
                continue;
            }
        };

        match command.command_type.as_str() {
            "audio_frame" => {
                let level = command.level.unwrap_or(0.0).clamp(0.0, 1.0);
                smoothed_level = (smoothed_level * 0.8) + (level * 0.2);
                frames_seen = frames_seen.saturating_add(1);
            }
            "toggle_pause" => paused = !paused,
            "quit" => break,
            _ => {}
        }

        if last_emit.elapsed() >= Duration::from_millis(50) || command.command_type == "quit" {
            let analysis = if paused {
                "worker paused: audio loop is still in C++, but interpretation is suspended"
            } else if smoothed_level > 0.55 {
                "worker sees energetic local frames from the C++ side"
            } else {
                "worker sees a calmer frame stream from the C++ side"
            };
            let advice = "C++ owns the meter and fake callback; Rust only owns interpretation";
            let snapshot = Snapshot {
                paused,
                frames_seen,
                analysis,
                advice,
            };
            serde_json::to_writer(&mut out, &snapshot)?;
            out.write_all(b"\n")?;
            out.flush()?;
            last_emit = Instant::now();
        }
    }

    Ok(())
}
