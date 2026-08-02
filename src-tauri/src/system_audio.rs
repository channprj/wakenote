//! macOS system-audio capture backend built on ScreenCaptureKit.
//!
//! [`SystemAudioInput`] is a sibling of [`crate::live_capture::CpalAudioInput`]:
//! it implements [`AudioInputBackend`] so the same live-capture pipeline can be
//! fed from a specific application's audio (a browser running Google Meet /
//! YouTube) instead of the microphone.
//!
//! The capture target is the process id of the application to record, set via
//! [`SystemAudioInput::set_target_app`] before `start` (the cpal-shaped
//! [`AudioInputConfig`] carries no app identity, so the target lives on the
//! backend rather than rippling a new field into the microphone path).
//!
//! ScreenCaptureKit audio capture requires macOS 13+ and Screen Recording
//! permission. On non-macOS targets `start` returns an `Unsupported` error.

use std::sync::Arc;

use crate::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, AudioStreamHandle, LiveCaptureError,
};
use crate::source_watcher::WindowSnapshot;

/// Sample rate fed into the transcription pipeline. ScreenCaptureKit is asked
/// to deliver audio at this rate so no resampling is required downstream.
pub const PIPELINE_SAMPLE_RATE: u32 = 16_000;

/// ScreenCaptureKit system-audio capture backend.
///
/// Set [`SystemAudioInput::set_target_app`] to the application's process id
/// and app name before calling `start`; without it the whole-system mix is
/// captured.
#[derive(Debug, Default)]
pub struct SystemAudioInput {
    target: Option<SystemAudioTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SystemAudioTarget {
    pid: i32,
    app_name: Option<String>,
}

impl SystemAudioInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pin capture to a specific application by process id. Must be called
    /// before `start`; changing it after a stream is running has no effect.
    pub fn set_target_pid(&mut self, pid: i32) {
        self.target = Some(SystemAudioTarget {
            pid,
            app_name: None,
        });
    }

    /// Pin capture to a specific application and related helper processes.
    /// Browser audio is often emitted by helper processes, so the macOS filter
    /// uses the app name to include the target app family instead of only the
    /// window-owning pid.
    pub fn set_target_app(&mut self, pid: i32, app_name: impl Into<String>) {
        self.target = Some(SystemAudioTarget {
            pid,
            app_name: Some(app_name.into()),
        });
    }

    pub fn target_pid(&self) -> Option<i32> {
        self.target.as_ref().map(|target| target.pid)
    }
}

impl AudioInputBackend for SystemAudioInput {
    fn start(
        &mut self,
        config: AudioInputConfig,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        let sample_rate = config.sample_rate.unwrap_or(PIPELINE_SAMPLE_RATE);
        macos::start_system_audio(self.target.clone(), sample_rate, on_frame)
    }
}

fn is_related_application(
    target_name: Option<&str>,
    target_bundle: Option<&str>,
    candidate_name: &str,
    candidate_bundle: &str,
) -> bool {
    if let Some(target_name) = target_name
        && !target_name.is_empty()
        && (candidate_name == target_name
            || candidate_name
                .strip_prefix(target_name)
                .map(|suffix| suffix.starts_with(' ') || suffix.starts_with(" Helper"))
                .unwrap_or(false))
    {
        return true;
    }

    if let Some(target_bundle) = target_bundle
        && !target_bundle.is_empty()
        && (candidate_bundle == target_bundle
            || candidate_bundle
                .strip_prefix(target_bundle)
                .map(|suffix| suffix.starts_with('.') || suffix.starts_with('-'))
                .unwrap_or(false))
    {
        return true;
    }

    false
}

/// Snapshot of the on-screen windows for source detection. Queries the same
/// `SCShareableContent` the capture path uses; requires Screen Recording
/// permission to return window titles. Returns an empty vec on non-macOS or on
/// any query failure (e.g. permission not yet granted) so the watcher polls
/// safely without surfacing transient errors.
pub fn enumerate_windows() -> Vec<WindowSnapshot> {
    macos::enumerate_windows()
}

/// Downmix an interleaved multi-channel f32 buffer to mono by averaging the
/// channels of each frame. Mirrors the cpal path's `emit_f32_frame` downmix so
/// both backends hand identical-shaped mono frames to the pipeline.
fn downmix_interleaved_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if interleaved.is_empty() || channels == 0 {
        return Vec::new();
    }
    if channels == 1 {
        return interleaved.to_vec();
    }
    let mut mono = Vec::with_capacity(interleaved.len() / channels);
    for frame in interleaved.chunks(channels) {
        let sum: f32 = frame.iter().copied().sum();
        mono.push(sum / frame.len() as f32);
    }
    mono
}

/// Duration in milliseconds of a mono buffer at the given sample rate, clamped
/// to at least 1ms so a frame never reports a zero duration.
fn frame_duration_ms(mono_len: usize, sample_rate: u32) -> u64 {
    if sample_rate == 0 || mono_len == 0 {
        return 1;
    }
    let ms = (mono_len as f64 / sample_rate as f64 * 1000.0).round() as u64;
    ms.max(1)
}

#[cfg(not(target_os = "macos"))]
mod macos {
    use super::*;

    pub(super) fn start_system_audio(
        _target: Option<SystemAudioTarget>,
        _sample_rate: u32,
        _on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        Err(LiveCaptureError::Cpal(
            "system-audio capture is only supported on macOS".to_string(),
        ))
    }

    pub(super) fn enumerate_windows() -> Vec<WindowSnapshot> {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use std::ptr;

    use block2::RcBlock;
    use chrono::Utc;
    use dispatch2::{DispatchQueue, DispatchQueueAttr};
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{AnyThread, DefinedClass, define_class, msg_send};
    use objc2_core_audio_types::AudioBufferList;
    use objc2_core_media::{CMBlockBuffer, CMSampleBuffer};
    use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
    use objc2_screen_capture_kit::{
        SCContentFilter, SCRunningApplication, SCShareableContent, SCStream, SCStreamConfiguration,
        SCStreamDelegate, SCStreamOutput, SCStreamOutputType,
    };

    use super::{
        AudioFrame, AudioStreamHandle, LiveCaptureError, SystemAudioTarget, WindowSnapshot,
        downmix_interleaved_to_mono, frame_duration_ms, is_related_application,
    };

    const SHAREABLE_CONTENT_TIMEOUT: Duration = Duration::from_secs(10);
    const STREAM_START_TIMEOUT: Duration = Duration::from_secs(10);
    /// Mono output: ScreenCaptureKit downmixes to the requested channel count.
    const OUTPUT_CHANNELS: i64 = 1;

    /// Ivars for the stream output/delegate object.
    struct DelegateIvars {
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
        sample_rate: u32,
        runtime_error: Arc<Mutex<Option<String>>>,
    }

    define_class!(
        // SAFETY:
        // - The superclass NSObject has no subclassing requirements.
        // - This class does not implement `Drop`.
        #[unsafe(super(NSObject))]
        #[ivars = DelegateIvars]
        struct StreamOutput;

        unsafe impl NSObjectProtocol for StreamOutput {}

        unsafe impl SCStreamOutput for StreamOutput {
            #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
            fn stream_did_output_sample_buffer(
                &self,
                _stream: &SCStream,
                sample_buffer: &CMSampleBuffer,
                of_type: SCStreamOutputType,
            ) {
                if of_type != SCStreamOutputType::Audio {
                    return;
                }
                self.handle_audio_sample(sample_buffer);
            }
        }

        unsafe impl SCStreamDelegate for StreamOutput {
            #[unsafe(method(stream:didStopWithError:))]
            fn stream_did_stop_with_error(&self, _stream: &SCStream, error: &NSError) {
                if let Ok(mut slot) = self.ivars().runtime_error.lock() {
                    *slot = Some(error.localizedDescription().to_string());
                }
            }
        }
    );

    impl StreamOutput {
        fn new(
            on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
            sample_rate: u32,
            runtime_error: Arc<Mutex<Option<String>>>,
        ) -> Retained<Self> {
            let this = Self::alloc().set_ivars(DelegateIvars {
                on_frame,
                sample_rate,
                runtime_error,
            });
            unsafe { msg_send![super(this), init] }
        }

        fn handle_audio_sample(&self, sample_buffer: &CMSampleBuffer) {
            let ivars = self.ivars();
            match extract_mono_samples(sample_buffer) {
                Ok(mono) if !mono.is_empty() => {
                    let duration_ms = frame_duration_ms(mono.len(), ivars.sample_rate);
                    (ivars.on_frame)(AudioFrame {
                        samples: mono,
                        duration_ms,
                        captured_at: Utc::now(),
                    });
                }
                Ok(_) => {}
                Err(error) => {
                    if let Ok(mut slot) = ivars.runtime_error.lock()
                        && slot.is_none()
                    {
                        *slot = Some(error);
                    }
                }
            }
        }
    }

    /// Live handle keeping the SCStream and its delegate alive. Dropping it
    /// stops capture.
    struct SystemAudioStreamHandle {
        stream: Retained<SCStream>,
        _output: Retained<StreamOutput>,
        runtime_error: Arc<Mutex<Option<String>>>,
    }

    // SCStream and the delegate are only ever touched on the dispatch queue we
    // own plus this owning thread; the ScreenCaptureKit objects are internally
    // thread-safe for start/stop. The raw pointers in `Retained` are otherwise
    // not `Send`, so assert it explicitly for the handle we move across threads.
    unsafe impl Send for SystemAudioStreamHandle {}

    impl AudioStreamHandle for SystemAudioStreamHandle {
        fn runtime_error(&self) -> Option<String> {
            self.runtime_error.lock().ok()?.clone()
        }
    }

    impl Drop for SystemAudioStreamHandle {
        fn drop(&mut self) {
            unsafe {
                self.stream.stopCaptureWithCompletionHandler(None);
            }
        }
    }

    pub(super) fn start_system_audio(
        target: Option<SystemAudioTarget>,
        sample_rate: u32,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        let content = fetch_shareable_content()?;
        let filter = build_content_filter(&content, target.as_ref())?;
        let config = build_stream_configuration(sample_rate);

        let runtime_error = Arc::new(Mutex::new(None));
        let output = StreamOutput::new(on_frame, sample_rate, runtime_error.clone());
        let delegate = ProtocolObject::from_ref(&*output);

        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &config,
                Some(delegate),
            )
        };

        let queue = DispatchQueue::new(
            "io.portrai.wakenote.system-audio",
            DispatchQueueAttr::SERIAL,
        );
        let stream_output = ProtocolObject::from_ref(&*output);
        unsafe {
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    stream_output,
                    SCStreamOutputType::Audio,
                    Some(&queue),
                )
                .map_err(|error| {
                    LiveCaptureError::Cpal(format!(
                        "failed to add audio stream output: {}",
                        error.localizedDescription()
                    ))
                })?;
        }

        start_capture_blocking(&stream)?;

        Ok(Box::new(SystemAudioStreamHandle {
            stream,
            _output: output,
            runtime_error,
        }))
    }

    /// Enumerate the on-screen windows visible to ScreenCaptureKit, reduced to
    /// the title / owning-app name / owning-app pid the detector needs. Returns
    /// empty when shareable content cannot be queried (e.g. Screen Recording
    /// permission not yet granted) so the watcher treats it as "nothing on
    /// screen" rather than an error.
    pub(super) fn enumerate_windows() -> Vec<WindowSnapshot> {
        let Ok(content) = fetch_shareable_content() else {
            return Vec::new();
        };
        let windows = unsafe { content.windows() };
        windows
            .iter()
            .filter(|window| unsafe { window.isOnScreen() })
            .filter_map(|window| {
                let title = unsafe { window.title() }?.to_string();
                let app = unsafe { window.owningApplication() }?;
                Some(WindowSnapshot {
                    title,
                    app_name: unsafe { app.applicationName() }.to_string(),
                    pid: unsafe { app.processID() },
                })
            })
            .collect()
    }

    /// Resolve the running applications/displays available to capture.
    /// `getShareableContentWithCompletionHandler:` is async; block on it.
    fn fetch_shareable_content() -> Result<Retained<SCShareableContent>, LiveCaptureError> {
        let started_at = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        let handler = RcBlock::new(
            move |content: *mut SCShareableContent, error: *mut NSError| {
                let result = if content.is_null() {
                    let message = unsafe { error.as_ref() }
                        .map(|error| error.localizedDescription().to_string())
                        .unwrap_or_else(|| "no shareable content".to_string());
                    Err(message)
                } else {
                    Ok(unsafe { Retained::retain(content) })
                };
                let _ = tx.send(result);
            },
        );
        unsafe {
            SCShareableContent::getShareableContentWithCompletionHandler(&handler);
        }

        match rx.recv_timeout(SHAREABLE_CONTENT_TIMEOUT) {
            Ok(Ok(Some(content))) => Ok(content),
            Ok(Ok(None)) => Err(LiveCaptureError::Cpal(format!(
                "shareable content was null elapsed_ms={}",
                started_at.elapsed().as_millis()
            ))),
            Ok(Err(message)) => Err(LiveCaptureError::Cpal(format!(
                "could not query shareable content elapsed_ms={}: {message}",
                started_at.elapsed().as_millis()
            ))),
            Err(_) => Err(LiveCaptureError::Cpal(format!(
                "timed out querying shareable content elapsed_ms={} (Screen Recording permission?)",
                started_at.elapsed().as_millis()
            ))),
        }
    }

    /// Build a content filter that captures the target application's audio. The
    /// filter is display-scoped (audio capture requires a display) and includes
    /// the target app plus related helper apps; with no target, captures the
    /// full mix.
    fn build_content_filter(
        content: &SCShareableContent,
        target: Option<&SystemAudioTarget>,
    ) -> Result<Retained<SCContentFilter>, LiveCaptureError> {
        let displays = unsafe { content.displays() };
        let Some(display) = displays.firstObject() else {
            return Err(LiveCaptureError::Cpal(
                "no display available for system-audio capture".to_string(),
            ));
        };

        let applications = unsafe { content.applications() };
        let included = match target {
            Some(target) => {
                let apps = find_related_applications(&applications, target);
                if apps.is_empty() {
                    return Err(LiveCaptureError::Cpal(format!(
                        "application with pid {} is not available for capture",
                        target.pid
                    )));
                }
                NSArray::from_retained_slice(&apps)
            }
            None => applications,
        };

        let empty_windows = NSArray::new();
        let filter = unsafe {
            SCContentFilter::initWithDisplay_includingApplications_exceptingWindows(
                SCContentFilter::alloc(),
                &display,
                &included,
                &empty_windows,
            )
        };
        Ok(filter)
    }

    fn find_related_applications(
        applications: &NSArray<SCRunningApplication>,
        target: &SystemAudioTarget,
    ) -> Vec<Retained<SCRunningApplication>> {
        let primary = find_application_by_pid(applications, target.pid);
        let primary_bundle = primary
            .as_ref()
            .map(|app| unsafe { app.bundleIdentifier() }.to_string());
        let target_name = target.app_name.as_deref();

        let mut related = Vec::new();
        for app in applications.iter() {
            let pid = unsafe { app.processID() };
            let name = unsafe { app.applicationName() }.to_string();
            let bundle = unsafe { app.bundleIdentifier() }.to_string();
            if pid == target.pid
                || is_related_application(target_name, primary_bundle.as_deref(), &name, &bundle)
            {
                related.push(app);
            }
        }

        if related.is_empty()
            && let Some(app) = primary
        {
            related.push(app);
        }

        related
    }

    fn find_application_by_pid(
        applications: &NSArray<SCRunningApplication>,
        pid: i32,
    ) -> Option<Retained<SCRunningApplication>> {
        applications
            .iter()
            .find(|app| unsafe { app.processID() } == pid)
    }

    fn build_stream_configuration(sample_rate: u32) -> Retained<SCStreamConfiguration> {
        let config = unsafe { SCStreamConfiguration::new() };
        unsafe {
            config.setCapturesAudio(true);
            config.setSampleRate(sample_rate as isize);
            config.setChannelCount(OUTPUT_CHANNELS as isize);
            config.setExcludesCurrentProcessAudio(true);
        }
        config
    }

    fn start_capture_blocking(stream: &SCStream) -> Result<(), LiveCaptureError> {
        let started_at = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        let handler = RcBlock::new(move |error: *mut NSError| {
            let result = unsafe { error.as_ref() }
                .map(|error| Err(error.localizedDescription().to_string()))
                .unwrap_or(Ok(()));
            let _ = tx.send(result);
        });
        unsafe {
            stream.startCaptureWithCompletionHandler(Some(&handler));
        }

        match rx.recv_timeout(STREAM_START_TIMEOUT) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => Err(LiveCaptureError::Cpal(format!(
                "failed to start system-audio capture elapsed_ms={}: {message}",
                started_at.elapsed().as_millis()
            ))),
            Err(_) => Err(LiveCaptureError::Cpal(format!(
                "timed out starting system-audio capture elapsed_ms={}",
                started_at.elapsed().as_millis()
            ))),
        }
    }

    /// Extract a mono f32 buffer from an audio `CMSampleBuffer`.
    ///
    /// ScreenCaptureKit is configured to deliver interleaved Float32 PCM (mono,
    /// per [`build_stream_configuration`]). We pull the data through a single
    /// retained [`CMBlockBuffer`] so the [`AudioBufferList`] stays valid for the
    /// read, interpret `mBuffers[0]` as `&[f32]`, and downmix to mono using the
    /// reported channel count.
    ///
    /// TODO(on-device): the `mBuffers[0]` byte layout (interleaved vs. planar,
    /// actual channel count returned by ScreenCaptureKit) can only be confirmed
    /// against live capture on a real Mac with Screen Recording permission. The
    /// interleaved-Float32 assumption matches the configured stream format; if
    /// on-device testing shows non-interleaved (planar) buffers, iterate over
    /// `mNumberBuffers` here instead of treating buffer 0 as interleaved.
    fn extract_mono_samples(sample_buffer: &CMSampleBuffer) -> Result<Vec<f32>, String> {
        let mut buffer_list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [objc2_core_audio_types::AudioBuffer {
                mNumberChannels: 0,
                mDataByteSize: 0,
                mData: ptr::null_mut(),
            }],
        };
        let mut block_buffer: *mut CMBlockBuffer = ptr::null_mut();

        // SAFETY: `buffer_list` is a valid single-buffer AudioBufferList of the
        // matching size; `block_buffer` receives a +1 retained CMBlockBuffer
        // that owns the data and is released when `_block_buffer` drops below.
        let status = unsafe {
            sample_buffer.audio_buffer_list_with_retained_block_buffer(
                ptr::null_mut(),
                &mut buffer_list,
                size_of::<AudioBufferList>(),
                None,
                None,
                0,
                &mut block_buffer,
            )
        };
        if status != 0 {
            return Err(format!("CMSampleBuffer audio buffer list error: {status}"));
        }
        if block_buffer.is_null() {
            return Err("CMSampleBuffer returned no block buffer".to_string());
        }
        // Reclaim ownership so the backing data is released on drop.
        let _block_buffer = unsafe { Retained::from_raw(block_buffer) };

        let audio_buffer = buffer_list.mBuffers[0];
        let channels = audio_buffer.mNumberChannels.max(1) as usize;
        let byte_len = audio_buffer.mDataByteSize as usize;
        if audio_buffer.mData.is_null() || byte_len == 0 {
            return Ok(Vec::new());
        }
        let sample_count = byte_len / size_of::<f32>();
        // SAFETY: mData points to `byte_len` bytes of Float32 PCM owned by the
        // retained block buffer kept alive for the duration of this read.
        let interleaved =
            unsafe { std::slice::from_raw_parts(audio_buffer.mData as *const f32, sample_count) };
        Ok(downmix_interleaved_to_mono(interleaved, channels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_stereo_averages_channels() {
        let interleaved = [0.0, 1.0, 0.5, -0.5, 1.0, -1.0];
        let mono = downmix_interleaved_to_mono(&interleaved, 2);
        assert_eq!(mono, vec![0.5, 0.0, 0.0]);
    }

    #[test]
    fn downmix_mono_is_passthrough() {
        let interleaved = [0.1, -0.2, 0.3];
        let mono = downmix_interleaved_to_mono(&interleaved, 1);
        assert_eq!(mono, interleaved.to_vec());
    }

    #[test]
    fn downmix_handles_empty_or_zero_channels() {
        assert!(downmix_interleaved_to_mono(&[], 2).is_empty());
        assert!(downmix_interleaved_to_mono(&[1.0, 2.0], 0).is_empty());
    }

    #[test]
    fn frame_duration_matches_sample_count() {
        assert_eq!(frame_duration_ms(16_000, 16_000), 1000);
        assert_eq!(frame_duration_ms(8_000, 16_000), 500);
    }

    #[test]
    fn frame_duration_never_zero() {
        assert_eq!(frame_duration_ms(0, 16_000), 1);
        assert_eq!(frame_duration_ms(1, 16_000), 1);
        assert_eq!(frame_duration_ms(100, 0), 1);
    }

    #[test]
    fn target_pid_round_trips() {
        let mut input = SystemAudioInput::new();
        assert_eq!(input.target_pid(), None);
        input.set_target_pid(4321);
        assert_eq!(input.target_pid(), Some(4321));
    }

    #[test]
    fn related_application_includes_browser_helpers() {
        assert!(is_related_application(
            Some("Google Chrome"),
            Some("com.google.Chrome"),
            "Google Chrome Helper",
            "com.google.Chrome.helper",
        ));
        assert!(is_related_application(
            Some("Microsoft Edge"),
            Some("com.microsoft.edgemac"),
            "Microsoft Edge Helper",
            "com.microsoft.edgemac.helper",
        ));
        assert!(!is_related_application(
            Some("Google Chrome"),
            Some("com.google.Chrome"),
            "Spotify",
            "com.spotify.client",
        ));
    }
}
