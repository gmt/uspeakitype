//! PipeWire audio capture - 16kHz mono float32 from default source

use std::convert::TryInto;
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use pipewire as pw;
use pw::spa;
use pw::spa::param::audio::{AudioFormat, AudioInfoRaw};
use pw::spa::param::format::{MediaSubtype, MediaType};
use pw::spa::param::format_utils;
use pw::spa::pod::Pod;

pub type AudioCallback = Box<dyn Fn(&[f32]) + Send + 'static>;

pub struct AudioCapture {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioCapture {
    pub fn new(callback: AudioCallback) -> Result<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let thread = thread::spawn(move || {
            if let Err(e) = run_capture_loop(running_clone, callback) {
                eprintln!("Audio capture error: {e}");
            }
        });

        Ok(Self {
            running,
            thread: Some(thread),
        })
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
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
}

fn run_capture_loop(running: Arc<AtomicBool>, callback: AudioCallback) -> Result<()> {
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
    let timer = mainloop.loop_().add_timer(move |_| {
        if !running.load(Ordering::SeqCst) {
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
