//! WGPU rendering
//!
//! TODO: Port from sonori/src/ui/
//! - window.rs (WGPU setup)
//! - text_renderer.rs (glyphon)
//! - rounded_rect.wgsl (background)
//!
//! Simplifications from sonori:
//! - No spectrogram
//! - No button system  
//! - No scrollbar (for now)
//! - Just: rounded rect background + text with partial/committed distinction

pub struct Renderer {
    // TODO: WGPU device, queue, text renderer
}

impl Renderer {
    pub fn new() -> anyhow::Result<Self> {
        todo!()
    }

    pub fn render(&mut self, committed: &str, partial: &str) -> anyhow::Result<()> {
        // committed: normal color
        // partial: dimmed or italic
        todo!()
    }
}
