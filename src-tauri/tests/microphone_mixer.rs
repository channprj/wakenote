use chrono::{TimeZone, Utc};
use wakenote::live_capture::AudioFrame;
use wakenote::multi_capture::MicrophoneMixer;
use wakenote::settings::MicrophoneSlot;

fn frame(samples: Vec<f32>, duration_ms: u64, captured_at_ms: i64) -> AudioFrame {
    AudioFrame {
        samples,
        duration_ms,
        captured_at: Utc
            .timestamp_millis_opt(captured_at_ms)
            .single()
            .expect("timestamp"),
    }
}

#[test]
fn aligned_microphone_frames_emit_one_energy_weighted_input() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::new(10, base_time);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];

    let primary = mixer.push_frame(
        MicrophoneSlot::Primary,
        10,
        frame(vec![0.8], 100, 100),
        &active,
    );
    assert!(primary.is_empty(), "wait for the matching secondary frame");

    let mixed = mixer.push_frame(
        MicrophoneSlot::Secondary,
        10,
        frame(vec![0.2], 100, 100),
        &active,
    );

    assert_eq!(mixed.len(), 1);
    assert_eq!(mixed[0].duration_ms, 100);
    assert_eq!(mixed[0].captured_at.timestamp_millis(), 100);
    assert_eq!(mixed[0].samples.len(), 1);
    assert!((mixed[0].samples[0] - 0.68).abs() < 0.0001);
}

#[test]
fn mixer_resamples_each_device_to_the_shared_output_rate() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::new(4, base_time);

    let mixed = mixer.push_frame(
        MicrophoneSlot::Primary,
        2,
        frame(vec![0.0, 1.0], 1_000, 1_000),
        &[MicrophoneSlot::Primary],
    );

    assert_eq!(mixed.len(), 1);
    assert_eq!(mixed[0].samples.len(), 4);
    assert!((mixed[0].samples[0] - 0.0).abs() < 0.0001);
    assert!((mixed[0].samples[1] - (1.0 / 3.0)).abs() < 0.0001);
    assert!((mixed[0].samples[2] - (2.0 / 3.0)).abs() < 0.0001);
    assert!((mixed[0].samples[3] - 1.0).abs() < 0.0001);
}

#[test]
fn mixer_continues_with_one_input_when_the_other_slot_stops() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::new(10, base_time);

    assert!(
        mixer
            .push_frame(
                MicrophoneSlot::Primary,
                10,
                frame(vec![0.6], 100, 100),
                &[MicrophoneSlot::Primary, MicrophoneSlot::Secondary],
            )
            .is_empty()
    );

    let mixed = mixer.drain(&[MicrophoneSlot::Primary]);

    assert_eq!(mixed.len(), 1);
    assert_eq!(mixed[0].samples, vec![0.6]);
}

#[test]
fn mixer_bounds_latency_when_an_already_seen_input_stalls() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::new(10, base_time);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];

    mixer.push_frame(
        MicrophoneSlot::Primary,
        10,
        frame(vec![0.4], 100, 100),
        &active,
    );
    assert_eq!(
        mixer
            .push_frame(
                MicrophoneSlot::Secondary,
                10,
                frame(vec![0.4], 100, 100),
                &active,
            )
            .len(),
        1
    );
    assert!(
        mixer
            .push_frame(
                MicrophoneSlot::Primary,
                10,
                frame(vec![0.6], 100, 200),
                &active,
            )
            .is_empty()
    );

    let fallback = mixer.push_frame(
        MicrophoneSlot::Primary,
        10,
        frame(vec![0.7], 100, 300),
        &active,
    );

    assert_eq!(fallback.len(), 1);
    assert_eq!(fallback[0].samples, vec![0.6]);
}

#[test]
fn mixer_discards_frames_older_than_the_emitted_timeline() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::new(10, base_time);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];

    mixer.push_frame(
        MicrophoneSlot::Primary,
        10,
        frame(vec![0.7], 100, 100),
        &active,
    );
    let emitted = mixer.push_frame(
        MicrophoneSlot::Primary,
        10,
        frame(vec![0.9], 100, 300),
        &active,
    );
    assert_eq!(emitted.iter().flat_map(|item| &item.samples).count(), 1);

    let late = mixer.push_frame(
        MicrophoneSlot::Secondary,
        10,
        frame(vec![-0.7], 100, 100),
        &active,
    );
    assert!(late.is_empty());

    let tail = mixer.flush();
    let tail_samples = tail
        .iter()
        .flat_map(|item| item.samples.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(tail_samples, vec![0.9]);
}

#[test]
fn mixer_suppresses_a_delayed_duplicate_instead_of_combining_it() {
    const SAMPLE_RATE: u32 = 1_000;
    const CHUNK_SAMPLES: usize = 10;
    const TOTAL_SAMPLES: usize = 600;
    const DUPLICATE_DELAY_SAMPLES: usize = 30;
    const ANALYSIS_WARMUP_SAMPLES: usize = 250;

    let source = (0..TOTAL_SAMPLES)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE as f32;
            0.32 * (std::f32::consts::TAU * 37.0 * time).sin()
                + 0.21 * (std::f32::consts::TAU * 73.0 * time).sin()
                + 0.13 * (std::f32::consts::TAU * 131.0 * time).sin()
        })
        .collect::<Vec<_>>();
    let delayed_duplicate = (0..TOTAL_SAMPLES)
        .map(|index| {
            index
                .checked_sub(DUPLICATE_DELAY_SAMPLES)
                .map(|source_index| source[source_index] * 0.8)
                .unwrap_or(0.0)
        })
        .collect::<Vec<_>>();

    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::new(SAMPLE_RATE, base_time);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];
    let mut mixed_samples = Vec::new();

    for start in (0..TOTAL_SAMPLES).step_by(CHUNK_SAMPLES) {
        let end = start + CHUNK_SAMPLES;
        let captured_at_ms = end as i64;
        assert!(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    SAMPLE_RATE,
                    frame(
                        source[start..end].to_vec(),
                        CHUNK_SAMPLES as u64,
                        captured_at_ms,
                    ),
                    &active,
                )
                .is_empty(),
            "the mixer must wait for the matching secondary frame",
        );
        mixed_samples.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Secondary,
                    SAMPLE_RATE,
                    frame(
                        delayed_duplicate[start..end].to_vec(),
                        CHUNK_SAMPLES as u64,
                        captured_at_ms,
                    ),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
    }

    assert_eq!(mixed_samples.len(), TOTAL_SAMPLES);
    let mean_squared_error = mixed_samples[ANALYSIS_WARMUP_SAMPLES..]
        .iter()
        .zip(&source[ANALYSIS_WARMUP_SAMPLES..])
        .map(|(actual, expected)| f64::from(actual - expected).powi(2))
        .sum::<f64>()
        / (TOTAL_SAMPLES - ANALYSIS_WARMUP_SAMPLES) as f64;
    assert!(
        mean_squared_error < 0.002,
        "a delayed copy remained audible in the merged signal (MSE {mean_squared_error:.6})",
    );
}
