use crate::{AppState, api::error::ApiError, config::Config};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Read, path::PathBuf, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

pub const MODEL: &str = "nvidia/Nemotron-3-Diarization";
pub const REVISION: &str = "f667ed73aee57d40cc39428eb768b4fd87a0a29e";
const MODEL_SHA256: &str = "08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1";
const MAX_AUDIO_BYTES: usize = 1024 * 1024;
const MAX_SECONDS: f64 = 60.0;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    audio: String,
    format: String,
}

pub fn verify_model(config: &Config) -> bool {
    if !std::path::Path::new(&config.diarization_binary).is_file() {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(&config.diarization_model_path) else {
        return false;
    };
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => hash.update(&buffer[..size]),
            Err(_) => return false,
        }
    }
    format!("{:x}", hash.finalize()) == MODEL_SHA256
}

fn decode(body: Value) -> Result<(Vec<u8>, &'static str), ApiError> {
    let input: Input = serde_json::from_value(body)
        .map_err(|_| ApiError::bad("Provide only base64 audio and format"))?;
    let demuxer = match input.format.as_str() {
        "wav" => "wav",
        "webm" => "matroska",
        "mp3" => "mp3",
        "mp4" => "mov",
        "ogg" => "ogg",
        _ => return Err(ApiError::bad("Unsupported recording format")),
    };
    if input.audio.is_empty() || input.audio.len() > MAX_AUDIO_BYTES.div_ceil(3) * 4 {
        return Err(ApiError::bad("Recording must contain at most 1 MiB"));
    }
    let audio = STANDARD
        .decode(input.audio)
        .map_err(|_| ApiError::bad("Recording must be valid base64"))?;
    if audio.is_empty() || audio.len() > MAX_AUDIO_BYTES {
        return Err(ApiError::bad("Recording must contain at most 1 MiB"));
    }
    Ok((audio, demuxer))
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, ApiError> {
        let path = std::env::temp_dir().join(format!("tomi-diarize-{}", uuid::Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|_| ApiError::transport())?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn execute(mut command: Command, seconds: u64) -> Result<Vec<u8>, ApiError> {
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("OMP_NUM_THREADS", "2")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| ApiError::unavailable("The local audio runtime is unavailable"))?;
    let mut stdout = child.stdout.take().ok_or_else(ApiError::transport)?;
    tokio::time::timeout(Duration::from_secs(seconds), async {
        let mut output = Vec::new();
        (&mut stdout)
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut output)
            .await
            .map_err(|_| ApiError::transport())?;
        if output.len() > 2 * 1024 * 1024 {
            return Err(ApiError::bad("Audio runtime output exceeded its limit"));
        }
        let status = child.wait().await.map_err(|_| ApiError::transport())?;
        if !status.success() {
            return Err(ApiError::bad("Recording could not be decoded or diarized"));
        }
        Ok(output)
    })
    .await
    .map_err(|_| ApiError::unavailable("Audio processing exceeded its time limit"))?
}

fn segments(value: Value, duration: f64) -> Result<Vec<Value>, ApiError> {
    let rows = value
        .get("segments")
        .and_then(Value::as_array)
        .ok_or_else(ApiError::transport)?;
    if rows.len() > 4096 {
        return Err(ApiError::transport());
    }
    rows.iter().map(|row| {
        let start = row.get("start").and_then(Value::as_f64).ok_or_else(ApiError::transport)?;
        let end = row.get("end").and_then(Value::as_f64).ok_or_else(ApiError::transport)?;
        let speaker = row.get("speaker").and_then(Value::as_u64).filter(|n| (1..=8).contains(n)).ok_or_else(ApiError::transport)?;
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start || start >= duration || end > duration + 0.1 {
            return Err(ApiError::transport());
        }
        Ok(json!({"speaker":format!("speaker_{speaker}"),"start":start,"end":end.min(duration)}))
    }).collect()
}

pub async fn run(state: &AppState, body: Value) -> Result<Value, ApiError> {
    let (audio, format) = decode(body)?;
    if !state.diarization_model_verified {
        return Err(ApiError::unavailable(
            "The pinned local diarization model is not installed",
        ));
    }
    let _permit = state
        .diarization_capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::capacity())?;
    let scratch = Scratch::new()?;
    let input = scratch.0.join("recording");
    let wav = scratch.0.join("recording.wav");
    tokio::fs::write(&input, &audio)
        .await
        .map_err(|_| ApiError::transport())?;
    let mut convert = Command::new(&state.config.ffmpeg_binary);
    convert
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-f",
            format,
            "-i",
        ])
        .arg(&input)
        .args([
            "-t",
            "60.1",
            "-threads",
            "1",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            "-f",
            "wav",
        ])
        .arg(&wav);
    execute(convert, 15).await?;
    let mut probe = Command::new(&state.config.ffprobe_binary);
    probe
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
        ])
        .arg(&wav);
    let metadata: Value =
        serde_json::from_slice(&execute(probe, 5).await?).map_err(|_| ApiError::transport())?;
    let duration = metadata
        .pointer("/format/duration")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_SECONDS)
        .ok_or_else(|| {
            ApiError::bad("Recording duration must be greater than zero and at most 60 seconds")
        })?;
    let mut command = Command::new(&state.config.diarization_binary);
    command
        .arg("diarize")
        .arg(&wav)
        .arg("--model")
        .arg(&state.config.diarization_model_path)
        .args(["--device", "cpu", "--format", "json"]);
    let result: Value =
        serde_json::from_slice(&execute(command, 90).await?).map_err(|_| ApiError::transport())?;
    let segments = segments(result, duration)?;
    Ok(
        json!({"model":MODEL,"model_revision":REVISION,"quantization":"q8_0","runtime":"NeMo-Speech.cpp 0.2.0 CPU","mode":"batch","duration_seconds":duration,"segments":segments,"audio_sha256":format!("{:x}",Sha256::digest(&audio)),"identity_inference":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unbounded_and_remote_audio_inputs() {
        for body in [
            json!({"audio":"https://example.com/meeting.wav","format":"wav"}),
            json!({"audio":"YQ==","format":"m3u8"}),
            json!({"audio":"YQ==","format":"wav","path":"/etc/passwd"}),
            json!({"audio":"a".repeat(2*1024*1024),"format":"wav"}),
        ] {
            assert!(decode(body).is_err());
        }
    }
    #[test]
    fn preserves_overlaps_but_rejects_identified_or_invalid_segments() {
        let value = json!({"segments":[{"start":0.0,"end":2.0,"speaker":1},{"start":1.0,"end":3.0,"speaker":2}]});
        assert_eq!(segments(value, 3.0).unwrap()[1]["speaker"], "speaker_2");
        for row in [
            json!({"start":0.0,"end":5.0,"speaker":1}),
            json!({"start":0.0,"end":1.0,"speaker":9}),
            json!({"start":-1.0,"end":1.0,"speaker":1}),
        ] {
            assert!(segments(json!({"segments":[row]}), 3.0).is_err());
        }
    }
    #[test]
    fn scratch_audio_is_deleted_on_drop() {
        let scratch = Scratch::new().unwrap();
        let path = scratch.0.clone();
        std::fs::write(path.join("recording"), b"fixture").unwrap();
        drop(scratch);
        assert!(!path.exists());
    }
    #[tokio::test]
    async fn audio_runtime_does_not_inherit_provider_credentials() {
        let mut command = Command::new("/usr/bin/env");
        command.env("NEBIUS_API_KEY", "fixture-provider-credential");
        let output = execute(command, 2).await.unwrap();
        assert!(
            !String::from_utf8(output)
                .unwrap()
                .contains("NEBIUS_API_KEY")
        );
    }
}
