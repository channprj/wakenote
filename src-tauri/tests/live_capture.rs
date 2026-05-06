use std::sync::{Arc, Mutex};

use sagwan::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, AudioStreamHandle, LiveCaptureError,
    LiveCaptureRuntime,
};

#[derive(Clone, Default)]
struct FakeInput {
    callback: Arc<Mutex<Option<Arc<dyn Fn(AudioFrame) + Send + Sync>>>>,
    starts: Arc<Mutex<usize>>,
}

struct FakeHandle;

impl AudioStreamHandle for FakeHandle {}

impl FakeInput {
    fn emit(&self, frame: AudioFrame) {
        let callback = self.callback.lock().unwrap().clone().expect("callback");
        callback(frame);
    }

    fn starts(&self) -> usize {
        *self.starts.lock().unwrap()
    }
}

impl AudioInputBackend for FakeInput {
    fn start(
        &mut self,
        _config: AudioInputConfig,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        *self.starts.lock().unwrap() += 1;
        *self.callback.lock().unwrap() = Some(on_frame);
        Ok(Box::new(FakeHandle))
    }
}

#[test]
fn live_capture_runtime_starts_once_and_delivers_audio_frames() {
    let input = FakeInput::default();
    let emitter = input.clone();
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();
    let mut runtime = LiveCaptureRuntime::new(input);

    runtime
        .start(AudioInputConfig::default(), move |frame| {
            received_clone.lock().unwrap().push(frame);
        })
        .expect("start");
    emitter.emit(AudioFrame {
        samples: vec![0.1, -0.1, 0.2],
        duration_ms: 20,
    });

    assert!(runtime.is_running());
    assert_eq!(emitter.starts(), 1);
    assert_eq!(received.lock().unwrap().len(), 1);
    assert_eq!(received.lock().unwrap()[0].duration_ms, 20);
}

#[test]
fn live_capture_runtime_rejects_double_start_and_can_stop() {
    let input = FakeInput::default();
    let mut runtime = LiveCaptureRuntime::new(input);

    runtime
        .start(AudioInputConfig::default(), |_| {})
        .expect("start");
    let second = runtime.start(AudioInputConfig::default(), |_| {});

    assert!(matches!(second, Err(LiveCaptureError::AlreadyRunning)));
    runtime.stop();
    assert!(!runtime.is_running());
}
