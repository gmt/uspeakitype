//! Widget test harness - visual verification of all new ratatui widgets
//!
//! Displays all 4 widgets in a vertical layout with deterministic test data:
//! - SpectrogramWidget: bar meter with animated sine wave
//! - WaterfallWidget: scrolling time-history spectrogram
//! - StatusWidget: centered keybindings help line
//! - TranscriptWidget: two-tone committed/partial text
//!
//! Controls:
//! - 'q': Quit
//! - 'space': Pause/resume animation
//! - 'w': Toggle between bar meter and waterfall
//!
//! Run with: cargo run --example widget_test

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
use barbara::spectrum::{FlameScheme, WaterfallHistory};
use barbara::ui::spectrogram_widget::SpectrogramWidget;
use barbara::ui::status_widget::{StatusInfo, StatusWidget};
use barbara::ui::theme::DEFAULT_THEME;
use barbara::ui::transcript_widget::TranscriptWidget;
use barbara::ui::waterfall_widget::WaterfallWidget;

/// Character set for bar meter visualization
const BLOCK_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Application state
struct App {
    /// Frequency band intensities [0.0, 1.0] for bar meter
    bands: Vec<f32>,
    /// Waterfall history for scrolling spectrogram
    waterfall: WaterfallHistory,
    /// Animation phase (0.0 to 1.0)
    phase: f32,
    /// Whether animation is paused
    paused: bool,
    /// Time of last frame
    last_frame: Instant,
    /// Whether to show waterfall (true) or bar meter (false)
    show_waterfall: bool,
    /// Committed transcript text
    committed: String,
    /// Partial transcript text
    partial: String,
}

impl App {
    fn new() -> Self {
        Self {
            bands: vec![0.0; 32],
            waterfall: WaterfallHistory::new(80, 24),
            phase: 0.0,
            paused: false,
            last_frame: Instant::now(),
            show_waterfall: false,
            committed: "hello world this is".to_string(),
            partial: "transcribed text".to_string(),
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

        // Push current bands to waterfall history
        self.waterfall.push(&self.bands);
    }

    /// Handle keyboard input
    fn handle_input(&mut self) -> bool {
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return false,
                    KeyCode::Char(' ') => self.paused = !self.paused,
                    KeyCode::Char('w') => self.show_waterfall = !self.show_waterfall,
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
                .constraints([
                    Constraint::Length(10), // Spectrogram/Waterfall
                    Constraint::Length(10), // Waterfall (if showing bar) or extra space
                    Constraint::Length(1),  // Status
                    Constraint::Length(1),  // Transcript
                    Constraint::Fill(1),    // Padding
                ])
                .split(f.area());

            // Render visualization (bar meter or waterfall)
            let viz_title = if app.show_waterfall {
                "Waterfall Spectrogram (Deterministic Sine Wave)"
            } else {
                "Bar Meter Spectrogram (Deterministic Sine Wave)"
            };

            f.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(viz_title)
                    .title_alignment(Alignment::Center),
                chunks[0],
            );

            let inner = Rect {
                x: chunks[0].x + 1,
                y: chunks[0].y + 1,
                width: chunks[0].width.saturating_sub(2),
                height: chunks[0].height.saturating_sub(2),
            };

            if app.show_waterfall {
                let waterfall_widget =
                    WaterfallWidget::new(&app.waterfall, &FlameScheme, &BLOCK_CHARS);
                f.render_widget(waterfall_widget, inner);
            } else {
                let spectrogram_widget =
                    SpectrogramWidget::new(&app.bands, &FlameScheme, &BLOCK_CHARS);
                f.render_widget(spectrogram_widget, inner);
            }

            // Render second visualization area (for comparison)
            f.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(if app.show_waterfall {
                        "Bar Meter (Reference)"
                    } else {
                        "Waterfall (Reference)"
                    })
                    .title_alignment(Alignment::Center),
                chunks[1],
            );

            let inner2 = Rect {
                x: chunks[1].x + 1,
                y: chunks[1].y + 1,
                width: chunks[1].width.saturating_sub(2),
                height: chunks[1].height.saturating_sub(2),
            };

            if app.show_waterfall {
                let spectrogram_widget =
                    SpectrogramWidget::new(&app.bands, &FlameScheme, &BLOCK_CHARS);
                f.render_widget(spectrogram_widget, inner2);
            } else {
                let waterfall_widget =
                    WaterfallWidget::new(&app.waterfall, &FlameScheme, &BLOCK_CHARS);
                f.render_widget(waterfall_widget, inner2);
            }

            // Render status widget
            let status_widget = StatusWidget::new(StatusInfo::Live {
                sample_rate: 16000,
                channels: 1,
            });
            f.render_widget(status_widget, chunks[2]);

            // Render transcript widget
            let transcript_widget = TranscriptWidget::new(
                &app.committed,
                &app.partial,
                DEFAULT_THEME,
                chunks[3].width as usize,
            );
            f.render_widget(transcript_widget, chunks[3]);

            // Help text
            let help_text = if app.paused {
                "PAUSED - Press SPACE to resume, W to toggle viz, Q to quit"
            } else {
                "ANIMATING - Press SPACE to pause, W to toggle viz, Q to quit"
            };

            let help_line = Line::from(vec![Span::styled(
                help_text,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
            )]);

            let help_widget = Paragraph::new(help_line).alignment(Alignment::Center);

            f.render_widget(help_widget, chunks[4]);
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
