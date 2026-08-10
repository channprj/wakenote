use std::fs;
use std::path::Path;
use std::process::{Command, Output};

pub fn convert_to_pcm_wav(
    source_path: &Path,
    destination_path: &Path,
    sample_rate: u32,
    channels: Option<u16>,
) -> Result<(), String> {
    let _ = fs::remove_file(destination_path);
    let mut native = Command::new("/usr/bin/afconvert");
    native
        .args(["-f", "WAVE", "-d"])
        .arg(format!("LEI16@{sample_rate}"));
    if let Some(channels) = channels {
        native.arg("-c").arg(channels.to_string());
    }
    let native_output = native.arg(source_path).arg(destination_path).output();
    if output_succeeded(&native_output, destination_path) {
        return Ok(());
    }

    let native_error = command_failure_message("afconvert", &native_output);
    let _ = fs::remove_file(destination_path);

    let mut fallback = ffmpeg_command();
    fallback
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(source_path)
        .args(["-c:a", "pcm_s16le", "-ar"])
        .arg(sample_rate.to_string());
    if let Some(channels) = channels {
        fallback.arg("-ac").arg(channels.to_string());
    }
    let fallback_output = fallback.args(["-f", "wav"]).arg(destination_path).output();
    if output_succeeded(&fallback_output, destination_path) {
        return Ok(());
    }

    let fallback_error = command_failure_message("ffmpeg", &fallback_output);
    let _ = fs::remove_file(destination_path);
    Err(format!(
        "native decoder failed ({native_error}); fallback decoder failed ({fallback_error})"
    ))
}

pub fn encode_wav_to_m4a(
    source_path: &Path,
    destination_path: &Path,
    bitrate_kbps: u32,
) -> Result<(), String> {
    let _ = fs::remove_file(destination_path);
    let bitrate_bps = bitrate_kbps.saturating_mul(1_000).to_string();
    let native_output = Command::new("/usr/bin/afconvert")
        .args(["-f", "m4af", "-d", "aac@44100", "-b"])
        .arg(&bitrate_bps)
        .arg(source_path)
        .arg(destination_path)
        .output();
    if output_succeeded(&native_output, destination_path) {
        return Ok(());
    }

    let native_error = command_failure_message("afconvert", &native_output);
    let _ = fs::remove_file(destination_path);

    // Some macOS releases expose afconvert while its AudioToolbox AAC
    // component is unavailable. WakeNote already supports FFmpeg for MP3, so
    // use its software AAC encoder before abandoning a completed recording.
    let bitrate_arg = format!("{bitrate_kbps}k");
    let fallback_output = ffmpeg_command()
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(source_path)
        .args(["-c:a", "aac", "-b:a"])
        .arg(&bitrate_arg)
        .args(["-ar", "44100", "-ac", "1", "-f", "ipod"])
        .arg(destination_path)
        .output();
    if output_succeeded(&fallback_output, destination_path) {
        return Ok(());
    }

    let fallback_error = command_failure_message("ffmpeg", &fallback_output);
    let _ = fs::remove_file(destination_path);
    Err(format!(
        "native encoder failed ({native_error}); fallback encoder failed ({fallback_error})"
    ))
}

pub(crate) fn ffmpeg_command() -> Command {
    for candidate in ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg"] {
        if Path::new(candidate).exists() {
            return Command::new(candidate);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = std::path::PathBuf::from(home).join(".local/bin/ffmpeg");
        if candidate.exists() {
            return Command::new(candidate);
        }
    }
    Command::new("ffmpeg")
}

fn output_succeeded(output: &std::io::Result<Output>, destination_path: &Path) -> bool {
    output.as_ref().is_ok_and(|output| output.status.success())
        && destination_path
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}

fn command_failure_message(command_name: &str, output: &std::io::Result<Output>) -> String {
    match output {
        Ok(output) => {
            let message = String::from_utf8_lossy(&output.stderr);
            message
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().chars().take(500).collect())
                .unwrap_or_else(|| format!("{command_name} exited with status {}", output.status))
        }
        Err(error) => format!("{command_name} could not start: {error}"),
    }
}
