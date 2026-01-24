//! Spectrogram widget visual verification spike
//!
//! Standalone ratatui example with deterministic fake data for visual testing.
//! Renders SpectrogramWidget with sine wave data that animates smoothly.
//!
//! Controls:
//! - 'q': Quit
//! - 'space': Pause/resume animation
//!
//! Run with: cargo run --example spectrogram_spike

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

// Import from barbara crate
use barbara::spectrum::FlameScheme;
use barbara::ui::spectrogram_widget::SpectrogramWidget;

/// Character set for bar meter visualization
const BLOCK_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Application state
struct App {
    /// Frequency band intensities [0.0, 1.0]
    bands: Vec<f32>,
    /// Animation phase (0.0 to 1.0)
    phase: f32,
    /// Whether animation is paused
    paused: bool,
    /// Time of last frame
    last_frame: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            bands: vec![0.0; 32],
            phase: 0.0,
            paused: false,
            last_frame: Instant::now(),
        }
    }

    /// Update animation phase and regenerate bands
    fn update(&mut self) {
        if self.paused {
            return;
        }

        let elapsed = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();

        // Advance phase at ~1 cycle per 4 seconds
        self.phase = (self.phase + elapsed * 0.25) % 1.0;

        // Generate deterministic sine wave data
        // Mix of multiple frequencies for visual interest
        let num_bands = self.bands.len();
        for (i, band) in self.bands.iter_mut().enumerate() {
            let normalized_idx = i as f32 / num_bands as f32;

            // Primary sine wave (varies with phase)
            let primary = (normalized_idx * std::f32::consts::PI * 2.0
                + self.phase * std::f32::consts::PI * 2.0)
                .sin();

            // Secondary harmonic (higher frequency)
            let harmonic = (normalized_idx * std::f32::consts::PI * 4.0
                + self.phase * std::f32::consts::PI * 4.0)
                .sin();

            // Combine: 70% primary, 30% harmonic
            let combined = primary * 0.7 + harmonic * 0.3;

            // Normalize to [0.0, 1.0]
            *band = (combined * 0.5 + 0.5).clamp(0.0, 1.0);
        }
    }

    /// Handle keyboard input
    fn handle_input(&mut self) -> bool {
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return false,
                    KeyCode::Char(' ') => self.paused = !self.paused,
                    _ => {}
                }
            }
        }
        true
    }
}

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new();

    // Main loop
    loop {
        // Update animation
        app.update();

        // Draw UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(3)])
                .split(f.area());

            // Spectrogram widget area
            let spec_area = chunks[0];
            let widget = SpectrogramWidget::new(&app.bands, &FlameScheme, &BLOCK_CHARS);
            f.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Spectrogram Spike (Deterministic Sine Wave)")
                    .title_alignment(Alignment::Center),
                spec_area,
            );

            // Render spectrogram inside the block (with padding)
            let inner = Rect {
                x: spec_area.x + 1,
                y: spec_area.y + 1,
                width: spec_area.width.saturating_sub(2),
                height: spec_area.height.saturating_sub(2),
            };
            f.render_widget(widget, inner);

            // Status line
            let status = if app.paused {
                "PAUSED - Press SPACE to resume, Q to quit"
            } else {
                "ANIMATING - Press SPACE to pause, Q to quit"
            };

            let status_line = Line::from(vec![Span::styled(
                status,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
            )]);

            let status_widget = Paragraph::new(status_line).alignment(Alignment::Center);

            f.render_widget(status_widget, chunks[1]);
        })?;

        // Handle input
        if !app.handle_input() {
            break;
        }
    }

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
