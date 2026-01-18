//! PipeWire audio capture with device selection, pause, and auto-gain

use std::convert::TryInto;
use std::mem;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use parking_lot::RwLock;
use pipewire as pw;
use pw::spa;
use pw::spa::param::audio::{AudioFormat, AudioInfoRaw};
use pw::spa::param::format::{MediaSubtype, MediaType};
use pw::spa::param::format_utils;
use pw::spa::pod::Pod;

pub type AudioCallback = Box<dyn Fn(&[f32]) + Send + 'static>;

#[derive(Debug, Clone)]
pub struct AudioSource {
    pub id: u32,
    pub name: String,
    pub description: String,
}

#[derive(Debug)]
pub struct CaptureConfig {
    pub auto_gain_enabled: bool,
    pub target_headroom: f32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            auto_gain_enabled: false,
            target_headroom: 0.8,
        }
    }
}

pub struct CaptureControl {
    pub paused: AtomicBool,
    pub auto_gain_enabled: AtomicBool,
    pub current_gain: std::sync::atomic::AtomicU32,
    running: AtomicBool,
}

impl CaptureControl {
    pub fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            auto_gain_enabled: AtomicBool::new(false),
            current_gain: AtomicU32::new(f32::to_bits(1.0)),
            running: AtomicBool::new(true),
        }
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn toggle_pause(&self) -> bool {
        let was_paused = self.paused.fetch_xor(true, Ordering::SeqCst);
        !was_paused
    }

    pub fn set_auto_gain(&self, enabled: bool) {
        self.auto_gain_enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn is_auto_gain_enabled(&self) -> bool {
        self.auto_gain_enabled.load(Ordering::SeqCst)
    }

    pub fn get_current_gain(&self) -> f32 {
        f32::from_bits(self.current_gain.load(Ordering::Relaxed))
    }

    fn set_current_gain(&self, gain: f32) {
        self.current_gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Default for CaptureControl {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AudioCapture {
    control: Arc<CaptureControl>,
    thread: Option<JoinHandle<()>>,
    sources: Arc<RwLock<Vec<AudioSource>>>,
}

impl AudioCapture {
    pub fn new(callback: AudioCallback, config: CaptureConfig) -> Result<Self> {
        let control = Arc::new(CaptureControl::new());
        control.set_auto_gain(config.auto_gain_enabled);

        let control_clone = control.clone();
        let sources = Arc::new(RwLock::new(Vec::new()));
        let sources_clone = sources.clone();

        let thread = thread::spawn(move || {
            if let Err(e) = run_capture_loop(control_clone, callback, sources_clone, config) {
                eprintln!("Audio capture error: {e}");
            }
        });

        Ok(Self {
            control,
            thread: Some(thread),
            sources,
        })
    }

    pub fn control(&self) -> &Arc<CaptureControl> {
        &self.control
    }

    pub fn sources(&self) -> Vec<AudioSource> {
        self.sources.read().clone()
    }

    pub fn stop(&mut self) {
        self.control.stop();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

struct UserData {
    format: AudioInfoRaw,
    callback: AudioCallback,
    control: Arc<CaptureControl>,
    peak_tracker: PeakTracker,
    target_headroom: f32,
}

struct PeakTracker {
    recent_peaks: [f32; 32],
    index: usize,
}

impl PeakTracker {
    fn new() -> Self {
        Self {
            recent_peaks: [0.0; 32],
            index: 0,
        }
    }

    fn update(&mut self, samples: &[f32]) -> f32 {
        let peak = samples
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, |a, b| a.max(b));
        self.recent_peaks[self.index] = peak;
        self.index = (self.index + 1) % self.recent_peaks.len();
        self.recent_peaks
            .iter()
            .cloned()
            .fold(0.0f32, |a, b| a.max(b))
    }
}

fn apply_auto_gain(
    samples: &mut [f32],
    control: &CaptureControl,
    peak_tracker: &mut PeakTracker,
    target_headroom: f32,
) {
    if !control.is_auto_gain_enabled() {
        return;
    }

    let recent_peak = peak_tracker.update(samples);
    if recent_peak < 0.001 {
        return;
    }

    let target_peak = target_headroom;
    let desired_gain = target_peak / recent_peak;
    let current_gain = control.get_current_gain();
    let new_gain = current_gain + (desired_gain - current_gain) * 0.1;
    let clamped_gain = new_gain.clamp(0.1, 10.0);

    control.set_current_gain(clamped_gain);

    for sample in samples.iter_mut() {
        *sample *= clamped_gain;
        *sample = sample.clamp(-1.0, 1.0);
    }
}

fn run_capture_loop(
    control: Arc<CaptureControl>,
    callback: AudioCallback,
    _sources: Arc<RwLock<Vec<AudioSource>>>,
    config: CaptureConfig,
) -> Result<()> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None).context("creating PipeWire main loop")?;
    let context =
        pw::context::ContextRc::new(&mainloop, None).context("creating PipeWire context")?;
    let core = context
        .connect_rc(None)
        .context("connecting to PipeWire server")?;

    let props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::NODE_NAME => "barbara-capture",
    };

    let stream =
        pw::stream::StreamBox::new(&core, "barbara-audio", props).context("creating stream")?;

    let user_data = UserData {
        format: AudioInfoRaw::new(),
        callback,
        control: control.clone(),
        peak_tracker: PeakTracker::new(),
        target_headroom: config.target_headroom,
    };

    let _listener = stream
        .add_local_listener_with_user_data(user_data)
        .param_changed(|_, user_data, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }

            let (media_type, media_subtype) = match format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };

            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }

            if user_data.format.parse(param).is_ok() {
                eprintln!(
                    "Audio format: {}Hz {}ch",
                    user_data.format.rate(),
                    user_data.format.channels()
                );
            }
        })
        .process(|stream, user_data| {
            if user_data.control.is_paused() {
                let _ = stream.dequeue_buffer();
                return;
            }

            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let data = &mut datas[0];
            let n_samples = data.chunk().size() / (mem::size_of::<f32>() as u32);

            if let Some(bytes) = data.data() {
                let mut samples = Vec::with_capacity(n_samples as usize);
                for n in 0..n_samples {
                    let start = n as usize * mem::size_of::<f32>();
                    let end = start + mem::size_of::<f32>();
                    if end <= bytes.len() {
                        let sample_bytes: [u8; 4] = bytes[start..end].try_into().unwrap();
                        samples.push(f32::from_le_bytes(sample_bytes));
                    }
                }

                apply_auto_gain(
                    &mut samples,
                    &user_data.control,
                    &mut user_data.peak_tracker,
                    user_data.target_headroom,
                );

                (user_data.callback)(&samples);
            }
        })
        .register()
        .context("registering stream listener")?;

    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(AudioFormat::F32LE);
    audio_info.set_rate(16000);
    audio_info.set_channels(1);

    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };

    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|e| anyhow::anyhow!("serializing audio format: {:?}", e))?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).unwrap()];

    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .context("connecting audio stream")?;

    let mainloop_weak = mainloop.downgrade();
    let control_for_timer = control.clone();
    let timer = mainloop.loop_().add_timer(move |_| {
        if !control_for_timer.is_running() {
            if let Some(ml) = mainloop_weak.upgrade() {
                ml.quit();
            }
        }
    });

    timer
        .update_timer(
            Some(std::time::Duration::from_millis(100)),
            Some(std::time::Duration::from_millis(100)),
        )
        .into_result()
        .context("starting timer")?;

    mainloop.run();

    Ok(())
}

pub fn list_audio_sources() -> Result<Vec<AudioSource>> {
    use std::cell::RefCell;
    use std::rc::Rc;

    pw::init();

    let sources: Rc<RefCell<Vec<AudioSource>>> = Rc::new(RefCell::new(Vec::new()));
    let done = Rc::new(RefCell::new(false));

    let mainloop = pw::main_loop::MainLoopRc::new(None).context("creating PipeWire main loop")?;
    let context =
        pw::context::ContextRc::new(&mainloop, None).context("creating PipeWire context")?;
    let core = context
        .connect_rc(None)
        .context("connecting to PipeWire server")?;

    let registry = core.get_registry_rc().context("getting registry")?;

    let sources_clone = sources.clone();
    let done_clone = done.clone();
    let mainloop_weak = mainloop.downgrade();

    let _registry_listener = registry
        .add_listener_local()
        .global(move |obj| {
            if obj.type_ == pw::types::ObjectType::Node {
                if let Some(props) = &obj.props {
                    let media_class = props.get("media.class").unwrap_or("");
                    if media_class.contains("Source") || media_class.contains("Input") {
                        let name = props.get("node.name").unwrap_or("unknown").to_string();
                        let description = props
                            .get("node.description")
                            .or_else(|| props.get("node.nick"))
                            .unwrap_or(&name)
                            .to_string();

                        sources_clone.borrow_mut().push(AudioSource {
                            id: obj.id,
                            name,
                            description,
                        });
                    }
                }
            }
        })
        .register();

    let mainloop_weak2 = mainloop.downgrade();
    let _core_listener = core
        .add_listener_local()
        .done(move |_id, _seq| {
            *done_clone.borrow_mut() = true;
            if let Some(ml) = mainloop_weak2.upgrade() {
                ml.quit();
            }
        })
        .register();

    core.sync(0).context("syncing core")?;

    let done_for_timer = done.clone();
    let timer = mainloop.loop_().add_timer(move |_| {
        if *done_for_timer.borrow() {
            if let Some(ml) = mainloop_weak.upgrade() {
                ml.quit();
            }
        }
    });

    timer
        .update_timer(
            Some(std::time::Duration::from_millis(100)),
            Some(std::time::Duration::from_millis(500)),
        )
        .into_result()
        .context("starting timer")?;

    mainloop.run();

    let result = sources.borrow().clone();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_control_default_state() {
        let control = CaptureControl::new();
        assert!(!control.is_paused());
        assert!(!control.is_auto_gain_enabled());
        assert!(control.is_running());
        assert!((control.get_current_gain() - 1.0).abs() < 0.001);
    }

    #[test]
    fn capture_control_pause_toggle() {
        let control = CaptureControl::new();
        assert!(!control.is_paused());

        let now_paused = control.toggle_pause();
        assert!(now_paused);
        assert!(control.is_paused());

        let now_paused = control.toggle_pause();
        assert!(!now_paused);
        assert!(!control.is_paused());
    }

    #[test]
    fn capture_control_auto_gain_toggle() {
        let control = CaptureControl::new();
        assert!(!control.is_auto_gain_enabled());

        control.set_auto_gain(true);
        assert!(control.is_auto_gain_enabled());

        control.set_auto_gain(false);
        assert!(!control.is_auto_gain_enabled());
    }

    #[test]
    fn peak_tracker_tracks_maximum() {
        let mut tracker = PeakTracker::new();

        let peak1 = tracker.update(&[0.1, 0.2, 0.3]);
        assert!((peak1 - 0.3).abs() < 0.001);

        let peak2 = tracker.update(&[0.5, 0.1, 0.1]);
        assert!((peak2 - 0.5).abs() < 0.001);

        let peak3 = tracker.update(&[0.1, 0.1, 0.1]);
        assert!((peak3 - 0.5).abs() < 0.001);
    }

    #[test]
    fn capture_config_defaults() {
        let config = CaptureConfig::default();
        assert!(!config.auto_gain_enabled);
        assert!((config.target_headroom - 0.8).abs() < 0.001);
    }
}
