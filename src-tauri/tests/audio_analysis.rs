use wakenote::audio_analysis::{
    AudioRange, audio_waveform_from_samples, next_audible_ranges_from_peaks,
};

#[test]
fn waveform_from_samples_returns_fixed_count_normalized_peaks_and_duration() {
    let waveform = audio_waveform_from_samples(&[0.0, 0.5, -1.0, 0.25], 4, 2, -50.0);

    assert_eq!(waveform.duration_seconds, 1.0);
    assert_eq!(waveform.peaks, vec![0.5, 1.0]);
}

#[test]
fn waveform_from_samples_emits_signed_min_and_max_peaks() {
    // Two buckets of two samples each: [0.0, 0.5] and [-1.0, 0.25].
    let waveform = audio_waveform_from_samples(&[0.0, 0.5, -1.0, 0.25], 4, 2, -50.0);

    assert_eq!(waveform.peaks_max, vec![0.5, 0.25]);
    assert_eq!(waveform.peaks_min, vec![0.0, -1.0]);
    // Absolute-peak fallback matches the larger of |max| or |min|.
    assert_eq!(waveform.peaks, vec![0.5, 1.0]);
}

#[test]
fn waveform_signed_peaks_clamp_to_unit_range() {
    // Bucket 0 = [1.5, -2.0] (both clamped); bucket 1 = [NaN, 0.4] (NaN ignored).
    let waveform = audio_waveform_from_samples(&[1.5, -2.0, f32::NAN, 0.4], 4, 2, -50.0);

    assert_eq!(waveform.peaks_max, vec![1.0, 0.4]);
    assert_eq!(waveform.peaks_min, vec![-1.0, 0.0]);
}

#[test]
fn waveform_analysis_marks_audible_ranges_for_skip_silence() {
    let waveform = audio_waveform_from_samples(
        &[0.0, 0.0, 0.7, 0.8, 0.6, 0.0, 0.0, 0.0, 0.4, 0.3],
        1,
        10,
        -35.0,
    );

    assert_eq!(
        waveform.audible_ranges,
        vec![
            AudioRange {
                start: 2.0,
                end: 5.0
            },
            AudioRange {
                start: 8.0,
                end: 10.0
            },
        ],
    );
}

#[test]
fn audible_ranges_merge_short_pauses_between_speech_regions() {
    let ranges = next_audible_ranges_from_peaks(&[0.7, 0.0, 0.6, 0.0, 0.0, 0.8], 6.0, -35.0);

    assert_eq!(
        ranges,
        vec![
            AudioRange {
                start: 0.0,
                end: 3.0
            },
            AudioRange {
                start: 5.0,
                end: 6.0
            },
        ],
    );
}
