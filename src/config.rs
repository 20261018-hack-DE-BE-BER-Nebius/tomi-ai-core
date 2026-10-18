use std::{env, net::SocketAddr};

#[derive(Clone)]
pub struct Endpoint {
    pub base_url: String,
    pub key: String,
    pub auth_header: String,
}

#[derive(Clone)]
pub struct Config {
    pub http_addr: SocketAddr,
    pub grpc_addr: SocketAddr,
    pub core_token: String,
    pub hermes_url: String,
    pub openai: Endpoint,
    pub nebius: Endpoint,
    pub parse: Endpoint,
    pub embed: Endpoint,
    pub rerank: Endpoint,
    pub diarize: Endpoint,
    pub diarization_binary: String,
    pub diarization_model_path: String,
    pub ffmpeg_binary: String,
    pub ffprobe_binary: String,
    pub reasoning_model: String,
    pub image_model: String,
    pub realtime_model: String,
    pub transcription_model: String,
    pub nemotron_model: String,
    pub parse_model: String,
    pub embed_model: String,
    pub rerank_model: String,
    pub voice: String,
    pub log_filter: String,
    pub request_timeout_seconds: u64,
    pub max_concurrent_runs: usize,
    pub max_output_tokens: u64,
}

fn value(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn secret(key: &str) -> String {
    let value = value(key, "");
    if value == "changeme" || value.starts_with("replace-") {
        String::new()
    } else {
        value
    }
}

fn endpoint(prefix: &str, default: &str) -> Result<Endpoint, String> {
    let base_url = value(&format!("{prefix}_BASE_URL"), default)
        .trim_end_matches('/')
        .to_owned();
    if !base_url.is_empty() {
        let url =
            reqwest::Url::parse(&base_url).map_err(|_| format!("Invalid {prefix}_BASE_URL"))?;
        if !matches!(url.scheme(), "https" | "http")
            || url.password().is_some()
            || !url.username().is_empty()
        {
            return Err(format!(
                "Invalid {prefix}_BASE_URL scheme or embedded credentials"
            ));
        }
    }
    Ok(Endpoint {
        base_url,
        key: secret(&format!("{prefix}_API_KEY")),
        auth_header: value(&format!("{prefix}_AUTH_HEADER"), "bearer"),
    })
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        dotenvy::dotenv().ok();
        let core_token = secret("TOMI_CORE_TOKEN");
        if core_token.len() < 32 {
            return Err("TOMI_CORE_TOKEN must contain at least 32 random characters".into());
        }
        Ok(Self {
            http_addr: value("TOMI_HTTP_ADDR", "127.0.0.1:8081")
                .parse()
                .map_err(|_| "Invalid TOMI_HTTP_ADDR")?,
            grpc_addr: value("TOMI_GRPC_ADDR", "127.0.0.1:50051")
                .parse()
                .map_err(|_| "Invalid TOMI_GRPC_ADDR")?,
            core_token,
            hermes_url: value("TOMI_HERMES_URL", "http://127.0.0.1:8083")
                .trim_end_matches('/')
                .into(),
            openai: endpoint("OPENAI", "https://api.openai.com/v1")?,
            nebius: endpoint("NEBIUS", "https://api.tokenfactory.nebius.com/v1")?,
            parse: endpoint("NEMOTRON_PARSE", "")?,
            embed: endpoint("NEMOTRON_EMBED", "")?,
            rerank: endpoint("NEMOTRON_RERANK", "")?,
            diarize: endpoint("NEMOTRON_DIARIZE", "")?,
            diarization_binary: value("DIARIZATION_BINARY", "/opt/nemo/bin/nemo-speech"),
            diarization_model_path: value(
                "DIARIZATION_MODEL_PATH",
                "/opt/models/Nemotron-3-Diarization.q8_0.gguf",
            ),
            ffmpeg_binary: value("FFMPEG_BINARY", "ffmpeg"),
            ffprobe_binary: value("FFPROBE_BINARY", "ffprobe"),
            reasoning_model: value("AI_REASONING_MODEL", "gpt-6-astra"),
            image_model: value("AI_IMAGE_MODEL", "gpt-image-2.5-sunburst"),
            realtime_model: value("AI_REALTIME_MODEL", "gpt-realtime-2.1"),
            transcription_model: value("AI_TRANSCRIPTION_MODEL", "gpt-live-transcribe"),
            nemotron_model: value("NEMOTRON_AGENT_MODEL", "nvidia/Nemotron-3.5-Super-VL"),
            parse_model: value("NEMOTRON_PARSE_MODEL", "nvidia/nemotron-3.5-parse"),
            embed_model: value(
                "NEMOTRON_EMBED_MODEL",
                "nvidia/llama-nemotron-embed-vl-1b-v2",
            ),
            rerank_model: value(
                "NEMOTRON_RERANK_MODEL",
                "nvidia/llama-nemotron-rerank-vl-1b-v2",
            ),
            voice: value("AI_VOICE", "marin"),
            log_filter: value("RUST_LOG", "tomi_ai_core=info"),
            request_timeout_seconds: value("AI_REQUEST_TIMEOUT_SECONDS", "180")
                .parse()
                .map_err(|_| "Invalid AI_REQUEST_TIMEOUT_SECONDS")?,
            max_concurrent_runs: value("AI_MAX_CONCURRENT_RUNS", "16")
                .parse()
                .map_err(|_| "Invalid AI_MAX_CONCURRENT_RUNS")?,
            max_output_tokens: value("AI_MAX_OUTPUT_TOKENS", "4096")
                .parse()
                .map_err(|_| "Invalid AI_MAX_OUTPUT_TOKENS")?,
        })
    }
}
