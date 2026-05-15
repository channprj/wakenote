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
