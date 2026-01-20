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

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub auto_gain_enabled: bool,
    pub agc: AgcConfig,
    pub source: Option<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            auto_gain_enabled: false,
            agc: AgcConfig::default(),
            source: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgcConfig {
    pub desired_rms: f32,
    pub smoothing_factor: f32,
    pub max_gain: f32,
    pub min_gain: f32,
}

impl Default for AgcConfig {
    fn default() -> Self {
        Self {
            desired_rms: 0.1,
            smoothing_factor: 0.0001,
            max_gain: 10.0,
            min_gain: 0.1,
        }
    }
}

pub struct CaptureControl {
    pub paused: AtomicBool,
    pub auto_gain_enabled: AtomicBool,
    pub current_gain: std::sync::atomic::AtomicU32,
    running: AtomicBool,
    sample_rate: AtomicU32,
    channels: AtomicU32,
}

impl CaptureControl {
    pub fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            auto_gain_enabled: AtomicBool::new(false),
            current_gain: AtomicU32::new(f32::to_bits(1.0)),
            running: AtomicBool::new(true),
            sample_rate: AtomicU32::new(0),
            channels: AtomicU32::new(0),
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

    pub fn set_format(&self, sample_rate: u32, channels: u32) {
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
        self.channels.store(channels, Ordering::Relaxed);
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    pub fn channels(&self) -> u32 {
        self.channels.load(Ordering::Relaxed)
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
    agc: SpeechAgc,
}

struct SpeechAgc {
    config: AgcConfig,
    gain: f32,
    frozen: bool,
}

impl SpeechAgc {
    fn new(config: AgcConfig) -> Self {
        Self {
            config,
            gain: 1.0,
            frozen: false,
        }
    }

    fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
    }

    fn gain(&self) -> f32 {
        self.gain
    }

    fn process(&mut self, samples: &mut [f32]) {
        let desired_rms_squared = self.config.desired_rms * self.config.desired_rms;

        for sample in samples.iter_mut() {
            *sample *= self.gain;

            if !self.frozen {
                let sample_power = sample.powi(2);
                if sample_power > 1e-10 {
                    let ratio = sample_power / desired_rms_squared;
                    let adjustment = 1.0 + self.config.smoothing_factor * (1.0 - ratio);
                    self.gain *= adjustment;
                    self.gain = self.gain.clamp(self.config.min_gain, self.config.max_gain);
                }
            }

            *sample = sample.clamp(-1.0, 1.0);
        }
    }
}

fn apply_auto_gain(samples: &mut [f32], control: &CaptureControl, agc: &mut SpeechAgc) {
    if !control.is_auto_gain_enabled() {
        return;
    }

    let rms = (samples.iter().map(|s| s.powi(2)).sum::<f32>() / samples.len() as f32).sqrt();
    agc.set_frozen(rms < 0.01);

    agc.process(samples);
    control.set_current_gain(agc.gain());
}

fn resolve_source(source: &str) -> Result<u32> {
    if let Ok(id) = source.parse::<u32>() {
        return Ok(id);
    }

    let sources = list_audio_sources()?;
    sources
        .iter()
        .find(|s| s.name == source || s.description == source)
        .map(|s| s.id)
        .ok_or_else(|| anyhow::anyhow!("audio source not found: {}", source))
}

fn run_capture_loop(
    control: Arc<CaptureControl>,
    callback: AudioCallback,
    _sources: Arc<RwLock<Vec<AudioSource>>>,
    config: CaptureConfig,
) -> Result<()> {
    let target_id = config
        .source
        .as_ref()
        .map(|s| resolve_source(s))
        .transpose()?;

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
        agc: SpeechAgc::new(config.agc),
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
                let rate = user_data.format.rate();
                let channels = user_data.format.channels();
                user_data.control.set_format(rate, channels);
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
            let (offset, mut size) = {
                let chunk = data.chunk();
                (chunk.offset() as usize, chunk.size() as usize)
            };

            if let Some(bytes) = data.data() {
                if offset >= bytes.len() {
                    return;
                }

                if size == 0 {
                    size = bytes.len().saturating_sub(offset);
                }

                let end = offset.saturating_add(size).min(bytes.len());
                let payload = &bytes[offset..end];
                let n_samples = payload.len() / mem::size_of::<f32>();

                if n_samples == 0 {
                    return;
                }

                let mut samples = Vec::with_capacity(n_samples);
                for n in 0..n_samples {
                    let start = n * mem::size_of::<f32>();
                    let end = start + mem::size_of::<f32>();
                    let sample_bytes: [u8; 4] = payload[start..end].try_into().unwrap();
                    samples.push(f32::from_le_bytes(sample_bytes));
                }

                apply_auto_gain(&mut samples, &user_data.control, &mut user_data.agc);

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
            target_id,
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
    fn speech_agc_increases_gain_for_quiet_signal() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.01,
            max_gain: 10.0,
            min_gain: 0.1,
        };
        let mut agc = SpeechAgc::new(config);

        let mut samples = vec![0.01; 1000];
        agc.process(&mut samples);

        assert!(agc.gain() > 1.0, "gain should increase for quiet signal");
    }

    #[test]
    fn speech_agc_decreases_gain_for_loud_signal() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.01,
            max_gain: 10.0,
            min_gain: 0.1,
        };
        let mut agc = SpeechAgc::new(config);

        let mut samples = vec![0.5; 1000];
        agc.process(&mut samples);

        assert!(agc.gain() < 1.0, "gain should decrease for loud signal");
    }

    #[test]
    fn speech_agc_frozen_preserves_gain() {
        let config = AgcConfig::default();
        let mut agc = SpeechAgc::new(config);
        agc.set_frozen(true);

        let initial_gain = agc.gain();
        let mut samples = vec![0.5; 1000];
        agc.process(&mut samples);

        assert!(
            (agc.gain() - initial_gain).abs() < 0.001,
            "gain should not change when frozen"
        );
    }

    #[test]
    fn speech_agc_respects_gain_limits() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.1,
            max_gain: 2.0,
            min_gain: 0.5,
        };
        let mut agc = SpeechAgc::new(config);

        let mut quiet_samples = vec![0.001; 10000];
        agc.process(&mut quiet_samples);
        assert!(agc.gain() <= 2.0, "gain should not exceed max_gain");

        let mut agc2 = SpeechAgc::new(AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.1,
            max_gain: 2.0,
            min_gain: 0.5,
        });
        let mut loud_samples = vec![0.9; 10000];
        agc2.process(&mut loud_samples);
        assert!(agc2.gain() >= 0.5, "gain should not go below min_gain");
    }

    #[test]
    fn capture_config_defaults() {
        let config = CaptureConfig::default();
        assert!(!config.auto_gain_enabled);
        assert!((config.agc.desired_rms - 0.1).abs() < 0.001);
        assert!((config.agc.smoothing_factor - 0.0001).abs() < 0.00001);
    }

    fn generate_sine(
        freq_hz: f32,
        sample_rate: f32,
        duration_samples: usize,
        amplitude: f32,
    ) -> Vec<f32> {
        (0..duration_samples)
            .map(|i| {
                amplitude * (2.0 * std::f32::consts::PI * freq_hz * i as f32 / sample_rate).sin()
            })
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s.powi(2)).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn agc_silence_does_not_explode_gain() {
        let mut agc = SpeechAgc::new(AgcConfig::default());
        let mut silence = vec![0.0; 16000];
        agc.process(&mut silence);
        assert!(
            agc.gain() <= AgcConfig::default().max_gain,
            "gain should stay bounded even with silence"
        );
    }

    #[test]
    fn agc_near_silence_freezes_appropriately() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.01,
            ..Default::default()
        };
        let mut agc = SpeechAgc::new(config);

        let mut noise_floor = vec![0.0001; 1000];
        let initial_gain = agc.gain();
        agc.set_frozen(true);
        agc.process(&mut noise_floor);

        assert!(
            (agc.gain() - initial_gain).abs() < 0.01,
            "frozen AGC should not adjust gain for noise floor"
        );
    }

    #[test]
    fn agc_sudden_loud_burst_after_quiet() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.001,
            ..Default::default()
        };
        let mut agc = SpeechAgc::new(config);

        let mut quiet = vec![0.01; 8000];
        agc.process(&mut quiet);
        let gain_after_quiet = agc.gain();
        assert!(
            gain_after_quiet > 1.0,
            "gain should increase for quiet input"
        );

        let mut loud_burst = vec![0.8; 1000];
        agc.process(&mut loud_burst);

        assert!(
            loud_burst.iter().all(|&s| s.abs() <= 1.0),
            "output should not clip even with sudden loud burst"
        );
    }

    #[test]
    fn agc_simulated_speech_pattern() {
        let config = AgcConfig {
            desired_rms: 0.15,
            smoothing_factor: 0.0001,
            ..Default::default()
        };
        let mut agc = SpeechAgc::new(config);

        for _ in 0..10 {
            let mut speech = generate_sine(200.0, 16000.0, 4000, 0.3);
            agc.process(&mut speech);

            agc.set_frozen(true);
            let mut pause = vec![0.001; 2000];
            agc.process(&mut pause);
            agc.set_frozen(false);
        }

        let gain = agc.gain();
        assert!(
            gain > 0.3 && gain < 3.0,
            "gain should stabilize around 1.0 for speech at ~0.3 amplitude targeting 0.15 RMS, got {}",
            gain
        );
    }

    #[test]
    fn agc_sine_wave_converges_to_target() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.001,
            ..Default::default()
        };
        let mut agc = SpeechAgc::new(config);

        for _ in 0..20 {
            let mut chunk = generate_sine(440.0, 16000.0, 1600, 0.05);
            agc.process(&mut chunk);
        }

        let mut final_chunk = generate_sine(440.0, 16000.0, 1600, 0.05);
        agc.process(&mut final_chunk);
        let output_rms = rms(&final_chunk);

        assert!(
            (output_rms - 0.1).abs() < 0.05,
            "output RMS should converge near target 0.1, got {}",
            output_rms
        );
    }

    #[test]
    fn agc_gradual_fade_in() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.0005,
            ..Default::default()
        };
        let mut agc = SpeechAgc::new(config);

        for i in 1..=10 {
            let amplitude = 0.02 * i as f32;
            let mut chunk = generate_sine(300.0, 16000.0, 1600, amplitude);
            agc.process(&mut chunk);
        }

        let gain = agc.gain();
        assert!(
            gain > 0.1 && gain < 10.0,
            "gain should be reasonable after fade-in, got {}",
            gain
        );
    }

    #[test]
    fn agc_output_never_clips() {
        let config = AgcConfig {
            desired_rms: 0.2,
            smoothing_factor: 0.01,
            max_gain: 10.0,
            min_gain: 0.1,
        };
        let mut agc = SpeechAgc::new(config);

        let mut quiet = vec![0.01; 5000];
        agc.process(&mut quiet);

        let mut medium = generate_sine(440.0, 16000.0, 2000, 0.5);
        agc.process(&mut medium);

        assert!(
            medium.iter().all(|&s| s >= -1.0 && s <= 1.0),
            "output samples must stay within [-1, 1]"
        );
    }

    #[test]
    fn agc_handles_dc_offset() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.001,
            ..Default::default()
        };
        let mut agc = SpeechAgc::new(config);

        let mut dc_with_signal: Vec<f32> = generate_sine(200.0, 16000.0, 4000, 0.1)
            .into_iter()
            .map(|s| s + 0.1)
            .collect();

        agc.process(&mut dc_with_signal);

        assert!(
            dc_with_signal.iter().all(|&s| s.is_finite()),
            "AGC should handle DC offset without producing NaN/Inf"
        );
    }

    #[test]
    fn agc_alternating_volume_stability() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.0001,
            ..Default::default()
        };
        let mut agc = SpeechAgc::new(config);

        let mut gains = Vec::new();
        for i in 0..20 {
            let amplitude = if i % 2 == 0 { 0.05 } else { 0.2 };
            let mut chunk = generate_sine(300.0, 16000.0, 1600, amplitude);
            agc.process(&mut chunk);
            gains.push(agc.gain());
        }

        let gain_variance: f32 = {
            let mean = gains.iter().sum::<f32>() / gains.len() as f32;
            gains.iter().map(|g| (g - mean).powi(2)).sum::<f32>() / gains.len() as f32
        };

        assert!(
            gain_variance < 1.0,
            "gain should not oscillate wildly with alternating volumes, variance: {}",
            gain_variance
        );
    }
}
