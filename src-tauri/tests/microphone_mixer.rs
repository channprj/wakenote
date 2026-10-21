use chrono::{TimeZone, Utc};
use wakenote::live_capture::AudioFrame;
use wakenote::multi_capture::{MicrophoneMixMode, MicrophoneMixer};
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

fn broadband_reference(total_samples: usize, sample_rate: u32) -> Vec<f32> {
    let mut seed = 0x1234_5678_u32;
    let mut smoothed_noise = 0.0_f32;
    (0..total_samples)
        .map(|index| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed as f32 / u32::MAX as f32 - 0.5) * 0.16;
            smoothed_noise = smoothed_noise * 0.58 + noise * 0.42;
            let time = index as f32 / sample_rate as f32;
            let envelope = 0.55 + 0.45 * (std::f32::consts::TAU * 2.7 * time).sin().abs();
            envelope
                * (0.26 * (std::f32::consts::TAU * 113.0 * time).sin()
                    + 0.17 * (std::f32::consts::TAU * 241.0 * time).sin()
                    + smoothed_noise)
        })
        .collect()
}

fn fractional_delayed(source: &[f32], delay_at: impl Fn(usize) -> f64) -> Vec<f32> {
    (0..source.len())
        .map(|index| {
            let position = index as f64 - delay_at(index);
            if position < 0.0 {
                return 0.0;
            }
            let left = position.floor() as usize;
            let right = (left + 1).min(source.len() - 1);
            let fraction = (position - left as f64) as f32;
            source[left] + (source[right] - source[left]) * fraction
        })
        .collect()
}

fn feed_pair(
    mixer: &mut MicrophoneMixer,
    sample_rate: u32,
    primary: &[f32],
    secondary: &[f32],
    chunk_samples: usize,
) -> Vec<f32> {
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];
    let mut output = Vec::new();
    for start in (0..primary.len()).step_by(chunk_samples) {
        let end = (start + chunk_samples).min(primary.len());
        let captured_at_ms = ((end as u64 * 1_000) / u64::from(sample_rate)) as i64;
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    sample_rate,
                    frame(
                        primary[start..end].to_vec(),
                        ((end - start) as u64 * 1_000) / u64::from(sample_rate),
                        captured_at_ms,
                    ),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Secondary,
                    sample_rate,
                    frame(
                        secondary[start..end].to_vec(),
                        ((end - start) as u64 * 1_000) / u64::from(sample_rate),
                        captured_at_ms,
                    ),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
    }
    output
}

#[test]
fn aligned_microphone_frames_emit_the_cleaner_input_during_alignment_warmup() {
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

    assert!(
        mixer
            .push_frame(
                MicrophoneSlot::Secondary,
                10,
                frame(vec![0.2], 100, 100),
                &active,
            )
            .is_empty()
    );
    let mixed = mixer.flush();

    assert_eq!(mixed.len(), 1);
    assert_eq!(mixed[0].duration_ms, 100);
    assert_eq!(mixed[0].captured_at.timestamp_millis(), 100);
    assert_eq!(mixed[0].samples.len(), 1);
    assert!((mixed[0].samples[0] - 0.8).abs() < 0.0001);
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
    assert!((mixed[0].samples[1] - 0.0).abs() < 0.0001);
    assert!((mixed[0].samples[2] - 0.5).abs() < 0.0001);
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
    assert!(
        mixer
            .push_frame(
                MicrophoneSlot::Secondary,
                10,
                frame(vec![0.4], 100, 100),
                &active,
            )
            .is_empty()
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

    for (value, captured_at) in [(0.7, 300), (0.8, 400)] {
        assert!(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    10,
                    frame(vec![value], 100, captured_at),
                    &active,
                )
                .is_empty()
        );
    }
    let fallback = mixer.push_frame(
        MicrophoneSlot::Primary,
        10,
        frame(vec![0.9], 100, 500),
        &active,
    );

    assert_eq!(fallback.len(), 1);
    assert_eq!(fallback[0].samples, vec![0.4]);
}

#[test]
fn mixer_discards_frames_older_than_the_emitted_timeline() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(10, base_time, MicrophoneMixMode::Priority);
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
    mixed_samples.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));

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

#[test]
fn adaptive_merge_holds_a_220ms_alignment_buffer() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(1_000, base_time, MicrophoneMixMode::Merge);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];

    for end in [100, 200] {
        assert!(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    1_000,
                    frame(vec![0.2; 100], 100, end),
                    &active,
                )
                .is_empty()
        );
        assert!(
            mixer
                .push_frame(
                    MicrophoneSlot::Secondary,
                    1_000,
                    frame(vec![0.2; 100], 100, end),
                    &active,
                )
                .is_empty()
        );
    }

    assert!(
        mixer
            .push_frame(
                MicrophoneSlot::Primary,
                1_000,
                frame(vec![0.2; 100], 100, 300),
                &active,
            )
            .is_empty()
    );
    let emitted = mixer.push_frame(
        MicrophoneSlot::Secondary,
        1_000,
        frame(vec![0.2; 100], 100, 300),
        &active,
    );
    assert_eq!(
        emitted
            .iter()
            .map(|frame| frame.samples.len())
            .sum::<usize>(),
        80
    );
}

#[test]
fn priority_audio_uses_the_stronger_clean_microphone_without_blending() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(1_000, base_time, MicrophoneMixMode::Priority);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];
    let mut output = Vec::new();

    for chunk in 1..=8 {
        let end = chunk * 100;
        let primary = vec![0.36; 100];
        let secondary = vec![0.08; 100];
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    1_000,
                    frame(primary, 100, end),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Secondary,
                    1_000,
                    frame(secondary, 100, end),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
    }
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));

    let tail = &output[output.len().saturating_sub(200)..];
    let mean = tail.iter().copied().sum::<f32>() / tail.len() as f32;
    assert!((mean - 0.36).abs() < 0.01, "priority mean {mean}");
}

#[test]
fn priority_audio_requires_three_better_windows_before_crossfading_sources() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(1_000, base_time, MicrophoneMixMode::Priority);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];
    let mut push_chunk = |chunk: i64, primary: f32, secondary: f32| {
        let end = chunk * 100;
        let mut output = mixer
            .push_frame(
                MicrophoneSlot::Primary,
                1_000,
                frame(vec![primary; 100], 100, end),
                &active,
            )
            .into_iter()
            .flat_map(|frame| frame.samples)
            .collect::<Vec<_>>();
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Secondary,
                    1_000,
                    frame(vec![secondary; 100], 100, end),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
        output
    };

    for chunk in 1..=4 {
        let _ = push_chunk(chunk, 0.36, 0.08);
    }
    for chunk in 5..=6 {
        let output = push_chunk(chunk, 0.08, 0.36);
        let mean = output.iter().copied().sum::<f32>() / output.len() as f32;
        assert!(mean < 0.1, "priority switched before hysteresis: {mean}");
    }
    let switched = push_chunk(7, 0.08, 0.36);
    let tail = &switched[switched.len() - 20..];
    let tail_mean = tail.iter().copied().sum::<f32>() / tail.len() as f32;
    assert!(
        tail_mean > 0.34,
        "priority did not finish crossfade: {tail_mean}"
    );
}

#[test]
fn priority_audio_rejects_a_louder_clipped_microphone() {
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(1_000, base_time, MicrophoneMixMode::Priority);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];
    let mut output = Vec::new();

    for chunk in 1..=8 {
        let end = chunk * 100;
        let primary = (0..100)
            .map(|index| if index % 2 == 0 { 0.32 } else { -0.32 })
            .collect();
        let clipped = (0..100)
            .map(|index| if index % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    1_000,
                    frame(primary, 100, end),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Secondary,
                    1_000,
                    frame(clipped, 100, end),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
    }
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));

    let tail_peak = output[output.len().saturating_sub(200)..]
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(tail_peak < 0.4, "clipped source won priority: {tail_peak}");
}

#[test]
fn adaptive_merge_estimates_fractional_delay_gain_and_polarity() {
    const SAMPLE_RATE: u32 = 4_000;
    const DELAY_SAMPLES: f64 = 17.4;
    let source = broadband_reference(SAMPLE_RATE as usize * 4, SAMPLE_RATE);
    let secondary = fractional_delayed(&source, |_| DELAY_SAMPLES)
        .into_iter()
        .map(|sample| sample * -0.5)
        .collect::<Vec<_>>();
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(SAMPLE_RATE, base_time, MicrophoneMixMode::Merge);

    let mut output = feed_pair(&mut mixer, SAMPLE_RATE, &source, &secondary, 80);
    let snapshot = mixer.alignment_snapshot();
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));

    assert!(snapshot.confident, "alignment snapshot: {snapshot:?}");
    assert!(
        (snapshot.lag_samples - DELAY_SAMPLES).abs() < 1.0,
        "alignment snapshot: {snapshot:?}",
    );
    assert!(
        snapshot.correlation < -0.9,
        "alignment snapshot: {snapshot:?}"
    );
    assert!(
        (snapshot.secondary_gain - 2.0).abs() < 0.25,
        "alignment snapshot: {snapshot:?}",
    );
    assert_eq!(snapshot.secondary_polarity, -1.0);

    let warmup = SAMPLE_RATE as usize;
    let comparable = output.len().min(source.len());
    let mse = output[warmup..comparable]
        .iter()
        .zip(&source[warmup..comparable])
        .map(|(actual, expected)| f64::from(actual - expected).powi(2))
        .sum::<f64>()
        / (comparable - warmup) as f64;
    assert!(mse < 0.004, "aligned output MSE {mse:.6}");
}

#[test]
fn adaptive_merge_tracks_linear_clock_drift() {
    const SAMPLE_RATE: u32 = 4_000;
    const DRIFT_PPM: f64 = 800.0;
    let source = broadband_reference(SAMPLE_RATE as usize * 8, SAMPLE_RATE);
    let secondary = fractional_delayed(&source, |index| {
        8.0 + index as f64 * DRIFT_PPM / 1_000_000.0
    });
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(SAMPLE_RATE, base_time, MicrophoneMixMode::Merge);

    let _ = feed_pair(&mut mixer, SAMPLE_RATE, &source, &secondary, 80);
    let snapshot = mixer.alignment_snapshot();

    assert!(snapshot.confident, "alignment snapshot: {snapshot:?}");
    assert!(
        (snapshot.drift_ppm - DRIFT_PPM).abs() < 350.0,
        "alignment snapshot: {snapshot:?}",
    );
}

#[test]
fn adaptive_merge_falls_back_cleanly_during_reflection_jitter_and_dropout() {
    const SAMPLE_RATE: u32 = 4_000;
    const TOTAL_SAMPLES: usize = SAMPLE_RATE as usize * 5;
    const CHUNK_SAMPLES: usize = 80;
    let source = broadband_reference(TOTAL_SAMPLES, SAMPLE_RATE);
    let delayed = fractional_delayed(&source, |_| 24.0);
    let reflected = fractional_delayed(&source, |_| 91.0);
    let secondary = delayed
        .iter()
        .zip(reflected)
        .map(|(direct, reflection)| direct * 0.58 + reflection * 0.52)
        .collect::<Vec<_>>();
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(SAMPLE_RATE, base_time, MicrophoneMixMode::Merge);
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];
    let jitter_ms = [0_i64, 3, -2, 1, -3, 2];
    let mut output = Vec::new();

    for (chunk, start) in (0..TOTAL_SAMPLES).step_by(CHUNK_SAMPLES).enumerate() {
        let end = (start + CHUNK_SAMPLES).min(TOTAL_SAMPLES);
        let captured_at_ms = ((end as u64 * 1_000) / u64::from(SAMPLE_RATE)) as i64;
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    SAMPLE_RATE,
                    frame(source[start..end].to_vec(), 20, captured_at_ms),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
        let dropout = (SAMPLE_RATE as usize * 2..SAMPLE_RATE as usize * 3).contains(&start);
        if !dropout {
            output.extend(
                mixer
                    .push_frame(
                        MicrophoneSlot::Secondary,
                        SAMPLE_RATE,
                        frame(
                            secondary[start..end].to_vec(),
                            20,
                            captured_at_ms + jitter_ms[chunk % jitter_ms.len()],
                        ),
                        &active,
                    )
                    .into_iter()
                    .flat_map(|frame| frame.samples),
            );
        }
    }
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));

    assert!(output.len() >= TOTAL_SAMPLES - CHUNK_SAMPLES);
    let silence_ratio = output
        .iter()
        .filter(|sample| sample.abs() < 0.000_001)
        .count() as f64
        / output.len() as f64;
    assert!(
        silence_ratio < 0.02,
        "dropout injected silence: {silence_ratio:.3}"
    );
    let (max_jump_index, max_jump) = output
        .windows(2)
        .enumerate()
        .map(|(index, pair)| (index, (pair[1] - pair[0]).abs()))
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .unwrap();
    let source_max_jump = source
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_jump <= source_max_jump + 0.12,
        "transition discontinuity {max_jump:.3} at {max_jump_index} ({:?}), source {source_max_jump:.3}",
        &output[max_jump_index.saturating_sub(3)..(max_jump_index + 5).min(output.len())],
    );
}

#[test]
fn priority_audio_preserves_every_sample_despite_callback_timestamp_jitter() {
    let mut mixer = MicrophoneMixer::with_mode(
        48_000,
        Utc.timestamp_millis_opt(0).unwrap(),
        MicrophoneMixMode::Priority,
    );
    let source = broadband_reference(48_000, 48_000);
    let jitter_ms = [0, 3, -2, 1, -3, 2];
    let mut output = Vec::new();
    for (chunk, samples) in source.chunks(480).enumerate() {
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    48_000,
                    frame(
                        samples.to_vec(),
                        10,
                        (chunk as i64 + 1) * 10 + jitter_ms[chunk % 6],
                    ),
                    &[MicrophoneSlot::Primary],
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
    }
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));
    assert_eq!(
        output.len(),
        source.len(),
        "callback jitter deleted audio samples"
    );
    assert_eq!(
        output, source,
        "callback jitter reordered or overwrote audio"
    );
}

#[test]
fn priority_audio_resampling_does_not_depend_on_callback_buffer_size() {
    let source = (0..48_000)
        .map(|index| 0.6 * (std::f32::consts::TAU * 1_000.0 * index as f32 / 48_000.0).sin())
        .collect::<Vec<_>>();
    let render = |chunk_samples: usize| {
        let mut mixer = MicrophoneMixer::with_mode(
            44_100,
            Utc.timestamp_millis_opt(0).unwrap(),
            MicrophoneMixMode::Priority,
        );
        let mut output = Vec::new();
        for start in (0..source.len()).step_by(chunk_samples) {
            let end = (start + chunk_samples).min(source.len());
            let mut input = frame(source[start..end].to_vec(), 1, 0);
            input.captured_at += chrono::Duration::microseconds(end as i64 * 1_000_000 / 48_000);
            output.extend(
                mixer
                    .push_frame(
                        MicrophoneSlot::Primary,
                        48_000,
                        input,
                        &[MicrophoneSlot::Primary],
                    )
                    .into_iter()
                    .flat_map(|frame| frame.samples),
            );
        }
        output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));
        output
    };
    let large = render(48_000);
    let small = render(512);
    assert_eq!(
        small.len(),
        44_100,
        "resampling rounded each buffer separately"
    );
    assert_eq!(small.len(), large.len());
    let error = small
        .iter()
        .zip(&large)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        error < 0.0001,
        "resampling changed waveform at buffer boundaries: {error}"
    );
}

#[test]
fn priority_audio_crossfade_does_not_clip_correlated_loud_inputs() {
    let mut mixer = MicrophoneMixer::with_mode(
        1_000,
        Utc.timestamp_millis_opt(0).unwrap(),
        MicrophoneMixMode::Priority,
    );
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];
    let mut output = Vec::new();
    for chunk in 1..=12 {
        let (primary, secondary) = if chunk <= 4 { (0.8, 0.2) } else { (0.4, 0.95) };
        for (slot, value) in [
            (MicrophoneSlot::Primary, primary),
            (MicrophoneSlot::Secondary, secondary),
        ] {
            output.extend(
                mixer
                    .push_frame(
                        slot,
                        1_000,
                        frame(vec![value; 100], 100, chunk * 100),
                        &active,
                    )
                    .into_iter()
                    .flat_map(|frame| frame.samples),
            );
        }
    }
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));
    let peak = output.iter().copied().fold(0.0_f32, f32::max);
    assert!(
        peak <= 0.95001,
        "priority crossfade amplified and clipped clean inputs: {peak}"
    );
    assert!(
        output.last().unwrap() > &0.94,
        "priority did not select the better input"
    );
}

#[test]
fn priority_audio_crossfades_when_a_missing_selected_input_returns() {
    let mut mixer = MicrophoneMixer::with_mode(
        1_000,
        Utc.timestamp_millis_opt(0).unwrap(),
        MicrophoneMixMode::Priority,
    );
    let active = [MicrophoneSlot::Primary, MicrophoneSlot::Secondary];
    let mut output = Vec::new();
    for chunk in 1..=12 {
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    1_000,
                    frame(vec![0.08; 100], 100, chunk * 100),
                    &active,
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
        if !(5..=8).contains(&chunk) {
            output.extend(
                mixer
                    .push_frame(
                        MicrophoneSlot::Secondary,
                        1_000,
                        frame(vec![0.36; 100], 100, chunk * 100),
                        &active,
                    )
                    .into_iter()
                    .flat_map(|frame| frame.samples),
            );
        }
    }
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));
    let return_jump = output[700..]
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        return_jump < 0.02,
        "reconnected input caused an abrupt waveform jump: {return_jump}"
    );
    assert!(output.last().unwrap() > &0.34);
}

#[test]
fn priority_audio_rejects_louder_unclipped_static_in_favor_of_clean_voice() {
    let mut mixer = MicrophoneMixer::with_mode(
        48_000,
        Utc.timestamp_millis_opt(0).unwrap(),
        MicrophoneMixMode::Priority,
    );
    let source = (0..48_000)
        .map(|index| 0.12 * (std::f32::consts::TAU * 220.0 * index as f32 / 48_000.0).sin())
        .collect::<Vec<_>>();
    let mut seed = 0x9173_1521_u32;
    let static_noise = (0..48_000)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed as f32 / u32::MAX as f32 - 0.5
        })
        .collect::<Vec<_>>();
    let mut output = feed_pair(&mut mixer, 48_000, &source, &static_noise, 480);
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));
    let error = output
        .iter()
        .zip(&source)
        .map(|(actual, clean)| (actual - clean).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        error < 0.0001,
        "louder static was selected over clean voice: {error}"
    );
}

#[test]
fn resampler_retains_phase_when_a_buffer_is_shorter_than_one_output_sample() {
    let mut mixer = MicrophoneMixer::with_mode(
        1,
        Utc.timestamp_millis_opt(0).unwrap(),
        MicrophoneMixMode::Priority,
    );
    let mut output = Vec::new();
    for (index, value) in [0.1, 0.2, 0.3, 0.4].into_iter().enumerate() {
        output.extend(
            mixer
                .push_frame(
                    MicrophoneSlot::Primary,
                    2,
                    frame(vec![value], 500, (index as i64 + 1) * 500),
                    &[MicrophoneSlot::Primary],
                )
                .into_iter()
                .flat_map(|frame| frame.samples),
        );
    }
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));
    assert_eq!(output, vec![0.2, 0.4]);
}

#[test]
fn priority_audio_switches_from_startup_static_to_quiet_clean_voice() {
    let mut mixer = MicrophoneMixer::with_mode(
        48_000,
        Utc.timestamp_millis_opt(0).unwrap(),
        MicrophoneMixMode::Priority,
    );
    let voice = (0..48_000)
        .map(|index| {
            if index < 4_800 {
                0.0
            } else {
                0.04 * (std::f32::consts::TAU * 220.0 * index as f32 / 48_000.0).sin()
            }
        })
        .collect::<Vec<_>>();
    let interference = (0..48_000)
        .map(|index| if index % 4 < 2 { 0.4 } else { -0.4 })
        .collect::<Vec<_>>();
    let mut output = feed_pair(&mut mixer, 48_000, &voice, &interference, 480);
    output.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));
    assert_eq!(output.len(), voice.len());
    let error = output[24_000..]
        .iter()
        .zip(&voice[24_000..])
        .map(|(actual, clean)| (actual - clean).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        error < 0.0001,
        "startup static held priority after quiet voice arrived: {error}"
    );
}

#[test]
fn merge_of_identical_samples_stays_comparable_to_the_single_signal() {
    const SAMPLE_RATE: u32 = 1_000;
    const CHUNK_SAMPLES: usize = 10;
    const TOTAL_SAMPLES: usize = 600;
    const ANALYSIS_WARMUP_SAMPLES: usize = 250;

    let source = (0..TOTAL_SAMPLES)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE as f32;
            0.32 * (std::f32::consts::TAU * 37.0 * time).sin()
                + 0.21 * (std::f32::consts::TAU * 73.0 * time).sin()
                + 0.13 * (std::f32::consts::TAU * 131.0 * time).sin()
        })
        .collect::<Vec<_>>();
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::new(SAMPLE_RATE, base_time);
    let mut mixed = feed_pair(&mut mixer, SAMPLE_RATE, &source, &source, CHUNK_SAMPLES);
    mixed.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));

    let source_peak = source
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    let mixed_peak = mixed
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    assert!(
        mixed_peak <= source_peak * 1.05,
        "identical inputs were amplified or hard-clipped (peak {mixed_peak} vs {source_peak})"
    );
    let compared = mixed.len().saturating_sub(ANALYSIS_WARMUP_SAMPLES).max(1);
    let mean_squared_error = mixed
        .iter()
        .rev()
        .zip(source.iter().rev())
        .take(compared)
        .map(|(actual, expected)| f64::from(actual - expected).powi(2))
        .sum::<f64>()
        / compared as f64;
    assert!(
        mean_squared_error < 0.002,
        "identical inputs cancelled or drifted (MSE {mean_squared_error:.6})"
    );
}

#[test]
fn priority_of_identical_samples_does_not_sum_the_two_slots() {
    const SAMPLE_RATE: u32 = 1_000;
    let base_time = Utc.timestamp_millis_opt(0).single().unwrap();
    let mut mixer = MicrophoneMixer::with_mode(SAMPLE_RATE, base_time, MicrophoneMixMode::Priority);
    let source = vec![0.4_f32; 800];
    let mut mixed = feed_pair(&mut mixer, SAMPLE_RATE, &source, &source, 100);
    mixed.extend(mixer.flush().into_iter().flat_map(|frame| frame.samples));

    assert!(!mixed.is_empty());
    let peak = mixed
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    assert!(
        peak < 0.55,
        "identical priority inputs were summed (peak {peak})"
    );
    let loud = mixed
        .iter()
        .filter(|sample| sample.abs() > 0.2)
        .copied()
        .collect::<Vec<_>>();
    assert!(!loud.is_empty());
    let mean = loud.iter().sum::<f32>() / loud.len() as f32;
    assert!(
        (mean - 0.4).abs() < 0.05,
        "priority output left the single-signal level (mean {mean})"
    );
}

#[test]
fn priority_audio_preserves_a_dropped_callback_gap_instead_of_shifting_later_audio() {
    let mut mixer = MicrophoneMixer::with_mode(
        48_000,
        Utc.timestamp_millis_opt(0).unwrap(),
        MicrophoneMixMode::Priority,
    );
    let ends = [10, 30, 40].map(|end| {
        let output = mixer.push_frame(
            MicrophoneSlot::Primary,
            48_000,
            frame(vec![0.2; 480], 10, end),
            &[MicrophoneSlot::Primary],
        );
        output.last().unwrap().captured_at.timestamp_millis()
    });
    assert_eq!(ends, [10, 30, 40]);
}
