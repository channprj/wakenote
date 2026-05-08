use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use sagwan::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, AudioStreamHandle, CandidateInputDevice,
    LiveCaptureError, LiveCaptureRuntime, resolve_input_device_from_candidates,
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
    let (received_tx, received_rx) = mpsc::channel();
    let mut runtime = LiveCaptureRuntime::new(input);

    runtime
        .start(AudioInputConfig::default(), move |frame| {
            received_tx.send(frame).expect("received frame");
        })
        .expect("start");
    emitter.emit(AudioFrame {
        samples: vec![0.1, -0.1, 0.2],
        duration_ms: 20,
    });

    let received = received_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("delivered frame");
    assert!(runtime.is_running());
    assert_eq!(emitter.starts(), 1);
    assert_eq!(received.duration_ms, 20);
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

#[test]
fn live_capture_runtime_does_not_block_input_callback_when_processing_is_busy() {
    let input = FakeInput::default();
    let emitter = input.clone();
    let mut runtime = LiveCaptureRuntime::new(input);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (returned_tx, returned_rx) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release_clone = release.clone();

    runtime
        .start(AudioInputConfig::default(), move |frame| {
            entered_tx.send(frame.duration_ms).expect("entered");
            let (lock, cvar) = &*release_clone;
            let mut released = lock.lock().expect("release lock");
            while !*released {
                released = cvar.wait(released).expect("release wait");
            }
        })
        .expect("start");

    let first_emitter = emitter.clone();
    let first = std::thread::spawn(move || {
        first_emitter.emit(AudioFrame {
            samples: vec![0.1],
            duration_ms: 10,
        });
    });
    assert_eq!(entered_rx.recv_timeout(Duration::from_secs(1)), Ok(10));

    let second_emitter = emitter.clone();
    let second = std::thread::spawn(move || {
        second_emitter.emit(AudioFrame {
            samples: vec![0.2],
            duration_ms: 20,
        });
        returned_tx.send(()).expect("returned");
    });

    let returned_without_waiting_for_processing =
        returned_rx.recv_timeout(Duration::from_millis(50)).is_ok();
    {
        let (lock, cvar) = &*release;
        *lock.lock().expect("release lock") = true;
        cvar.notify_all();
    }
    first.join().expect("first emit");
    second.join().expect("second emit");

    assert!(returned_without_waiting_for_processing);
}

#[test]
fn live_capture_runtime_drops_stale_frames_when_processing_falls_behind() {
    let input = FakeInput::default();
    let emitter = input.clone();
    let mut runtime = LiveCaptureRuntime::new(input);
    let (entered_tx, entered_rx) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release_clone = release.clone();

    runtime
        .start(AudioInputConfig::default(), move |frame| {
            let _ = entered_tx.send(frame.duration_ms);
            if frame.duration_ms == 0 {
                let (lock, cvar) = &*release_clone;
                let mut released = lock.lock().expect("release lock");
                while !*released {
                    released = cvar.wait(released).expect("release wait");
                }
            }
        })
        .expect("start");

    emitter.emit(AudioFrame {
        samples: vec![0.1],
        duration_ms: 0,
    });
    assert_eq!(entered_rx.recv_timeout(Duration::from_secs(1)), Ok(0));

    for duration_ms in 1..=600 {
        emitter.emit(AudioFrame {
            samples: vec![0.2],
            duration_ms,
        });
    }

    {
        let (lock, cvar) = &*release;
        *lock.lock().expect("release lock") = true;
        cvar.notify_all();
    }

    let first_after_busy = entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first frame after busy processing");
    assert!(first_after_busy > 1);
    assert!(runtime.dropped_frame_count() > 0);
}

#[test]
fn input_device_resolution_marks_fallback_when_pinned_device_is_missing() {
    let resolved = resolve_input_device_from_candidates(
        "input-9-missing-airpods",
        &[
            CandidateInputDevice {
                id: "input-0-built-in".to_string(),
                label: "Built-in Microphone".to_string(),
                is_default: true,
            },
            CandidateInputDevice {
                id: "input-1-usb".to_string(),
                label: "USB Mic".to_string(),
                is_default: false,
            },
        ],
    )
    .expect("fallback device");

    assert_eq!(resolved.device_id, "default");
    assert_eq!(resolved.device_name, "System Default");
    assert!(resolved.used_fallback_device);
}
