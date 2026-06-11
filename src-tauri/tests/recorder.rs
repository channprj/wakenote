use chrono::{TimeZone, Utc};
use wakenote::recorder::{
    ChunkMetadata, ChunkSource, Recorder, RecordingRequest, TranscriptionSidecar,
    TranscriptionStatus,
};
use wakenote::settings::{AppSettings, AudioFormat};

fn wav_settings() -> AppSettings {
    AppSettings {
        audio_format: AudioFormat::Wav,
        ..AppSettings::default()
    }
}

#[test]
fn recorder_writes_m4a_with_native_encoder_bridge() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = AppSettings {
        audio_format: AudioFormat::M4a,
        ..AppSettings::default()
    };
    let started_at = Utc.with_ymd_and_hms(2026, 5, 6, 23, 8, 12).unwrap();
    let ended_at = Utc.with_ymd_and_hms(2026, 5, 6, 23, 8, 14).unwrap();
    let local = started_at.with_timezone(&chrono::Local);
    let dir = local.format("%Y%m%d").to_string();
    let stem = local.format("%H%M%S").to_string();

    let chunk = Recorder::write_chunk(RecordingRequest {
        save_root: tmp.path(),
        settings: &settings,
        samples: &[0.0; 16_000],
        sample_rate: 16_000,
        started_at,
        ended_at,
        device_id: "builtin-input",
        device_name: "Built-in Microphone",
        used_fallback_device: false,
        transcription_enabled: false,
        app_version: "0.1.0",
        live_capture_chunk_id: None,
        source: ChunkSource::Microphone,
        source_label: None,
    })
    .expect("record m4a chunk");

    assert!(chunk.audio_path.ends_with(format!("{dir}/{stem}.m4a")));
    assert!(chunk.audio_path.exists());
    assert!(chunk.metadata_path.exists());
    assert!(!chunk.audio_path.with_extension("wav").exists());

    let metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(&chunk.metadata_path).expect("metadata bytes"))
            .expect("metadata json");
    assert_eq!(metadata.sample_rate, 16_000);
    assert_eq!(
        metadata.transcription_status,
        TranscriptionStatus::NotRequested
    );
}

#[test]
fn recorder_writes_mp3_with_ffmpeg_encoder() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = AppSettings {
        audio_format: AudioFormat::Mp3,
        audio_bitrate_kbps: 128,
        ..AppSettings::default()
    };
    let started_at = Utc.with_ymd_and_hms(2026, 5, 6, 23, 9, 12).unwrap();
    let ended_at = Utc.with_ymd_and_hms(2026, 5, 6, 23, 9, 14).unwrap();
    let local = started_at.with_timezone(&chrono::Local);
    let dir = local.format("%Y%m%d").to_string();
    let stem = local.format("%H%M%S").to_string();

    let chunk = Recorder::write_chunk(RecordingRequest {
        save_root: tmp.path(),
        settings: &settings,
        samples: &[0.0, 0.2, -0.2, 0.0].repeat(4_000),
        sample_rate: 16_000,
        started_at,
        ended_at,
        device_id: "builtin-input",
        device_name: "Built-in Microphone",
        used_fallback_device: false,
        transcription_enabled: false,
        app_version: "0.1.0",
        live_capture_chunk_id: None,
        source: ChunkSource::Microphone,
        source_label: None,
    })
    .expect("record mp3 chunk");

    assert!(chunk.audio_path.ends_with(format!("{dir}/{stem}.mp3")));
    assert!(chunk.audio_path.exists());
    assert!(chunk.metadata_path.exists());
    assert!(!chunk.audio_path.with_extension("wav").exists());
    assert!(
        mp3_bit_rate(&chunk.audio_path) >= 120_000,
        "mp3 capture should honor the configured bitrate"
    );
}

#[test]
fn recorder_writes_wav_and_metadata_without_txt_when_transcription_is_off() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = wav_settings();
    let started_at = Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap();
    let ended_at = Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 11).unwrap();
    let local = started_at.with_timezone(&chrono::Local);
    let dir = local.format("%Y%m%d").to_string();
    let stem = local.format("%H%M%S").to_string();

    let chunk = Recorder::write_chunk(RecordingRequest {
        save_root: tmp.path(),
        settings: &settings,
        samples: &[0.0, 0.25, -0.25, 0.0],
        sample_rate: 16_000,
        started_at,
        ended_at,
        device_id: "builtin-input",
        device_name: "Built-in Microphone",
        used_fallback_device: false,
        transcription_enabled: false,
        app_version: "0.1.0",
        live_capture_chunk_id: None,
        source: ChunkSource::Microphone,
        source_label: None,
    })
    .expect("record chunk");

    assert!(chunk.audio_path.ends_with(format!("{dir}/{stem}.wav")));
    assert!(chunk.audio_path.exists());
    assert!(chunk.metadata_path.exists());
    assert!(!chunk.transcript_path.exists());
    assert!(!chunk.error_path.exists());

    let metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(&chunk.metadata_path).expect("metadata bytes"))
            .expect("metadata json");
    assert_eq!(metadata.model_id, "whisper-medium");
    assert_eq!(metadata.device_id, "builtin-input");
    assert_eq!(metadata.device_name, "Built-in Microphone");
    assert_eq!(metadata.sample_rate, 16_000);
    assert_eq!(metadata.threshold_dbfs, -42.0);
    assert_eq!(metadata.duration_ms, 2_000);
    assert_eq!(
        metadata.transcription_status,
        TranscriptionStatus::NotRequested
    );
    assert_eq!(metadata.app_version, "0.1.0");

    let reader = hound::WavReader::open(&chunk.audio_path).expect("wav reader");
    assert_eq!(reader.spec().sample_rate, 16_000);
}

#[test]
fn transcription_sidecar_writes_txt_and_updates_metadata_on_success() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = wav_settings();
    let timestamp = Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap();
    let chunk = Recorder::write_chunk(RecordingRequest {
        save_root: tmp.path(),
        settings: &settings,
        samples: &[0.0, 0.1, -0.1, 0.0],
        sample_rate: 16_000,
        started_at: timestamp,
        ended_at: timestamp + chrono::Duration::milliseconds(500),
        device_id: "default",
        device_name: "System Default",
        used_fallback_device: false,
        transcription_enabled: true,
        app_version: "0.1.0",
        live_capture_chunk_id: None,
        source: ChunkSource::Microphone,
        source_label: None,
    })
    .expect("record chunk");

    TranscriptionSidecar::write_success(&chunk, "안녕하세요 hello").expect("write transcript");

    assert_eq!(
        std::fs::read_to_string(&chunk.transcript_path).expect("transcript"),
        "안녕하세요 hello\n"
    );
    assert!(!chunk.error_path.exists());
    let metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(&chunk.metadata_path).expect("metadata bytes"))
            .expect("metadata json");
    assert_eq!(
        metadata.transcription_status,
        TranscriptionStatus::Completed
    );
}

#[test]
fn transcription_sidecar_writes_error_without_removing_audio() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = wav_settings();
    let timestamp = Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap();
    let chunk = Recorder::write_chunk(RecordingRequest {
        save_root: tmp.path(),
        settings: &settings,
        samples: &[0.0, 0.1, -0.1, 0.0],
        sample_rate: 16_000,
        started_at: timestamp,
        ended_at: timestamp + chrono::Duration::milliseconds(500),
        device_id: "default",
        device_name: "System Default",
        used_fallback_device: true,
        transcription_enabled: true,
        app_version: "0.1.0",
        live_capture_chunk_id: None,
        source: ChunkSource::Microphone,
        source_label: None,
    })
    .expect("record chunk");

    TranscriptionSidecar::write_error(&chunk, "model checksum mismatch").expect("write error");

    assert!(chunk.audio_path.exists());
    assert!(!chunk.transcript_path.exists());
    assert_eq!(
        std::fs::read_to_string(&chunk.error_path).expect("error"),
        "model checksum mismatch\n"
    );
    let metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(&chunk.metadata_path).expect("metadata bytes"))
            .expect("metadata json");
    assert_eq!(metadata.transcription_status, TranscriptionStatus::Failed);
    assert!(metadata.used_fallback_device);
}

fn mp3_bit_rate(path: &std::path::Path) -> u64 {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=bit_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .expect("ffprobe");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("mp3 bit_rate")
}
