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
            max_gain: 100.0,
            min_gain: 0.1,
            smoothing_factor: 0.001,
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
    hpf: HighPassFilter,
    bandpass: BandpassFilter,
    limiter: SoftLimiter,
}

struct SpeechAgc {
    config: AgcConfig,
    /// Smoothed gain estimate based on the measurement side.
    ///
    /// This is the "controller" state (attack/decay) before peak limiting.
    target_gain: f32,
    gain: f32,
    frozen: bool,
}

impl SpeechAgc {
    fn new(config: AgcConfig) -> Self {
        Self {
            config,
            target_gain: 1.0,
            gain: 1.0,
            frozen: false,
        }
    }

    fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
    }

    #[allow(dead_code)]
    fn gain(&self) -> f32 {
        self.gain
    }

    #[allow(dead_code)]
    fn calculate_gain(&mut self, speech_rms: f32, peak: f32) -> f32 {
        if self.frozen {
            return self.gain;
        }

        let desired_rms_squared = self.config.desired_rms * self.config.desired_rms;
        let speech_rms_squared = speech_rms * speech_rms;

        if speech_rms_squared > 1e-10 {
            let ratio = speech_rms_squared / desired_rms_squared;
            let adjustment = 1.0 + self.config.smoothing_factor * (1.0 - ratio);
            self.target_gain *= adjustment;
            self.target_gain = self
                .target_gain
                .clamp(self.config.min_gain, self.config.max_gain);
        }

        self.gain = self.target_gain;

        const HEADROOM: f32 = 0.95;
        if peak > 0.0 && peak * self.gain > HEADROOM {
            self.gain = HEADROOM / peak;
        }

        self.gain
    }

    #[allow(dead_code)]
    fn process(&mut self, samples: &mut [f32]) {
        let desired_rms_squared = self.config.desired_rms * self.config.desired_rms;

        for sample in samples.iter_mut() {
            *sample *= self.gain;

            if !self.frozen {
                let sample_power = sample.powi(2);
                if sample_power > 1e-10 {
                    let ratio = sample_power / desired_rms_squared;
                    let adjustment = 1.0 + self.config.smoothing_factor * (1.0 - ratio);
                    self.target_gain *= adjustment;
                    self.target_gain = self
                        .target_gain
                        .clamp(self.config.min_gain, self.config.max_gain);
                    self.gain = self.target_gain;
                }
            }

            *sample = sample.clamp(-1.0, 1.0);
        }
    }
}

struct HighPassFilter {
    prev_in: f32,
    prev_out: f32,
    alpha: f32,
}

impl HighPassFilter {
    fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
        let dt = 1.0 / sample_rate;
        let alpha = rc / (rc + dt);
        Self {
            prev_in: 0.0,
            prev_out: 0.0,
            alpha,
        }
    }

    fn process(&mut self, sample: f32) -> f32 {
        let out = self.alpha * (self.prev_out + sample - self.prev_in);
        self.prev_in = sample;
        self.prev_out = out;
        out
    }
}

#[allow(dead_code)]
struct BiquadFilter {
    // Feedforward
    b0: f32,
    b1: f32,
    b2: f32,

    // Feedback (a0 assumed normalized to 1)
    a1: f32,
    a2: f32,

    // State
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

#[allow(dead_code)]
impl BiquadFilter {
    fn new_lowpass(freq: f32, q: f32, sample_rate: f32) -> Self {
        // Audio EQ Cookbook (RBJ) lowpass biquad.
        let omega: f32 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let sin_omega: f32 = omega.sin();
        let cos_omega: f32 = omega.cos();
        let alpha: f32 = sin_omega / (2.0 * q);

        let mut b0: f32 = (1.0 - cos_omega) / 2.0;
        let mut b1: f32 = 1.0 - cos_omega;
        let mut b2: f32 = (1.0 - cos_omega) / 2.0;
        let a0: f32 = 1.0 + alpha;
        let mut a1: f32 = -2.0 * cos_omega;
        let mut a2: f32 = 1.0 - alpha;

        b0 /= a0;
        b1 /= a0;
        b2 /= a0;
        a1 /= a0;
        a2 /= a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn new_highpass(freq: f32, q: f32, sample_rate: f32) -> Self {
        // Audio EQ Cookbook (RBJ) highpass biquad.
        let omega: f32 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let sin_omega: f32 = omega.sin();
        let cos_omega: f32 = omega.cos();
        let alpha: f32 = sin_omega / (2.0 * q);

        let mut b0: f32 = (1.0 + cos_omega) / 2.0;
        let mut b1: f32 = -(1.0 + cos_omega);
        let mut b2: f32 = (1.0 + cos_omega) / 2.0;
        let a0: f32 = 1.0 + alpha;
        let mut a1: f32 = -2.0 * cos_omega;
        let mut a2: f32 = 1.0 - alpha;

        b0 /= a0;
        b1 /= a0;
        b2 /= a0;
        a1 /= a0;
        a2 /= a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        // Direct Form I:
        // y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]
        let y: f32 = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;

        y
    }
}

#[allow(dead_code)]
struct BandpassFilter {
    hpf: BiquadFilter,
    lpf: BiquadFilter,
}

#[allow(dead_code)]
impl BandpassFilter {
    fn new(sample_rate: f32) -> Self {
        let hpf = BiquadFilter::new_highpass(300.0, 0.707, sample_rate);
        let lpf = BiquadFilter::new_lowpass(3400.0, 0.707, sample_rate);
        Self { hpf, lpf }
    }

    #[allow(clippy::let_and_return)]
    fn process(&mut self, sample: f32) -> f32 {
        // Chain: HPF removes bass, LPF removes highs.
        let after_hpf = self.hpf.process(sample);
        let after_lpf = self.lpf.process(after_hpf);
        after_lpf
    }
}

#[allow(dead_code)]
struct SoftLimiter {
    threshold: f32,
    knee: f32,
}

#[allow(dead_code)]
impl SoftLimiter {
    const DEFAULT_THRESHOLD: f32 = 0.9;
    const DEFAULT_KNEE: f32 = 0.1;

    fn new(threshold: f32, knee: f32) -> Self {
        Self { threshold, knee }
    }

    fn process(&mut self, sample: f32) -> f32 {
        let abs_s = sample.abs();
        if abs_s <= self.threshold {
            sample
        } else {
            let excess = abs_s - self.threshold;
            let compressed = self.threshold + self.knee * (1.0 - (-excess / self.knee).exp());
            compressed.copysign(sample)
        }
    }
}

#[allow(dead_code)]
impl Default for SoftLimiter {
    fn default() -> Self {
        Self::new(Self::DEFAULT_THRESHOLD, Self::DEFAULT_KNEE)
    }
}

fn apply_auto_gain(
    samples: &mut [f32],
    control: &CaptureControl,
    agc: &mut SpeechAgc,
    hpf: &mut HighPassFilter,
    bandpass: &mut BandpassFilter,
    limiter: &mut SoftLimiter,
) {
    if !control.is_auto_gain_enabled() {
        return;
    }

    // 1. HPF for output path (remove DC + subsonic)
    let mut hpf_samples: Vec<f32> = samples.iter().map(|s| hpf.process(*s)).collect();

    // 2. Bandpass for speech detection (sidechain)
    let speech_samples: Vec<f32> = samples.iter().map(|s| bandpass.process(*s)).collect();
    let speech_rms = if speech_samples.is_empty() {
        0.0
    } else {
        let mean_sq =
            speech_samples.iter().map(|s| s * s).sum::<f32>() / speech_samples.len() as f32;
        mean_sq.sqrt()
    };

    // 3. Peak detection on full spectrum (HPF'd signal)
    let peak = hpf_samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);

    // 4. Set frozen state based on speech RMS
    agc.set_frozen(speech_rms < 0.001);

    // 5. Calculate gain (speech-aware, peak-limited)
    let gain = agc.calculate_gain(speech_rms, peak);

    // 6. Apply gain to HPF'd signal
    for sample in hpf_samples.iter_mut() {
        *sample *= gain;
    }

    // 7. Soft limiter for safety
    for (out, &gained) in samples.iter_mut().zip(hpf_samples.iter()) {
        *out = limiter.process(gained);
    }

    // 8. Update control display
    control.set_current_gain(gain);
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
        hpf: HighPassFilter::new(100.0, 16000.0),
        bandpass: BandpassFilter::new(16000.0),
        limiter: SoftLimiter::default(),
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

                apply_auto_gain(
                    &mut samples,
                    &user_data.control,
                    &mut user_data.agc,
                    &mut user_data.hpf,
                    &mut user_data.bandpass,
                    &mut user_data.limiter,
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
        assert!((config.agc.smoothing_factor - 0.001).abs() < 0.0001);
    }

    fn generate_sine(freq: f32, sample_rate: f32, num_samples: usize) -> Vec<f32> {
        generate_sine_with_amplitude(freq, sample_rate, num_samples, 1.0)
    }

    fn generate_sine_with_amplitude(
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

    fn rms_skip(samples: &[f32], skip: usize) -> f32 {
        if samples.len() <= skip {
            return 0.0;
        }
        rms(&samples[skip..])
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
            let mut speech = generate_sine_with_amplitude(200.0, 16000.0, 4000, 0.3);
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
            let mut chunk = generate_sine_with_amplitude(440.0, 16000.0, 1600, 0.05);
            agc.process(&mut chunk);
        }

        let mut final_chunk = generate_sine_with_amplitude(440.0, 16000.0, 1600, 0.05);
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
            let mut chunk = generate_sine_with_amplitude(300.0, 16000.0, 1600, amplitude);
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

        let mut medium = generate_sine_with_amplitude(440.0, 16000.0, 2000, 0.5);
        agc.process(&mut medium);

        assert!(
            medium.iter().all(|&s| (-1.0..=1.0).contains(&s)),
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

        let mut dc_with_signal: Vec<f32> = generate_sine_with_amplitude(200.0, 16000.0, 4000, 0.1)
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
            let mut chunk = generate_sine_with_amplitude(300.0, 16000.0, 1600, amplitude);
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

    #[test]
    fn test_biquad_lowpass_response() {
        let sample_rate = 16000.0;
        let num_samples = 2000;
        let settle = 200;

        let mut lpf = BiquadFilter::new_lowpass(1000.0, 0.707, sample_rate);

        // 500Hz (below cutoff) should pass through mostly.
        let low_freq = generate_sine(500.0, sample_rate, num_samples);
        let filtered_low: Vec<f32> = low_freq.iter().map(|&s| lpf.process(s)).collect();
        let attenuation_low = rms_skip(&filtered_low, settle) / rms_skip(&low_freq, settle);
        assert!(
            attenuation_low > 0.6,
            "500Hz should pass with minimal attenuation (ratio={})",
            attenuation_low
        );

        // Reset filter state.
        lpf = BiquadFilter::new_lowpass(1000.0, 0.707, sample_rate);

        // 5kHz (well above cutoff) should be heavily attenuated.
        let high_freq = generate_sine(5000.0, sample_rate, num_samples);
        let filtered_high: Vec<f32> = high_freq.iter().map(|&s| lpf.process(s)).collect();
        let attenuation_high = rms_skip(&filtered_high, settle) / rms_skip(&high_freq, settle);
        assert!(
            attenuation_high < 0.2,
            "5kHz should be heavily attenuated (ratio={})",
            attenuation_high
        );
    }

    #[test]
    fn test_bandpass_speech_band() {
        let sample_rate = 16000.0;
        let num_samples = 2000;
        let settle = 200;

        // Bass attenuation (100Hz, below 300Hz cutoff)
        let mut bp_bass = BandpassFilter::new(sample_rate);
        let bass = generate_sine(100.0, sample_rate, num_samples);
        let filtered_bass: Vec<f32> = bass.iter().map(|&s| bp_bass.process(s)).collect();
        let atten_bass = rms_skip(&filtered_bass, settle) / rms_skip(&bass, settle);
        assert!(
            atten_bass < 0.2,
            "100Hz should be heavily attenuated (ratio={})",
            atten_bass
        );

        // Passband (1kHz, within 300-3400Hz)
        let mut bp_mid = BandpassFilter::new(sample_rate);
        let mid = generate_sine(1000.0, sample_rate, num_samples);
        let filtered_mid: Vec<f32> = mid.iter().map(|&s| bp_mid.process(s)).collect();
        let atten_mid = rms_skip(&filtered_mid, settle) / rms_skip(&mid, settle);
        assert!(
            atten_mid > 0.5,
            "1kHz should pass through (ratio={})",
            atten_mid
        );

        // Highs attenuation (6kHz, above 3400Hz cutoff)
        let mut bp_high = BandpassFilter::new(sample_rate);
        let high = generate_sine(6000.0, sample_rate, num_samples);
        let filtered_high: Vec<f32> = high.iter().map(|&s| bp_high.process(s)).collect();
        let atten_high = rms_skip(&filtered_high, settle) / rms_skip(&high, settle);
        assert!(
            atten_high < 0.2,
            "6kHz should be heavily attenuated (ratio={})",
            atten_high
        );
    }

    #[test]
    fn test_soft_limiter_threshold() {
        let mut limiter = SoftLimiter::new(0.9, 0.1);

        // Below threshold: pass through unchanged.
        let below = 0.5;
        assert_eq!(limiter.process(below), below);

        // Above threshold: compress smoothly.
        let above = 1.5;
        let limited = limiter.process(above);
        assert!(limited <= 1.0, "Output should not exceed 1.0");
        assert!(limited > 0.9, "Output should be above threshold");
        assert!(limited < above, "Output should be less than input");
    }

    #[test]
    fn test_soft_limiter_bounds() {
        let mut limiter = SoftLimiter::default();

        for i in 0..=100 {
            let input = -2.0 + 0.04 * i as f32; // -2.0..=+2.0
            let output = limiter.process(input);
            assert!(
                output.abs() <= 1.0 + 1e-6,
                "Output should never exceed ±1.0 (input={}, output={})",
                input,
                output
            );
        }
    }

    #[test]
    fn test_agc_sidechain_ignores_bass() {
        let sample_rate = 16000.0;
        let num_samples = 4000;
        let settle = 400;

        // Loud bass + quiet speech.
        // Choose a bass frequency far below the 300Hz HPF corner so the sidechain
        // largely rejects it.
        let bass = generate_sine_with_amplitude(50.0, sample_rate, num_samples, 0.8);
        let speech = generate_sine_with_amplitude(1000.0, sample_rate, num_samples, 0.02);
        let mix: Vec<f32> = bass.iter().zip(speech.iter()).map(|(b, s)| b + s).collect();

        // Sidechain RMS should be dominated by speech, not the bass.
        let mut bp_mix = BandpassFilter::new(sample_rate);
        let mix_bp: Vec<f32> = mix.iter().map(|&s| bp_mix.process(s)).collect();
        let mix_bp_rms = rms_skip(&mix_bp, settle);

        let mut bp_speech = BandpassFilter::new(sample_rate);
        let speech_bp: Vec<f32> = speech.iter().map(|&s| bp_speech.process(s)).collect();
        let speech_bp_rms = rms_skip(&speech_bp, settle);

        assert!(
            speech_bp_rms > 0.0,
            "speech bandpass RMS should be non-zero"
        );
        let relative_delta = (mix_bp_rms - speech_bp_rms).abs() / speech_bp_rms;
        assert!(
            relative_delta < 0.5,
            "sidechain should mostly ignore bass: speech_bp_rms={}, mix_bp_rms={}, delta={}",
            speech_bp_rms,
            mix_bp_rms,
            relative_delta
        );

        // Full-band RMS DOES increase substantially when bass is added.
        let mix_rms = rms_skip(&mix, settle);
        let speech_rms = rms_skip(&speech, settle);
        assert!(
            mix_rms > 5.0 * speech_rms,
            "mix RMS should be much larger than speech RMS (mix_rms={}, speech_rms={})",
            mix_rms,
            speech_rms
        );

        // Gain computed from sidechain should be similar with/without bass present.
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.01,
            min_gain: 0.1,
            max_gain: 100.0,
        };

        let mut agc_speech = SpeechAgc::new(config.clone());
        let mut agc_mix = SpeechAgc::new(config);

        let peak = 0.1;
        for _ in 0..100 {
            agc_speech.calculate_gain(speech_bp_rms, peak);
            agc_mix.calculate_gain(mix_bp_rms, peak);
        }

        let gain_speech = agc_speech.gain();
        let gain_mix = agc_mix.gain();
        let gain_delta = (gain_speech - gain_mix).abs() / gain_speech.max(1e-6);
        assert!(
            gain_delta < 0.1,
            "bass should not strongly affect gain (speech_gain={}, mix_gain={}, delta={})",
            gain_speech,
            gain_mix,
            gain_delta
        );
    }

    #[test]
    fn test_agc_peak_limiting() {
        let config = AgcConfig {
            desired_rms: 0.1,
            smoothing_factor: 0.01,
            min_gain: 0.1,
            max_gain: 100.0,
        };
        let mut agc = SpeechAgc::new(config);

        // Drive the desired gain up (quiet speech, low peak).
        for _ in 0..500 {
            agc.calculate_gain(0.001, 0.1);
        }

        // Now present a high peak (e.g. bass transient) and ensure gain backs off.
        let peak = 0.9;
        let gain = agc.calculate_gain(0.001, peak);

        let expected = 0.95 / peak;
        assert!(
            (gain - expected).abs() < 0.02,
            "gain should be headroom-limited (gain={}, expected={})",
            gain,
            expected
        );
        assert!(
            peak * gain <= 0.95 + 1e-6,
            "Peak * gain must not exceed headroom"
        );
        assert!(gain <= 1.1, "Gain should be backed off due to high peak");
    }
}
