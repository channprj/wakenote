use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInputConfig {
    pub device_id: String,
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateInputDevice {
    pub id: String,
    pub label: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInputDevice {
    pub device_id: String,
    pub device_name: String,
    pub used_fallback_device: bool,
}

impl Default for AudioInputConfig {
    fn default() -> Self {
        Self {
            device_id: "default".to_string(),
            sample_rate: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum LiveCaptureError {
    #[error("live capture is already running")]
    AlreadyRunning,
    #[error("no input device is available")]
    NoInputDevice,
    #[error("cpal error: {0}")]
    Cpal(String),
}

pub trait AudioStreamHandle: Send {}

pub trait AudioInputBackend: Send {
    fn start(
        &mut self,
        config: AudioInputConfig,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError>;
}

pub struct LiveCaptureRuntime<B: AudioInputBackend> {
    backend: B,
    stream: Option<Box<dyn AudioStreamHandle>>,
    dispatcher: Option<FrameDispatcher>,
}

struct FrameDispatcher {
    sender: Arc<Mutex<Option<mpsc::Sender<AudioFrame>>>>,
    join: Option<thread::JoinHandle<()>>,
}

impl<B: AudioInputBackend> LiveCaptureRuntime<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            stream: None,
            dispatcher: None,
        }
    }

    pub fn start(
        &mut self,
        config: AudioInputConfig,
        on_frame: impl Fn(AudioFrame) + Send + Sync + 'static,
    ) -> Result<(), LiveCaptureError> {
        if self.stream.is_some() {
            return Err(LiveCaptureError::AlreadyRunning);
        }

        let (dispatcher, on_frame) = FrameDispatcher::new(on_frame);
        let stream = match self.backend.start(config, on_frame) {
            Ok(stream) => stream,
            Err(error) => {
                drop(dispatcher);
                return Err(error);
            }
        };
        self.stream = Some(stream);
        self.dispatcher = Some(dispatcher);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream = None;
        self.dispatcher = None;
    }

    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }
}

impl FrameDispatcher {
    fn new(
        on_frame: impl Fn(AudioFrame) + Send + Sync + 'static,
    ) -> (Self, Arc<dyn Fn(AudioFrame) + Send + Sync>) {
        let (tx, rx) = mpsc::channel::<AudioFrame>();
        let sender = Arc::new(Mutex::new(Some(tx)));
        let callback_sender = sender.clone();
        let callback = Arc::new(move |frame| {
            let Ok(sender) = callback_sender.lock() else {
                return;
            };
            if let Some(tx) = sender.as_ref() {
                let _ = tx.send(frame);
            }
        });
        let join = thread::spawn(move || {
            while let Ok(frame) = rx.recv() {
                on_frame(frame);
            }
        });

        (
            Self {
                sender,
                join: Some(join),
            },
            callback,
        )
    }

    fn stop(&mut self) {
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for FrameDispatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Default)]
pub struct CpalAudioInput;

pub struct CpalStreamHandle {
    stop_tx: Option<mpsc::Sender<()>>,
    join: Option<thread::JoinHandle<()>>,
}

impl AudioStreamHandle for CpalStreamHandle {}

impl Drop for CpalStreamHandle {
    fn drop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl AudioInputBackend for CpalAudioInput {
    fn start(
        &mut self,
        config: AudioInputConfig,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop_tx, stop_rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let stream = match build_cpal_stream(config, on_frame) {
                Ok(stream) => stream,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };

            if let Err(error) = stream.play() {
                let _ = ready_tx.send(Err(LiveCaptureError::Cpal(error.to_string())));
                return;
            }

            let _ = ready_tx.send(Ok(()));
            let _ = stop_rx.recv();
            drop(stream);
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Box::new(CpalStreamHandle {
                stop_tx: Some(stop_tx),
                join: Some(join),
            })),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(error) => {
                let _ = join.join();
                Err(LiveCaptureError::Cpal(error.to_string()))
            }
        }
    }
}

impl CpalAudioInput {
    pub fn default_sample_rate(device_id: &str) -> Result<u32, LiveCaptureError> {
        let resolved = Self::resolve_device(device_id)?;
        Ok(resolved.sample_rate)
    }

    pub fn resolve_device(device_id: &str) -> Result<ResolvedCpalInputDevice, LiveCaptureError> {
        let host = cpal::default_host();
        let (device, resolved) = select_device_with_resolution(&host, device_id)?;
        let config = device
            .default_input_config()
            .map_err(|error| LiveCaptureError::Cpal(error.to_string()))?;
        Ok(ResolvedCpalInputDevice {
            device_id: resolved.device_id,
            device_name: resolved.device_name,
            used_fallback_device: resolved.used_fallback_device,
            sample_rate: config.sample_rate().0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCpalInputDevice {
    pub device_id: String,
    pub device_name: String,
    pub used_fallback_device: bool,
    pub sample_rate: u32,
}

fn build_cpal_stream(
    config: AudioInputConfig,
    on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
) -> Result<cpal::Stream, LiveCaptureError> {
    let host = cpal::default_host();
    let (device, _) = select_device_with_resolution(&host, &config.device_id)?;
    let supported_config = device
        .default_input_config()
        .map_err(|error| LiveCaptureError::Cpal(error.to_string()))?;
    let mut stream_config: cpal::StreamConfig = supported_config.clone().into();
    if let Some(sample_rate) = config.sample_rate {
        stream_config.sample_rate = cpal::SampleRate(sample_rate);
    }
    let channels = usize::from(stream_config.channels);
    let sample_rate = stream_config.sample_rate.0;

    let err_fn = |error| {
        eprintln!("audio input stream error: {error}");
    };

    match supported_config.sample_format() {
        cpal::SampleFormat::F32 => {
            let on_frame = on_frame.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| emit_f32_frame(data, channels, sample_rate, &on_frame),
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let on_frame = on_frame.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let samples = data
                        .iter()
                        .map(|sample| *sample as f32 / i16::MAX as f32)
                        .collect::<Vec<_>>();
                    emit_f32_frame(&samples, channels, sample_rate, &on_frame);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let on_frame = on_frame.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let samples = data
                        .iter()
                        .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0)
                        .collect::<Vec<_>>();
                    emit_f32_frame(&samples, channels, sample_rate, &on_frame);
                },
                err_fn,
                None,
            )
        }
        other => {
            return Err(LiveCaptureError::Cpal(format!(
                "unsupported input sample format {other:?}"
            )));
        }
    }
    .map_err(|error| LiveCaptureError::Cpal(error.to_string()))
}

fn select_device_with_resolution(
    host: &cpal::Host,
    device_id: &str,
) -> Result<(cpal::Device, ResolvedInputDevice), LiveCaptureError> {
    let default_device = host
        .default_input_device()
        .ok_or(LiveCaptureError::NoInputDevice)?;
    if device_id == "default" {
        return Ok((
            default_device,
            ResolvedInputDevice {
                device_id: "default".to_string(),
                device_name: "System Default".to_string(),
                used_fallback_device: false,
            },
        ));
    }

    let devices = host
        .input_devices()
        .map_err(|error| LiveCaptureError::Cpal(error.to_string()))?;
    for (index, device) in devices.enumerate() {
        let label = device.name().unwrap_or_default();
        let stable_id = stable_input_device_id(index, &label);
        if stable_id == device_id || label == device_id {
            return Ok((
                device,
                ResolvedInputDevice {
                    device_id: stable_id,
                    device_name: label,
                    used_fallback_device: false,
                },
            ));
        }
    }

    Ok((
        default_device,
        ResolvedInputDevice {
            device_id: "default".to_string(),
            device_name: "System Default".to_string(),
            used_fallback_device: true,
        },
    ))
}

pub fn resolve_input_device_from_candidates(
    requested_device_id: &str,
    candidates: &[CandidateInputDevice],
) -> Option<ResolvedInputDevice> {
    if requested_device_id == "default" {
        return Some(ResolvedInputDevice {
            device_id: "default".to_string(),
            device_name: "System Default".to_string(),
            used_fallback_device: false,
        });
    }

    if let Some(candidate) = candidates.iter().find(|candidate| {
        candidate.id == requested_device_id || candidate.label == requested_device_id
    }) {
        return Some(ResolvedInputDevice {
            device_id: candidate.id.clone(),
            device_name: candidate.label.clone(),
            used_fallback_device: false,
        });
    }

    candidates
        .iter()
        .find(|candidate| candidate.is_default)
        .map(|_| ResolvedInputDevice {
            device_id: "default".to_string(),
            device_name: "System Default".to_string(),
            used_fallback_device: true,
        })
}

pub fn stable_input_device_id(index: usize, label: &str) -> String {
    format!("input-{index}-{}", slugify_device_label(label))
}

fn slugify_device_label(label: &str) -> String {
    let slug = label
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if character.is_whitespace() || character == '-' || character == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();
    slug.trim_matches('-').to_string()
}

fn emit_f32_frame(
    interleaved: &[f32],
    channels: usize,
    sample_rate: u32,
    on_frame: &Arc<dyn Fn(AudioFrame) + Send + Sync>,
) {
    if interleaved.is_empty() || channels == 0 || sample_rate == 0 {
        return;
    }

    let mut mono = Vec::with_capacity(interleaved.len() / channels.max(1));
    for frame in interleaved.chunks(channels) {
        let sample = frame.iter().copied().sum::<f32>() / frame.len() as f32;
        mono.push(sample);
    }
    let duration_ms = ((mono.len() as f64 / sample_rate as f64) * 1000.0).round() as u64;
    on_frame(AudioFrame {
        samples: mono,
        duration_ms: duration_ms.max(1),
    });
}
