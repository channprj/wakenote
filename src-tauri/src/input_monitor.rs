use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

const MONITOR_BUFFER_MS: u32 = 250;
const MONITOR_STREAM_READY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Error)]
pub enum InputMonitorError {
    #[error("no output device is available")]
    NoOutputDevice,
    #[error("cpal error: {0}")]
    Cpal(String),
}

#[derive(Debug)]
pub struct MonitorBuffer {
    capacity: usize,
    samples: Mutex<VecDeque<f32>>,
    dropped_samples: AtomicU64,
}

impl MonitorBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            samples: Mutex::new(VecDeque::with_capacity(capacity)),
            dropped_samples: AtomicU64::new(0),
        }
    }

    pub fn push(&self, samples: &[f32]) {
        if self.capacity == 0 || samples.is_empty() {
            return;
        }

        let Ok(mut queued) = self.samples.lock() else {
            return;
        };
        if samples.len() >= self.capacity {
            let dropped = queued.len().saturating_add(samples.len() - self.capacity);
            queued.clear();
            queued.extend(samples[samples.len() - self.capacity..].iter().copied());
            self.dropped_samples
                .fetch_add(dropped as u64, Ordering::Relaxed);
            return;
        }

        queued.extend(samples.iter().copied());
        let overflow = queued.len().saturating_sub(self.capacity);
        for _ in 0..overflow {
            queued.pop_front();
        }
        if overflow > 0 {
            self.dropped_samples
                .fetch_add(overflow as u64, Ordering::Relaxed);
        }
    }

    pub fn read(&self, output: &mut [f32]) -> usize {
        output.fill(0.0);
        let Ok(mut queued) = self.samples.lock() else {
            return 0;
        };

        let mut read = 0;
        for slot in output.iter_mut() {
            let Some(sample) = queued.pop_front() else {
                break;
            };
            *slot = sample;
            read += 1;
        }
        read
    }

    pub fn dropped_sample_count(&self) -> u64 {
        self.dropped_samples.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
pub struct InputMonitorRuntime {
    stop_tx: Option<mpsc::Sender<()>>,
    join: Option<thread::JoinHandle<()>>,
    buffer: Option<Arc<MonitorBuffer>>,
    input_sample_rate: u32,
    output_sample_rate: u32,
    runtime_error: Arc<Mutex<Option<String>>>,
}

impl InputMonitorRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, input_sample_rate: u32) -> Result<(), InputMonitorError> {
        if input_sample_rate == 0 {
            return Err(InputMonitorError::Cpal(
                "input sample rate must be non-zero".to_string(),
            ));
        }
        if self.is_running()
            && self.input_sample_rate == input_sample_rate
            && self.runtime_error().is_none()
        {
            return Ok(());
        }

        self.stop();
        let runtime_error = Arc::new(Mutex::new(None));
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop_tx, stop_rx) = mpsc::channel();
        let thread_runtime_error = runtime_error.clone();
        let join = thread::spawn(move || {
            let (stream, buffer, output_sample_rate) =
                match build_output_stream(thread_runtime_error.clone()) {
                    Ok(output) => output,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
            if let Err(error) = stream.play() {
                let _ = ready_tx.send(Err(InputMonitorError::Cpal(error.to_string())));
                return;
            }

            let _ = ready_tx.send(Ok((buffer, output_sample_rate)));
            let _ = stop_rx.recv();
            drop(stream);
        });

        let (buffer, output_sample_rate) = match ready_rx.recv_timeout(MONITOR_STREAM_READY_TIMEOUT)
        {
            Ok(Ok(ready)) => ready,
            Ok(Err(error)) => {
                let _ = join.join();
                return Err(error);
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = stop_tx.send(());
                return Err(InputMonitorError::Cpal(
                    "output device did not become ready within 2 seconds".to_string(),
                ));
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = join.join();
                return Err(InputMonitorError::Cpal(
                    "output device startup disconnected".to_string(),
                ));
            }
        };

        self.stop_tx = Some(stop_tx);
        self.join = Some(join);
        self.buffer = Some(buffer);
        self.input_sample_rate = input_sample_rate;
        self.output_sample_rate = output_sample_rate;
        self.runtime_error = runtime_error;
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.buffer = None;
        self.input_sample_rate = 0;
        self.output_sample_rate = 0;
        if let Ok(mut runtime_error) = self.runtime_error.lock() {
            *runtime_error = None;
        }
    }

    pub fn is_running(&self) -> bool {
        self.stop_tx.is_some()
    }

    pub fn feed(&self, samples: &[f32]) {
        let Some(buffer) = self.buffer.as_ref() else {
            return;
        };
        let output_samples =
            resample_for_output(samples, self.input_sample_rate, self.output_sample_rate);
        buffer.push(&output_samples);
    }

    pub fn runtime_error(&self) -> Option<String> {
        self.runtime_error.lock().ok()?.clone()
    }

    pub fn dropped_sample_count(&self) -> u64 {
        self.buffer
            .as_ref()
            .map(|buffer| buffer.dropped_sample_count())
            .unwrap_or(0)
    }
}

impl Drop for InputMonitorRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn build_output_stream(
    runtime_error: Arc<Mutex<Option<String>>>,
) -> Result<(cpal::Stream, Arc<MonitorBuffer>, u32), InputMonitorError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(InputMonitorError::NoOutputDevice)?;
    let supported_config = device
        .default_output_config()
        .map_err(|error| InputMonitorError::Cpal(error.to_string()))?;
    let stream_config: cpal::StreamConfig = supported_config.clone().into();
    let channels = usize::from(stream_config.channels);
    let output_sample_rate = stream_config.sample_rate.0;
    let capacity = samples_for_duration_ms(output_sample_rate, MONITOR_BUFFER_MS);
    let buffer = Arc::new(MonitorBuffer::new(capacity.max(1)));
    let err_fn = move |error: cpal::StreamError| {
        if let Ok(mut runtime_error) = runtime_error.lock() {
            *runtime_error = Some(error.to_string());
        }
        eprintln!("input monitor output stream error: {error}");
    };

    let stream = match supported_config.sample_format() {
        cpal::SampleFormat::F32 => {
            let buffer = buffer.clone();
            device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    write_output(data, channels, &buffer, |slot, sample| {
                        *slot = sample;
                    })
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let buffer = buffer.clone();
            device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    write_output(data, channels, &buffer, |slot, sample| {
                        *slot = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    });
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let buffer = buffer.clone();
            device.build_output_stream(
                &stream_config,
                move |data: &mut [u16], _| {
                    write_output(data, channels, &buffer, |slot, sample| {
                        let normalized = sample.clamp(-1.0, 1.0) * 0.5 + 0.5;
                        *slot = (normalized * u16::MAX as f32) as u16;
                    });
                },
                err_fn,
                None,
            )
        }
        other => {
            return Err(InputMonitorError::Cpal(format!(
                "unsupported output sample format {other:?}"
            )));
        }
    }
    .map_err(|error| InputMonitorError::Cpal(error.to_string()))?;

    Ok((stream, buffer, output_sample_rate))
}

fn write_output<T>(
    data: &mut [T],
    channels: usize,
    buffer: &MonitorBuffer,
    mut write_sample: impl FnMut(&mut T, f32),
) {
    if data.is_empty() || channels == 0 {
        return;
    }
    let frames = data.len() / channels;
    let mut mono = vec![0.0; frames];
    buffer.read(&mut mono);

    for (frame, sample) in data.chunks_mut(channels).zip(mono) {
        for slot in frame {
            write_sample(slot, sample);
        }
    }
}

pub fn resample_for_output(
    samples: &[f32],
    input_sample_rate: u32,
    output_sample_rate: u32,
) -> Vec<f32> {
    if samples.is_empty() || input_sample_rate == 0 || output_sample_rate == 0 {
        return Vec::new();
    }
    if input_sample_rate == output_sample_rate {
        return samples.to_vec();
    }

    let output_len = ((samples.len() as u128 * output_sample_rate as u128)
        / input_sample_rate as u128)
        .max(1) as usize;
    let scale = input_sample_rate as f64 / output_sample_rate as f64;
    let mut output = Vec::with_capacity(output_len);
    for index in 0..output_len {
        let position = index as f64 * scale;
        let left = position.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let weight = (position - left as f64) as f32;
        output.push(samples[left] * (1.0 - weight) + samples[right] * weight);
    }
    output
}

fn samples_for_duration_ms(sample_rate: u32, duration_ms: u32) -> usize {
    ((sample_rate as u128 * duration_ms as u128) / 1_000) as usize
}
