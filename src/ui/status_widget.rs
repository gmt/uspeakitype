//! Status line widget for keybindings help

use ratatui::{
    buffer::Buffer, layout::Alignment, layout::Rect, prelude::Stylize, style::Style,
    widgets::Paragraph, widgets::Widget,
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

    fn build_candidates(&self) -> Vec<String> {
        let icon = if self.is_paused {
            "‖"
        } else if self.is_speaking {
            "●"
        } else {
            "▶"
        };

        match self.info {
            StatusInfo::Demo => vec![
                format!("{}  spc:pause  c:settings  w:viz  demo  q:quit", icon),
                format!("{} spc:pause c:settings w:viz demo q:quit", icon),
                format!("{} spc c:set w:viz demo q:quit", icon),
                format!("{} spc c w q", icon),
            ],
            StatusInfo::Live {
                sample_rate,
                channels,
            } => {
                let ch = if channels == 1 { "mono" } else { "stereo" };
                let rate_khz = sample_rate / 1000;
                vec![
                    format!(
                        "{}  spc:pause  c:settings  w:viz  {}Hz {}  q:quit",
                        icon, sample_rate, ch
                    ),
                    format!(
                        "{} spc:pause c:settings w:viz {}Hz {} q:quit",
                        icon, sample_rate, ch
                    ),
                    format!("{} spc c:set w:viz {}kHz {} q:quit", icon, rate_khz, ch),
                    format!("{} spc c:set w:viz {}k q:quit", icon, rate_khz),
                    format!("{} spc c w q", icon),
                ]
            }
        }
    }

    fn select_candidate(&self, candidates: &[String], max_width: usize) -> String {
        candidates
            .iter()
            .find(|s| s.chars().count() <= max_width)
            .cloned()
            .unwrap_or_default()
    }
}

impl Widget for StatusWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let candidates = self.build_candidates();
        let text = self.select_candidate(&candidates, area.width as usize);

        if text.is_empty() {
            return;
        }

        let paragraph = Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::new().dim());

        paragraph.render(area, buf);
    }
}
