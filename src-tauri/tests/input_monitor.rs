use wakenote::input_monitor::{MonitorBuffer, resample_for_output};

#[test]
fn monitor_buffer_drops_oldest_samples_when_full() {
    let buffer = MonitorBuffer::new(4);

    buffer.push(&[1.0, 2.0, 3.0]);
    buffer.push(&[4.0, 5.0, 6.0]);

    assert_eq!(buffer.dropped_sample_count(), 2);

    let mut output = [9.0; 6];
    let read = buffer.read(&mut output);

    assert_eq!(read, 4);
    assert_eq!(output, [3.0, 4.0, 5.0, 6.0, 0.0, 0.0]);
}

#[test]
fn monitor_buffer_fills_underflow_with_silence() {
    let buffer = MonitorBuffer::new(8);
    buffer.push(&[0.25, -0.25]);

    let mut output = [1.0; 4];
    let read = buffer.read(&mut output);

    assert_eq!(read, 2);
    assert_eq!(output, [0.25, -0.25, 0.0, 0.0]);
}

#[test]
fn monitor_resampler_preserves_samples_when_rates_match() {
    let samples = [0.0, 0.5, -0.5, 1.0];

    let resampled = resample_for_output(&samples, 48_000, 48_000);

    assert_eq!(resampled, samples);
}
