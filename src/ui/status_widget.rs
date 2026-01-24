//! Status line widget for keybindings help

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

#[derive(Debug, Clone)]
pub enum StatusInfo {
    Demo,
    Live { sample_rate: u32, channels: u32 },
}

pub struct StatusWidget {
    pub info: StatusInfo,
    pub is_paused: bool,
    pub is_speaking: bool,
}

impl StatusWidget {
    pub fn new(info: StatusInfo) -> Self {
        Self {
            info,
            is_paused: false,
            is_speaking: false,
        }
    }

    pub fn paused(mut self, paused: bool) -> Self {
        self.is_paused = paused;
        self
    }

    pub fn speaking(mut self, speaking: bool) -> Self {
        self.is_speaking = speaking;
        self
    }

    fn icon_and_color(&self) -> (&'static str, Color) {
        if self.is_paused {
            ("‖", Color::Yellow)
        } else if self.is_speaking {
            ("●", Color::Red)
        } else {
            ("▶", Color::Green)
        }
    }

    fn build_candidates(&self) -> Vec<String> {
        match self.info {
            StatusInfo::Demo => vec![
                "  spc:pause  c:settings  w:viz  demo  q:quit".to_string(),
                " spc:pause c:settings w:viz demo q:quit".to_string(),
                " spc c:set w:viz demo q:quit".to_string(),
                " spc c w q".to_string(),
            ],
            StatusInfo::Live {
                sample_rate,
                channels,
            } => {
                let ch = if channels == 1 { "mono" } else { "stereo" };
                let rate_khz = sample_rate / 1000;
                vec![
                    format!(
                        "  spc:pause  c:settings  w:viz  {}Hz {}  q:quit",
                        sample_rate, ch
                    ),
                    format!(
                        " spc:pause c:settings w:viz {}Hz {} q:quit",
                        sample_rate, ch
                    ),
                    format!(" spc c:set w:viz {}kHz {} q:quit", rate_khz, ch),
                    format!(" spc c:set w:viz {}k q:quit", rate_khz),
                    " spc c w q".to_string(),
                ]
            }
        }
    }

    fn select_candidate(&self, candidates: &[String], icon_len: usize, max_width: usize) -> String {
        candidates
            .iter()
            .find(|s| s.chars().count() + icon_len <= max_width)
            .cloned()
            .unwrap_or_default()
    }
}

impl Widget for StatusWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (icon, color) = self.icon_and_color();
        let candidates = self.build_candidates();
        let rest = self.select_candidate(&candidates, icon.chars().count(), area.width as usize);

        if rest.is_empty() {
            return;
        }

        let line = Line::from(vec![
            Span::styled(icon, Style::default().fg(color)),
            Span::styled(rest, Style::default().fg(Color::DarkGray)),
        ]);

        let paragraph = Paragraph::new(line).alignment(Alignment::Center);
        paragraph.render(area, buf);
    }
}
