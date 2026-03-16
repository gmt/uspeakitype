use std::io::{self, Stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{
    self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use parking_lot::Mutex;

use crate::cpl::{ControlId, RuntimeControls};
use crate::spectrum::{ColorScheme, FlameScheme};
use crate::FrameSnapshot;

#[derive(Default)]
pub(crate) struct AnsiState {
    inner: Mutex<AnsiSnapshot>,
}

#[derive(Clone, Default)]
struct AnsiSnapshot {
    frame: FrameSnapshot,
    status: String,
}

impl AnsiState {
    pub(crate) fn publish(&self, frame: FrameSnapshot, status: &str) {
        let mut inner = self.inner.lock();
        inner.frame = frame;
        inner.status.clear();
        inner.status.push_str(status);
    }

    fn snapshot(&self) -> AnsiSnapshot {
        self.inner.lock().clone()
    }
}

pub(crate) fn run(
    state: Arc<AnsiState>,
    controls: Arc<RuntimeControls>,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;

    while running.load(Ordering::Relaxed) {
        while event::poll(Duration::from_millis(1))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        if controls.is_open() && matches!(key.code, KeyCode::Esc) {
                            controls.close_panel();
                        } else {
                            running.store(false, Ordering::Relaxed);
                        }
                    }
                    KeyCode::Char('c') => {
                        controls.toggle_panel();
                    }
                    KeyCode::Up if controls.is_open() => {
                        controls.focus_previous();
                    }
                    KeyCode::Down if controls.is_open() => {
                        controls.focus_next();
                    }
                    KeyCode::Left if controls.is_open() => {
                        controls.adjust_selected(-1);
                    }
                    KeyCode::Right if controls.is_open() => {
                        controls.adjust_selected(1);
                    }
                    KeyCode::Enter | KeyCode::Char(' ') if controls.is_open() => {
                        controls.activate_selected();
                    }
                    _ => {}
                },
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        render(&mut terminal.stdout, &state.snapshot(), &controls)?;
        terminal.stdout.flush()?;
        std::thread::sleep(Duration::from_millis(33));
    }

    Ok(())
}

struct TerminalGuard {
    stdout: Stdout,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        queue!(stdout, EnterAlternateScreen, Hide)?;
        stdout.flush()?;
        Ok(Self { stdout })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = queue!(self.stdout, Show, LeaveAlternateScreen, ResetColor);
        let _ = self.stdout.flush();
        let _ = disable_raw_mode();
    }
}

fn render(stdout: &mut Stdout, snapshot: &AnsiSnapshot, controls: &RuntimeControls) -> Result<()> {
    let (cols, rows) = terminal::size()?;
    queue!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;

    if cols < 12 || rows < 6 {
        queue!(
            stdout,
            MoveTo(0, 0),
            Print(truncate(&snapshot.status, cols as usize))
        )?;
        return Ok(());
    }

    let panel_width = cols.saturating_sub(4).max(10);
    let panel_height = rows.saturating_sub(4).max(4);
    let panel_x = (cols.saturating_sub(panel_width)) / 2;
    let panel_y = (rows.saturating_sub(panel_height)) / 2;

    draw_border(stdout, panel_x, panel_y, panel_width, panel_height)?;

    let title = " usit ansi ";
    queue!(stdout, MoveTo(panel_x + 2, panel_y), Print(title))?;

    let control_snapshot = controls.snapshot();
    let status_y = panel_y + panel_height.saturating_sub(2);
    let status_text = if control_snapshot.panel_open {
        format!(
            "{} · c toggles controls · ↑↓ focus · ←→ gain · enter toggles · q quits",
            snapshot.status
        )
    } else {
        format!("{} · c controls · q quits", snapshot.status)
    };
    queue!(
        stdout,
        MoveTo(panel_x + 2, status_y),
        Print(truncate(
            &status_text,
            panel_width.saturating_sub(4) as usize
        ))
    )?;

    let viz_x = panel_x + 2;
    let viz_y = panel_y + 2;
    let viz_width = panel_width.saturating_sub(4);
    let viz_height = if control_snapshot.panel_open {
        panel_height.saturating_sub(13)
    } else {
        panel_height.saturating_sub(5)
    };
    if viz_width > 0 && viz_height > 0 {
        draw_bars(
            stdout,
            viz_x,
            viz_y,
            viz_width as usize,
            viz_height as usize,
            &snapshot.frame,
        )?;
    }

    if control_snapshot.panel_open && panel_height > 10 {
        let cpl_height = 7;
        let cpl_y = panel_y + panel_height.saturating_sub(cpl_height + 3);
        draw_border(
            stdout,
            panel_x + 2,
            cpl_y,
            panel_width.saturating_sub(4),
            cpl_height,
        )?;
        draw_controls(
            stdout,
            panel_x + 4,
            cpl_y + 1,
            panel_width.saturating_sub(8) as usize,
            &control_snapshot,
        )?;
    }

    Ok(())
}

fn draw_controls(
    stdout: &mut Stdout,
    x: u16,
    y: u16,
    width: usize,
    snapshot: &crate::cpl::RuntimeControlSnapshot,
) -> Result<()> {
    let controls = [
        (
            ControlId::Pause,
            format!(
                "{}: {}",
                ControlId::Pause.title(),
                if snapshot.paused {
                    "paused"
                } else {
                    "listening"
                }
            ),
        ),
        (
            ControlId::AutoGain,
            format!(
                "{}: {}",
                ControlId::AutoGain.title(),
                if snapshot.auto_gain_enabled {
                    "on"
                } else {
                    "off"
                }
            ),
        ),
        (
            ControlId::Gain,
            format!(
                "{}: {:.1}x{}",
                ControlId::Gain.title(),
                snapshot.manual_gain,
                if snapshot.auto_gain_enabled {
                    format!(" (active {:.1}x)", snapshot.current_gain)
                } else {
                    String::new()
                }
            ),
        ),
    ];

    for (index, (control, line)) in controls.iter().enumerate() {
        let selected = *control == snapshot.selected_control;
        queue!(
            stdout,
            MoveTo(x, y + index as u16),
            SetForegroundColor(if selected {
                Color::Rgb {
                    r: 242,
                    g: 215,
                    b: 122,
                }
            } else {
                Color::Rgb {
                    r: 200,
                    g: 176,
                    b: 141,
                }
            }),
            Print(truncate(
                &format!("{} {}", if selected { ">" } else { " " }, line),
                width
            )),
            ResetColor
        )?;
    }

    queue!(
        stdout,
        MoveTo(x, y + 4),
        SetForegroundColor(Color::Rgb {
            r: 160,
            g: 136,
            b: 110
        }),
        Print(truncate(
            &format!(
                "{} · {}",
                snapshot.source_label,
                snapshot.selected_control.help()
            ),
            width
        )),
        ResetColor
    )?;

    Ok(())
}

fn draw_border(stdout: &mut Stdout, x: u16, y: u16, width: u16, height: u16) -> Result<()> {
    if width < 2 || height < 2 {
        return Ok(());
    }

    queue!(stdout, MoveTo(x, y), Print("╭"))?;
    for dx in 1..width.saturating_sub(1) {
        queue!(stdout, MoveTo(x + dx, y), Print("─"))?;
    }
    queue!(stdout, MoveTo(x + width - 1, y), Print("╮"))?;

    for dy in 1..height.saturating_sub(1) {
        queue!(stdout, MoveTo(x, y + dy), Print("│"))?;
        queue!(stdout, MoveTo(x + width - 1, y + dy), Print("│"))?;
    }

    queue!(stdout, MoveTo(x, y + height - 1), Print("╰"))?;
    for dx in 1..width.saturating_sub(1) {
        queue!(stdout, MoveTo(x + dx, y + height - 1), Print("─"))?;
    }
    queue!(stdout, MoveTo(x + width - 1, y + height - 1), Print("╯"))?;
    Ok(())
}

fn draw_bars(
    stdout: &mut Stdout,
    x: u16,
    y: u16,
    width: usize,
    height: usize,
    frame: &FrameSnapshot,
) -> Result<()> {
    const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let scheme = FlameScheme;

    for col in 0..width {
        let intensity = sample_bin(&frame.bins, col, width).clamp(0.0, 1.0);
        let color = scheme.color_for_intensity(intensity);
        let term_color = Color::Rgb {
            r: (color.r * 255.0) as u8,
            g: (color.g * 255.0) as u8,
            b: (color.b * 255.0) as u8,
        };

        for row in (0..height).rev() {
            let threshold = ((height - 1 - row) as f32 + 0.5) / height as f32;
            let fill = ((intensity - threshold) * height as f32 + 0.5).clamp(0.0, 1.0);
            let mut idx = (fill * (BLOCKS.len() - 1) as f32).round() as usize;
            idx = idx.min(BLOCKS.len() - 1);
            if intensity > 0.0 && idx == 0 {
                idx = 1;
            }

            queue!(
                stdout,
                MoveTo(x + col as u16, y + row as u16),
                SetForegroundColor(term_color),
                Print(BLOCKS[idx]),
                ResetColor
            )?;
        }
    }

    Ok(())
}

fn sample_bin(bins: &[f32], col: usize, width: usize) -> f32 {
    if bins.is_empty() {
        return 0.0;
    }
    if width <= 1 || bins.len() == 1 {
        return bins[0];
    }

    let position = col as f32 * (bins.len() - 1) as f32 / (width - 1) as f32;
    let left = position.floor() as usize;
    let right = position.ceil() as usize;
    if left == right {
        return bins[left];
    }
    let t = position - left as f32;
    bins[left] + (bins[right] - bins[left]) * t
}

fn truncate(text: &str, max_width: usize) -> String {
    text.chars().take(max_width).collect()
}

#[cfg(test)]
mod tests {
    use super::sample_bin;

    #[test]
    fn sample_bin_interpolates() {
        let bins = [0.0, 1.0];
        assert_eq!(sample_bin(&bins, 0, 3), 0.0);
        assert!((sample_bin(&bins, 1, 3) - 0.5).abs() < 0.001);
        assert_eq!(sample_bin(&bins, 2, 3), 1.0);
    }
}
