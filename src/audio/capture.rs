//! Audio capture from microphone
//! 
//! TODO: Port from sonori/src/audio_capture.rs
//! Key pieces to bring over:
//! - PortAudio stream setup
//! - 16kHz mono capture
//! - Ring buffer for samples

pub struct AudioCapture {
    // TODO
}

impl AudioCapture {
    pub fn new() -> anyhow::Result<Self> {
        todo!("Port from sonori")
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        todo!()
    }

    pub fn stop(&mut self) -> anyhow::Result<()> {
        todo!()
    }
}
