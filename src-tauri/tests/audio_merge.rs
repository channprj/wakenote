use std::path::{Path, PathBuf};

use wakenote::audio_merge::{
    AudioMergeStage, MergeAudioInput, MergeAudioRequest, merge_audio_to_m4a,
    validate_and_order_request,
};

fn write_source(path: &Path) {
    std::fs::write(path, b"audio").expect("source audio");
}

fn input(path: impl Into<PathBuf>, recorded_at: &str) -> MergeAudioInput {
    MergeAudioInput {
        audio_path: path.into(),
        recorded_at: recorded_at.to_string(),
    }
}

fn request(
    destination_path: impl Into<PathBuf>,
    inputs: Vec<MergeAudioInput>,
) -> MergeAudioRequest {
    MergeAudioRequest {
        operation_id: "merge-test_01".to_string(),
        destination_path: destination_path.into(),
        inputs,
    }
}

#[test]
fn validation_orders_inputs_by_timestamp_then_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let earlier_b = tmp.path().join("earlier-b.wav");
    let earlier_a = tmp.path().join("earlier-a.m4a");
    let later = tmp.path().join("later.mp3");
    for path in [&earlier_b, &earlier_a, &later] {
        write_source(path);
    }

    let ordered = validate_and_order_request(request(
        tmp.path().join("merged.m4a"),
        vec![
            input(&later, "2026-08-04T05:00:02Z"),
            input(&earlier_b, "2026-08-04T05:00:01Z"),
            input(&earlier_a, "2026-08-04T05:00:01Z"),
        ],
    ))
    .expect("valid request");

    assert_eq!(ordered[0].audio_path, earlier_a);
    assert_eq!(ordered[1].audio_path, earlier_b);
    assert_eq!(ordered[2].audio_path, later);
}

#[test]
fn validation_rejects_invalid_operation_and_input_counts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let source = tmp.path().join("source.wav");
    write_source(&source);

    let mut invalid_id = request(
        tmp.path().join("merged.m4a"),
        vec![
            input(&source, "2026-08-04T05:00:01Z"),
            input(&source, "2026-08-04T05:00:02Z"),
        ],
    );
    invalid_id.operation_id = "not valid!".to_string();
    assert!(
        validate_and_order_request(invalid_id)
            .expect_err("invalid operation id")
            .contains("operation id")
    );

    let too_few = request(
        tmp.path().join("merged.m4a"),
        vec![input(&source, "2026-08-04T05:00:01Z")],
    );
    assert!(
        validate_and_order_request(too_few)
            .expect_err("too few inputs")
            .contains("at least 2")
    );
}

#[test]
fn validation_rejects_duplicate_unsupported_and_malformed_inputs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let first = tmp.path().join("first.wav");
    let unsupported = tmp.path().join("notes.txt");
    write_source(&first);
    write_source(&unsupported);

    let duplicate = request(
        tmp.path().join("duplicate.m4a"),
        vec![
            input(&first, "2026-08-04T05:00:01Z"),
            input(&first, "2026-08-04T05:00:02Z"),
        ],
    );
    assert!(
        validate_and_order_request(duplicate)
            .expect_err("duplicate input")
            .contains("duplicate")
    );

    let unsupported_request = request(
        tmp.path().join("unsupported.m4a"),
        vec![
            input(&first, "2026-08-04T05:00:01Z"),
            input(&unsupported, "2026-08-04T05:00:02Z"),
        ],
    );
    assert!(
        validate_and_order_request(unsupported_request)
            .expect_err("unsupported input")
            .contains("unsupported")
    );

    let malformed = request(
        tmp.path().join("malformed.m4a"),
        vec![
            input(&first, "not-a-timestamp"),
            input(tmp.path().join("second.wav"), "2026-08-04T05:00:02Z"),
        ],
    );
    write_source(&tmp.path().join("second.wav"));
    assert!(
        validate_and_order_request(malformed)
            .expect_err("malformed timestamp")
            .contains("RFC 3339")
    );
}

#[test]
fn validation_rejects_source_destination_collision_and_bad_destination() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let first = tmp.path().join("first.m4a");
    let second = tmp.path().join("second.wav");
    write_source(&first);
    write_source(&second);

    let collision = request(
        &first,
        vec![
            input(&first, "2026-08-04T05:00:01Z"),
            input(&second, "2026-08-04T05:00:02Z"),
        ],
    );
    assert!(
        validate_and_order_request(collision)
            .expect_err("source destination collision")
            .contains("destination")
    );

    let bad_extension = request(
        tmp.path().join("merged.wav"),
        vec![
            input(&first, "2026-08-04T05:00:01Z"),
            input(&second, "2026-08-04T05:00:02Z"),
        ],
    );
    assert!(
        validate_and_order_request(bad_extension)
            .expect_err("destination extension")
            .contains(".m4a")
    );

    let missing_parent = request(
        tmp.path().join("missing").join("merged.m4a"),
        vec![
            input(&first, "2026-08-04T05:00:01Z"),
            input(&second, "2026-08-04T05:00:02Z"),
        ],
    );
    assert!(
        validate_and_order_request(missing_parent)
            .expect_err("destination parent")
            .contains("parent")
    );
}

fn write_constant_wav(path: &Path, sample: i16) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("wav writer");
    for _ in 0..8_820 {
        writer.write_sample(sample).expect("wav sample");
    }
    writer.finalize().expect("finalize wav");
}

fn convert_to_m4a(source: &Path, destination: &Path) {
    let output = std::process::Command::new("/usr/bin/afconvert")
        .args(["-f", "m4af", "-d", "aac@44100", "-c", "1", "-b", "96000"])
        .arg(source)
        .arg(destination)
        .output()
        .expect("run afconvert");
    assert!(
        output.status.success(),
        "afconvert failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn decode_m4a_samples(source: &Path, destination: &Path) -> Vec<i16> {
    let output = std::process::Command::new("/usr/bin/afconvert")
        .args(["-f", "WAVE", "-d", "LEI16@44100", "-c", "1"])
        .arg(source)
        .arg(destination)
        .output()
        .expect("decode merged m4a");
    assert!(
        output.status.success(),
        "afconvert decode failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    hound::WavReader::open(destination)
        .expect("decoded wav")
        .samples::<i16>()
        .map(|sample| sample.expect("decoded sample"))
        .collect()
}

#[test]
fn merge_preserves_chronological_audio_and_emits_monotonic_progress() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let earlier = tmp.path().join("earlier.wav");
    let later_wav = tmp.path().join("later-source.wav");
    let later = tmp.path().join("later.m4a");
    let destination = tmp.path().join("merged.m4a");
    write_constant_wav(&earlier, 12_000);
    write_constant_wav(&later_wav, -12_000);
    convert_to_m4a(&later_wav, &later);
    let mut progress = Vec::new();

    let result = merge_audio_to_m4a(
        request(
            &destination,
            vec![
                input(&later, "2026-08-04T05:00:02Z"),
                input(&earlier, "2026-08-04T05:00:01Z"),
            ],
        ),
        |event| progress.push(event),
    )
    .expect("merge audio");

    assert_eq!(result.destination_path, destination);
    assert_eq!(result.input_count, 2);
    assert!(destination.is_file());
    assert!(
        progress
            .windows(2)
            .all(|pair| pair[0].percent <= pair[1].percent)
    );
    assert_eq!(
        progress.last().map(|item| item.stage),
        Some(AudioMergeStage::Saved)
    );
    assert_eq!(progress.last().map(|item| item.percent), Some(100));

    let samples = decode_m4a_samples(&destination, &tmp.path().join("decoded.wav"));
    assert!(
        samples.len() >= 12_000,
        "decoded samples: {}",
        samples.len()
    );
    let first_mean = samples[samples.len() / 8..samples.len() * 3 / 8]
        .iter()
        .map(|sample| i64::from(*sample))
        .sum::<i64>()
        / (samples.len() as i64 / 4);
    let second_mean = samples[samples.len() * 5 / 8..samples.len() * 7 / 8]
        .iter()
        .map(|sample| i64::from(*sample))
        .sum::<i64>()
        / (samples.len() as i64 / 4);
    assert!(first_mean > 2_000, "first mean: {first_mean}");
    assert!(second_mean < -2_000, "second mean: {second_mean}");
}

#[test]
fn merge_preserves_existing_destination_and_cleans_temporary_artifacts_on_failure() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let first = tmp.path().join("first.wav");
    let broken = tmp.path().join("broken.wav");
    let destination = tmp.path().join("merged.m4a");
    write_constant_wav(&first, 8_000);
    std::fs::write(&broken, b"not a valid wav").expect("broken wav");
    std::fs::write(&destination, b"original").expect("existing destination");

    let error = merge_audio_to_m4a(
        request(
            &destination,
            vec![
                input(&first, "2026-08-04T05:00:01Z"),
                input(&broken, "2026-08-04T05:00:02Z"),
            ],
        ),
        |_| {},
    )
    .expect_err("merge should fail");

    assert!(error.contains("broken.wav"), "error: {error}");
    assert_eq!(
        std::fs::read(&destination).expect("destination"),
        b"original"
    );
    let leftovers = std::fs::read_dir(tmp.path())
        .expect("destination directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".wakenote-merge-")
        })
        .count();
    assert_eq!(leftovers, 0);
}
