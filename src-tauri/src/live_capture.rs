use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

use crate::voice_leveling::AudioFrameProcessor;

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
    pub core_audio_uid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateInputDevice {
    pub id: String,
    pub label: String,
    pub is_default: bool,
    pub core_audio_uid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInputDevice {
    pub device_id: String,
    pub device_name: String,
    pub used_fallback_device: bool,
    pub core_audio_uid: Option<String>,
}

impl Default for AudioInputConfig {
    fn default() -> Self {
        Self {
            device_id: "default".to_string(),
            sample_rate: None,
            label_hint: None,
            core_audio_uid: None,
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
        on_frame: impl FnMut(AudioFrame) + Send + 'static,
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

    pub fn start_processed<P, F>(
        &mut self,
        config: AudioInputConfig,
        mut processor: P,
        mut on_frame: F,
    ) -> Result<(), LiveCaptureError>
    where
        P: AudioFrameProcessor + 'static,
        F: FnMut(AudioFrame) + Send + 'static,
    {
        self.start(config, move |frame| {
            for processed in processor.process(frame) {
                on_frame(processed);
            }
        })
    }

    pub fn start_processed_with_warnings<P, F, W>(
        &mut self,
        config: AudioInputConfig,
        mut processor: P,
        mut on_frame: F,
        mut on_warning: W,
    ) -> Result<(), LiveCaptureError>
    where
        P: AudioFrameProcessor + 'static,
        F: FnMut(AudioFrame) + Send + 'static,
        W: FnMut(String) + Send + 'static,
    {
        self.start(config, move |frame| {
            let processed = processor.process(frame);
            if let Some(warning) = processor.take_warning() {
                on_warning(warning);
            }
            for frame in processed {
                on_frame(frame);
            }
        })
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

impl<B: AudioInputBackend> AudioStreamHandle for LiveCaptureRuntime<B> {
    fn runtime_error(&self) -> Option<String> {
        LiveCaptureRuntime::runtime_error(self)
    }
}

impl FrameDispatcher {
    fn new(
        mut on_frame: impl FnMut(AudioFrame) + Send + 'static,
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
        Self::resolve_device_with_uid(device_id, label_hint, None)
    }

    pub fn resolve_device_with_uid(
        device_id: &str,
        label_hint: Option<&str>,
        core_audio_uid: Option<&str>,
    ) -> Result<ResolvedCpalInputDevice, LiveCaptureError> {
        let host = cpal::default_host();
        let (device, resolved) =
            select_device_with_resolution(&host, device_id, label_hint, core_audio_uid)?;
        let config = device
            .default_input_config()
            .map_err(|error| LiveCaptureError::Cpal(error.to_string()))?;
        Ok(ResolvedCpalInputDevice {
            device_id: resolved.device_id,
            device_name: resolved.device_name,
            used_fallback_device: resolved.used_fallback_device,
            core_audio_uid: resolved.core_audio_uid,
            sample_rate: config.sample_rate().0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCpalInputDevice {
    pub device_id: String,
    pub device_name: String,
    pub used_fallback_device: bool,
    pub core_audio_uid: Option<String>,
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
        config.core_audio_uid.as_deref(),
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
    let mut encoder = MicrophoneFrameEncoder::default();

    crate::log_event!(
        "audio input format: device={} rate={} channels={} sample_format={:?}",
        config.device_id,
        sample_rate,
        channels,
        supported_config.sample_format()
    );

    let err_fn = move |error: cpal::StreamError| {
        if let Ok(mut runtime_error) = runtime_error.lock() {
            *runtime_error = Some(error.to_string());
        }
        crate::log_error!("audio input stream error: {error}");
    };

    match supported_config.sample_format() {
        cpal::SampleFormat::F32 => {
            let on_frame = on_frame.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], info| {
                    encoder.emit(data, channels, sample_rate, info, &on_frame)
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let on_frame = on_frame.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], info| {
                    let samples = data
                        .iter()
                        .map(|sample| *sample as f32 / i16::MAX as f32)
                        .collect::<Vec<_>>();
                    encoder.emit(&samples, channels, sample_rate, info, &on_frame);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let on_frame = on_frame.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], info| {
                    let samples = data
                        .iter()
                        .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0)
                        .collect::<Vec<_>>();
                    encoder.emit(&samples, channels, sample_rate, info, &on_frame);
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
    core_audio_uid: Option<&str>,
) -> Result<(cpal::Device, ResolvedInputDevice), LiveCaptureError> {
    use crate::microphone_level::MicrophoneVolumeBackend;
    use crate::microphone_level::macos::PlatformVolumeBackend;

    let volume_backend = PlatformVolumeBackend::new();
    let system_devices = volume_backend.input_devices().unwrap_or_default();
    let default_device = host.default_input_device();
    if device_id == "default" {
        let Some(default_device) = default_device else {
            return Err(LiveCaptureError::NoInputDevice);
        };
        return Ok((
            default_device,
            default_input_resolution(false, volume_backend.default_input_uid().ok().flatten()),
        ));
    }

    let devices = host
        .input_devices()
        .map_err(|error| LiveCaptureError::Cpal(error.to_string()))?;
    // First pass: exact id match.
    let mut label_matches: Vec<(cpal::Device, ResolvedInputDevice)> = Vec::new();
    let mut legacy_match: Option<(cpal::Device, ResolvedInputDevice)> = None;
    let label_hint = label_hint.filter(|label| !label.is_empty());
    let core_audio_uid = core_audio_uid.filter(|uid| !uid.is_empty());
    for (index, device) in devices.enumerate() {
        let label = device.name().unwrap_or_default();
        let stable_id = stable_input_device_id(index, &label);
        let candidate_uid = system_devices
            .iter()
            .find(|candidate| candidate.legacy_cpal_id.as_deref() == Some(stable_id.as_str()))
            .map(|candidate| candidate.uid.clone());
        if let Some(requested_uid) = core_audio_uid {
            if candidate_uid.as_deref() == Some(requested_uid) {
                return Ok((
                    device,
                    ResolvedInputDevice {
                        device_id: stable_id,
                        device_name: label,
                        used_fallback_device: false,
                        core_audio_uid: candidate_uid,
                    },
                ));
            }
        } else if stable_id == device_id {
            return Ok((
                device,
                ResolvedInputDevice {
                    device_id: stable_id,
                    device_name: label,
                    used_fallback_device: false,
                    core_audio_uid: candidate_uid,
                },
            ));
        }
        if let Some(hint) = label_hint
            && microphone_labels_match(&label, hint)
        {
            label_matches.push((
                device,
                ResolvedInputDevice {
                    device_id: stable_id.clone(),
                    device_name: label.clone(),
                    used_fallback_device: false,
                    core_audio_uid: candidate_uid.clone(),
                },
            ));
            continue;
        }
        if legacy_match.is_none() && label == device_id {
            legacy_match = Some((
                device,
                ResolvedInputDevice {
                    device_id: stable_id,
                    device_name: label,
                    used_fallback_device: false,
                    core_audio_uid: candidate_uid,
                },
            ));
        }
    }

    if label_matches.len() == 1 {
        return Ok(label_matches.pop().expect("one label match"));
    }
    if core_audio_uid.is_some() {
        return Err(LiveCaptureError::NoInputDevice);
    }
    if let Some(matched) = legacy_match {
        return Ok(matched);
    }

    let Some(default_device) = default_device else {
        return Err(LiveCaptureError::NoInputDevice);
    };
    Ok((
        default_device,
        default_input_resolution(true, volume_backend.default_input_uid().ok().flatten()),
    ))
}

fn default_input_resolution(
    used_fallback_device: bool,
    core_audio_uid: Option<String>,
) -> ResolvedInputDevice {
    ResolvedInputDevice {
        device_id: "default".to_string(),
        device_name: "System Default".to_string(),
        used_fallback_device,
        core_audio_uid,
    }
}

pub fn resolve_input_device_from_candidates(
    requested_device_id: &str,
    label_hint: Option<&str>,
    candidates: &[CandidateInputDevice],
) -> Option<ResolvedInputDevice> {
    resolve_input_device_from_candidates_with_uid(requested_device_id, label_hint, None, candidates)
}

pub fn resolve_input_device_from_candidates_with_uid(
    requested_device_id: &str,
    label_hint: Option<&str>,
    core_audio_uid: Option<&str>,
    candidates: &[CandidateInputDevice],
) -> Option<ResolvedInputDevice> {
    if requested_device_id == "default" {
        return candidates
            .iter()
            .find(|candidate| candidate.is_default)
            .map(|candidate| default_input_resolution(false, candidate.core_audio_uid.clone()));
    }

    let pinned_uid = core_audio_uid.filter(|uid| !uid.is_empty());
    if let Some(uid) = pinned_uid
        && let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.core_audio_uid.as_deref() == Some(uid))
    {
        return Some(resolved_candidate(candidate));
    }

    // Exact id match wins: the stable id is the precise pin and a different
    // device that happens to share a label must not be selected.
    if pinned_uid.is_none()
        && let Some(candidate) = candidates.iter().find(|c| c.id == requested_device_id)
    {
        return Some(resolved_candidate(candidate));
    }

    // Label-based fallback. The stable id embeds the cpal enumeration index,
    // which is not actually stable across reboots / hot-swaps on macOS — but
    // the device label usually is. Match by the persisted label so the same
    // physical mic is re-acquired after its index moves.
    let label_hint = label_hint.filter(|label| !label.is_empty());
    if let Some(label) = label_hint {
        let matches = candidates
            .iter()
            .filter(|candidate| microphone_labels_match(&candidate.label, label))
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return Some(resolved_candidate(matches[0]));
        }
    }

    if pinned_uid.is_some() {
        return None;
    }

    // Legacy: callers used to pass a label string as `requested_device_id`.
    if let Some(candidate) = candidates.iter().find(|c| c.label == requested_device_id) {
        return Some(resolved_candidate(candidate));
    }

    candidates
        .iter()
        .find(|candidate| candidate.is_default)
        .map(|candidate| default_input_resolution(true, candidate.core_audio_uid.clone()))
}

fn resolved_candidate(candidate: &CandidateInputDevice) -> ResolvedInputDevice {
    ResolvedInputDevice {
        device_id: candidate.id.clone(),
        device_name: candidate.label.clone(),
        used_fallback_device: false,
        core_audio_uid: candidate.core_audio_uid.clone(),
    }
}

pub fn microphone_labels_match(left: &str, right: &str) -> bool {
    normalized_microphone_label(left) == normalized_microphone_label(right)
}

fn normalized_microphone_label(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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

#[derive(Default)]
struct MicrophoneFrameEncoder {
    downmixer: MicrophoneDownmixer,
    clock: CaptureFrameClock,
}

impl MicrophoneFrameEncoder {
    fn emit(
        &mut self,
        interleaved: &[f32],
        channels: usize,
        sample_rate: u32,
        info: &cpal::InputCallbackInfo,
        on_frame: &Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) {
        if channels == 0 || sample_rate == 0 || interleaved.len() < channels {
            return;
        }
        let mono = self.downmixer.process(interleaved, channels, sample_rate);
        let captured_at =
            self.clock
                .frame_end(info.timestamp(), mono.len(), sample_rate, Utc::now());
        let duration_ms = ((mono.len() as f64 / sample_rate as f64) * 1000.0).round() as u64;
        on_frame(AudioFrame {
            samples: mono,
            duration_ms: duration_ms.max(1),
            captured_at,
        });
    }
}

#[derive(Default)]
struct CaptureFrameClock {
    wall_anchor: Option<(cpal::StreamInstant, DateTime<Utc>)>,
    sample_anchor: Option<DateTime<Utc>>,
    samples: u64,
}

impl CaptureFrameClock {
    fn frame_end(
        &mut self,
        timestamp: cpal::InputStreamTimestamp,
        samples: usize,
        sample_rate: u32,
        callback_wall_time: DateTime<Utc>,
    ) -> DateTime<Utc> {
        let (stream_anchor, wall_anchor) = *self
            .wall_anchor
            .get_or_insert((timestamp.callback, callback_wall_time));
        let capture_end = timestamp
            .capture
            .add(Duration::from_nanos(
                (samples as u128 * 1_000_000_000 / sample_rate as u128) as u64,
            ))
            .unwrap_or(timestamp.callback);
        let elapsed = if let Some(elapsed) = capture_end.duration_since(&stream_anchor) {
            chrono::Duration::from_std(elapsed).unwrap_or_default()
        } else {
            -chrono::Duration::from_std(
                stream_anchor
                    .duration_since(&capture_end)
                    .unwrap_or_default(),
            )
            .unwrap_or_default()
        };
        let observed_end = wall_anchor + elapsed;
        let sample_duration = |count: u64| {
            chrono::Duration::microseconds((count as u128 * 1_000_000 / sample_rate as u128) as i64)
        };
        let expected_end = self
            .sample_anchor
            .map(|start| start + sample_duration(self.samples + samples as u64));
        // Device timestamps are much steadier than callback arrival times. A
        // complete missing buffer must be a discontinuity, even at small sizes.
        let tolerance_us =
            (samples as u128 * 1_000_000 / sample_rate as u128 / 4).clamp(1, 1_000) as i64;
        if expected_end.is_none_or(|expected| {
            (observed_end - expected)
                .num_microseconds()
                .unwrap_or(i64::MAX)
                .abs()
                > tolerance_us
        }) {
            self.sample_anchor = Some(observed_end - sample_duration(samples as u64));
            self.samples = 0;
        }
        self.samples += samples as u64;
        self.sample_anchor.expect("sample clock initialized") + sample_duration(self.samples)
    }
}

#[derive(Default)]
struct MicrophoneDownmixer {
    selected_channel: Option<usize>,
    left_weight: Option<f32>,
}

impl MicrophoneDownmixer {
    fn process(&mut self, interleaved: &[f32], channels: usize, sample_rate: u32) -> Vec<f32> {
        let finite = |sample: f32| if sample.is_finite() { sample } else { 0.0 };
        if channels != 2 {
            return interleaved
                .chunks_exact(channels)
                .map(|frame| {
                    frame.iter().map(|sample| finite(*sample)).sum::<f32>() / channels as f32
                })
                .collect();
        }
        let mut energy = [0.0_f64; 2];
        let mut clipped = [0_usize; 2];
        let mut cross = 0.0_f64;
        let count = interleaved.len() / 2;
        for frame in interleaved.as_chunks::<2>().0 {
            let left = finite(frame[0]);
            let right = finite(frame[1]);
            for (channel, sample) in [left, right].into_iter().enumerate() {
                energy[channel] += f64::from(sample).powi(2);
                clipped[channel] += usize::from(sample.abs() >= 0.98);
            }
            cross += f64::from(left) * f64::from(right);
        }
        let correlation = cross / (energy[0] * energy[1]).sqrt().max(f64::EPSILON);
        let clipping = clipped.map(|count_clipped| count_clipped as f64 / count as f64 > 0.01);
        let selected = if energy[1] < energy[0] * 0.001 {
            Some(0)
        } else if energy[0] < energy[1] * 0.001 {
            Some(1)
        } else if correlation.abs() > 0.75 {
            match (clipping[0], clipping[1]) {
                (true, false) => Some(1),
                (false, true) => Some(0),
                _ if self.selected_channel.is_some() => self.selected_channel,
                _ if correlation < -0.75 => Some(0),
                _ => None,
            }
        } else if energy.iter().all(|energy| *energy < 1e-10) {
            self.selected_channel
        } else {
            None // Independent stereo transmitters must retain both voices.
        };
        self.selected_channel = selected;
        let target = match selected {
            Some(0) => 1.0,
            Some(_) => 0.0,
            None => 0.5,
        };
        let mut weight = self.left_weight.unwrap_or(target);
        let step = 1.0 / (sample_rate as f32 * 0.02).max(1.0);
        let mono = interleaved
            .as_chunks::<2>()
            .0
            .iter()
            .map(|frame| {
                weight += (target - weight).clamp(-step, step);
                finite(frame[0]) * weight + finite(frame[1]) * (1.0 - weight)
            })
            .collect();
        self.left_weight = Some(weight);
        mono
    }
}

#[cfg(test)]
mod microphone_frame_tests {
    use super::*;

    fn captured_samples(interleaved: &[f32], channels: usize) -> Vec<f32> {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let received = frames.clone();
        let callback: Arc<dyn Fn(AudioFrame) + Send + Sync> = Arc::new(move |frame| {
            received.lock().unwrap().push(frame);
        });
        let info = cpal::InputCallbackInfo::new(cpal::InputStreamTimestamp {
            callback: cpal::StreamInstant::new(1, 0),
            capture: cpal::StreamInstant::new(0, 990_000_000),
        });
        MicrophoneFrameEncoder::default().emit(interleaved, channels, 48_000, &info, &callback);
        frames.lock().unwrap()[0].samples.clone()
    }

    #[test]
    fn usb_microphone_uses_the_clean_safety_channel_when_main_channel_clips() {
        let source = (0..480)
            .map(|index| (std::f32::consts::TAU * index as f32 / 48.0).sin())
            .collect::<Vec<_>>();
        let interleaved = source
            .iter()
            .flat_map(|sample| [(sample * 1.4).clamp(-1.0, 1.0), sample * 0.5])
            .collect::<Vec<_>>();
        let mono = captured_samples(&interleaved, 2);
        let error = mono
            .iter()
            .zip(&source)
            .map(|(actual, original)| (actual - original * 0.5).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            error < 0.0001,
            "clipped main track contaminated the safety track: {error}"
        );
    }

    #[test]
    fn usb_microphone_does_not_attenuate_a_single_active_stereo_channel() {
        let mono = captured_samples(&[0.2, 0.0, -0.4, 0.0, 0.6, 0.0], 2);
        assert_eq!(mono, vec![0.2, -0.4, 0.6]);
    }

    #[test]
    fn microphone_downmix_does_not_cancel_opposite_polarity_voice() {
        let mono = captured_samples(&[0.2, -0.2, -0.4, 0.4, 0.6, -0.6], 2);
        assert_eq!(mono, vec![0.2, -0.4, 0.6]);
    }

    #[test]
    fn microphone_clock_uses_capture_time_despite_callback_delays_and_wall_clock_changes() {
        let mut clock = CaptureFrameClock::default();
        let base = DateTime::<Utc>::UNIX_EPOCH;
        for chunk in 0..100_u32 {
            let capture = cpal::StreamInstant::new(1, chunk * 10_000_000);
            let timestamp = cpal::InputStreamTimestamp {
                capture,
                callback: capture.add(Duration::from_millis(10)).unwrap(),
            };
            let wall_time = if chunk == 0 {
                base
            } else {
                base + chrono::Duration::seconds(100)
            };
            let end = clock.frame_end(timestamp, 480, 48_000, wall_time);
            assert_eq!(
                end,
                base + chrono::Duration::milliseconds(chunk as i64 * 10)
            );
        }
    }

    #[test]
    fn microphone_clock_preserves_real_capture_gaps() {
        let mut clock = CaptureFrameClock::default();
        let first = cpal::InputStreamTimestamp {
            capture: cpal::StreamInstant::new(1, 0),
            callback: cpal::StreamInstant::new(1, 10_000_000),
        };
        let second = cpal::InputStreamTimestamp {
            capture: cpal::StreamInstant::new(2, 0),
            callback: cpal::StreamInstant::new(2, 10_000_000),
        };
        let base = DateTime::<Utc>::UNIX_EPOCH;
        assert_eq!(clock.frame_end(first, 480, 48_000, base), base);
        assert_eq!(
            clock.frame_end(second, 480, 48_000, base),
            base + chrono::Duration::seconds(1)
        );
    }

    #[test]
    fn stereo_transmitters_with_different_voices_are_both_retained() {
        let mono = captured_samples(&[0.4, 0.1, -0.4, 0.1, 0.4, -0.1, -0.4, -0.1], 2);
        assert_eq!(mono, vec![0.25, -0.15, 0.15, -0.25]);
    }

    #[test]
    fn safety_channel_selection_does_not_pump_when_the_main_track_recovers() {
        let mut mixer = MicrophoneDownmixer::default();
        let clipped = (0..480)
            .flat_map(|index| {
                let sample = (std::f32::consts::TAU * index as f32 / 48.0).sin();
                [(sample * 1.4).clamp(-1.0, 1.0), sample * 0.5]
            })
            .collect::<Vec<_>>();
        mixer.process(&clipped, 2, 48_000);
        let recovered = (0..480)
            .flat_map(|index| {
                let sample = (std::f32::consts::TAU * index as f32 / 48.0).sin();
                [sample * 0.8, sample * 0.4]
            })
            .collect::<Vec<_>>();
        let mono = mixer.process(&recovered, 2, 48_000);
        for (actual, stereo) in mono.iter().zip(recovered.chunks_exact(2)) {
            assert!((actual - stereo[1]).abs() < 0.0001);
        }
    }

    #[test]
    fn microphone_clock_preserves_one_missing_ten_millisecond_buffer() {
        let mut clock = CaptureFrameClock::default();
        let base = DateTime::<Utc>::UNIX_EPOCH;
        for (chunk, expected_ms) in [(0_u32, 0), (2, 20), (3, 30)] {
            let capture = cpal::StreamInstant::new(1, chunk * 10_000_000);
            let timestamp = cpal::InputStreamTimestamp {
                capture,
                callback: capture.add(Duration::from_millis(10)).unwrap(),
            };
            assert_eq!(
                clock.frame_end(timestamp, 480, 48_000, base),
                base + chrono::Duration::milliseconds(expected_ms)
            );
        }
    }
}
