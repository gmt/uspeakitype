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
}

impl StatusWidget {
    pub fn new(info: StatusInfo) -> Self {
        Self { info }
    }

    fn build_candidates(&self) -> Vec<String> {
        match self.info {
            StatusInfo::Demo => vec![
                "spc:pause  c:settings  w:viz  |  demo  |  q:quit".to_string(),
                "spc:pause c:settings w:viz | demo | q:quit".to_string(),
                "spc c:set w:viz demo q:quit".to_string(),
                "spc c w q".to_string(),
            ],
            StatusInfo::Live {
                sample_rate,
                channels,
            } => {
                let ch = if channels == 1 { "mono" } else { "stereo" };
                let rate_khz = sample_rate / 1000;
                vec![
                    format!(
                        "spc:pause  c:settings  w:viz  |  {}Hz {}  |  q:quit",
                        sample_rate, ch
                    ),
                    format!(
                        "spc:pause c:settings w:viz | {}Hz {} | q:quit",
                        sample_rate, ch
                    ),
                    format!("spc c:set w:viz {}kHz {} q:quit", rate_khz, ch),
                    format!("spc c:set w:viz {}k q:quit", rate_khz),
                    "spc c w q".to_string(),
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
