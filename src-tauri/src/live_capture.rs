use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

const FRAME_DISPATCH_QUEUE_CAPACITY: usize = 512;
const CPAL_STREAM_READY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub duration_ms: u64,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInputConfig {
    pub device_id: String,
    pub sample_rate: Option<u32>,
    /// Persisted label for the requested device, used as a fallback when the
    /// cpal enumeration index has changed and `device_id` no longer matches.
    pub label_hint: Option<String>,
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
            label_hint: None,
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

pub trait AudioStreamHandle: Send {
    fn runtime_error(&self) -> Option<String> {
        None
    }
}

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
    queue: Arc<FrameDispatchQueue>,
    join: Option<thread::JoinHandle<()>>,
}

struct FrameDispatchQueue {
    state: Mutex<FrameDispatchState>,
    available: Condvar,
    dropped_frames: AtomicU64,
}

struct FrameDispatchState {
    frames: VecDeque<AudioFrame>,
    closed: bool,
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
        if self
            .stream
            .as_ref()
            .and_then(|stream| stream.runtime_error())
            .is_some()
        {
            self.stop();
        } else if self.stream.is_some() {
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

    pub fn dropped_frame_count(&self) -> u64 {
        self.dispatcher
            .as_ref()
            .map(FrameDispatcher::dropped_frame_count)
            .unwrap_or(0)
    }

    pub fn runtime_error(&self) -> Option<String> {
        self.stream
            .as_ref()
            .and_then(|stream| stream.runtime_error())
    }
}

impl FrameDispatcher {
    fn new(
        on_frame: impl Fn(AudioFrame) + Send + Sync + 'static,
    ) -> (Self, Arc<dyn Fn(AudioFrame) + Send + Sync>) {
        let queue = Arc::new(FrameDispatchQueue::new());
        let callback_queue = queue.clone();
        let callback = Arc::new(move |frame| {
            callback_queue.push(frame);
        });
        let worker_queue = queue.clone();
        let join = thread::spawn(move || {
            while let Some(frame) = worker_queue.pop() {
                on_frame(frame);
            }
        });

        (
            Self {
                queue,
                join: Some(join),
            },
            callback,
        )
    }

    fn stop(&mut self) {
        self.queue.close();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    fn dropped_frame_count(&self) -> u64 {
        self.queue.dropped_frame_count()
    }
}

impl FrameDispatchQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(FrameDispatchState {
                frames: VecDeque::new(),
                closed: false,
            }),
            available: Condvar::new(),
            dropped_frames: AtomicU64::new(0),
        }
    }

    fn push(&self, frame: AudioFrame) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.closed {
            return;
        }
        if state.frames.len() >= FRAME_DISPATCH_QUEUE_CAPACITY {
            state.frames.pop_front();
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
        state.frames.push_back(frame);
        self.available.notify_one();
    }

    fn pop(&self) -> Option<AudioFrame> {
        let mut state = self.state.lock().ok()?;
        loop {
            if let Some(frame) = state.frames.pop_front() {
                return Some(frame);
            }
            if state.closed {
                return None;
            }
            state = self.available.wait(state).ok()?;
        }
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            self.available.notify_all();
        }
    }

    fn dropped_frame_count(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
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
    runtime_error: Arc<Mutex<Option<String>>>,
}

impl AudioStreamHandle for CpalStreamHandle {
    fn runtime_error(&self) -> Option<String> {
        self.runtime_error.lock().ok()?.clone()
    }
}

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
        let runtime_error = Arc::new(Mutex::new(None));
        let stream_error = runtime_error.clone();
        let join = thread::spawn(move || {
            let stream = match build_cpal_stream(config, on_frame, stream_error) {
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

        match ready_rx.recv_timeout(CPAL_STREAM_READY_TIMEOUT) {
            Ok(Ok(())) => Ok(Box::new(CpalStreamHandle {
                stop_tx: Some(stop_tx),
                join: Some(join),
                runtime_error,
            })),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = stop_tx.send(());
                Err(LiveCaptureError::Cpal(
                    "input device did not become ready within 2 seconds".to_string(),
                ))
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = join.join();
                Err(LiveCaptureError::Cpal(
                    "input device startup disconnected".to_string(),
                ))
            }
        }
    }
}

impl CpalAudioInput {
    pub fn default_sample_rate(device_id: &str) -> Result<u32, LiveCaptureError> {
        let resolved = Self::resolve_device(device_id, None)?;
        Ok(resolved.sample_rate)
    }

    pub fn resolve_device(
        device_id: &str,
        label_hint: Option<&str>,
    ) -> Result<ResolvedCpalInputDevice, LiveCaptureError> {
        let host = cpal::default_host();
        let (device, resolved) = select_device_with_resolution(&host, device_id, label_hint)?;
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
    runtime_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, LiveCaptureError> {
    let host = cpal::default_host();
    let (device, _) = select_device_with_resolution(
        &host,
        &config.device_id,
        config.label_hint.as_deref(),
    )?;
    let supported_config = device
        .default_input_config()
        .map_err(|error| LiveCaptureError::Cpal(error.to_string()))?;
    let mut stream_config: cpal::StreamConfig = supported_config.clone().into();
    if let Some(sample_rate) = config.sample_rate {
        stream_config.sample_rate = cpal::SampleRate(sample_rate);
    }
    let channels = usize::from(stream_config.channels);
    let sample_rate = stream_config.sample_rate.0;

    let err_fn = move |error: cpal::StreamError| {
        if let Ok(mut runtime_error) = runtime_error.lock() {
            *runtime_error = Some(error.to_string());
        }
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
    label_hint: Option<&str>,
) -> Result<(cpal::Device, ResolvedInputDevice), LiveCaptureError> {
    let default_device = host.default_input_device();
    if device_id == "default" {
        let Some(default_device) = default_device else {
            return Err(LiveCaptureError::NoInputDevice);
        };
        return Ok((default_device, default_input_resolution(false)));
    }

    let devices = host
        .input_devices()
        .map_err(|error| LiveCaptureError::Cpal(error.to_string()))?;
    // First pass: exact id match.
    let mut label_match: Option<(cpal::Device, ResolvedInputDevice)> = None;
    let mut legacy_match: Option<(cpal::Device, ResolvedInputDevice)> = None;
    let label_hint = label_hint.filter(|label| !label.is_empty());
    for (index, device) in devices.enumerate() {
        let label = device.name().unwrap_or_default();
        let stable_id = stable_input_device_id(index, &label);
        if stable_id == device_id {
            return Ok((
                device,
                ResolvedInputDevice {
                    device_id: stable_id,
                    device_name: label,
                    used_fallback_device: false,
                },
            ));
        }
        if label_match.is_none() {
            if let Some(hint) = label_hint {
                if label == hint {
                    label_match = Some((
                        device,
                        ResolvedInputDevice {
                            device_id: stable_id.clone(),
                            device_name: label.clone(),
                            used_fallback_device: false,
                        },
                    ));
                    continue;
                }
            }
        }
        if legacy_match.is_none() && label == device_id {
            legacy_match = Some((
                device,
                ResolvedInputDevice {
                    device_id: stable_id,
                    device_name: label,
                    used_fallback_device: false,
                },
            ));
        }
    }

    if let Some(matched) = label_match.or(legacy_match) {
        return Ok(matched);
    }

    let Some(default_device) = default_device else {
        return Err(LiveCaptureError::NoInputDevice);
    };
    Ok((default_device, default_input_resolution(true)))
}

fn default_input_resolution(used_fallback_device: bool) -> ResolvedInputDevice {
    ResolvedInputDevice {
        device_id: "default".to_string(),
        device_name: "System Default".to_string(),
        used_fallback_device,
    }
}

pub fn resolve_input_device_from_candidates(
    requested_device_id: &str,
    label_hint: Option<&str>,
    candidates: &[CandidateInputDevice],
) -> Option<ResolvedInputDevice> {
    if requested_device_id == "default" {
        return candidates
            .iter()
            .any(|candidate| candidate.is_default)
            .then(|| default_input_resolution(false));
    }

    // Exact id match wins: the stable id is the precise pin and a different
    // device that happens to share a label must not be selected.
    if let Some(candidate) = candidates.iter().find(|c| c.id == requested_device_id) {
        return Some(ResolvedInputDevice {
            device_id: candidate.id.clone(),
            device_name: candidate.label.clone(),
            used_fallback_device: false,
        });
    }

    // Label-based fallback. The stable id embeds the cpal enumeration index,
    // which is not actually stable across reboots / hot-swaps on macOS — but
    // the device label usually is. Match by the persisted label so the same
    // physical mic is re-acquired after its index moves.
    let label_hint = label_hint.filter(|label| !label.is_empty());
    if let Some(label) = label_hint {
        if let Some(candidate) = candidates.iter().find(|c| c.label == label) {
            return Some(ResolvedInputDevice {
                device_id: candidate.id.clone(),
                device_name: candidate.label.clone(),
                used_fallback_device: false,
            });
        }
    }

    // Legacy: callers used to pass a label string as `requested_device_id`.
    if let Some(candidate) = candidates.iter().find(|c| c.label == requested_device_id) {
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
        captured_at: Utc::now(),
    });
}
