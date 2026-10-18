use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use serde_json::{Value, json};
use tomi_ai_core::{
    AppState,
    config::{Config, Endpoint},
};

fn config(base: &str) -> Config {
    let endpoint = Endpoint {
        base_url: base.into(),
        key: "fixture-provider-secret".into(),
        auth_header: "bearer".into(),
    };
    Config {
        http_addr: "127.0.0.1:0".parse().unwrap(),
        grpc_addr: "127.0.0.1:0".parse().unwrap(),
        core_token: "fixture-internal-token-with-32-characters".into(),
        hermes_url: base.into(),
        openai: endpoint.clone(),
        nebius: endpoint.clone(),
        parse: endpoint.clone(),
        embed: endpoint.clone(),
        rerank: endpoint.clone(),
        diarize: endpoint,
        diarization_binary: String::new(),
        diarization_model_path: String::new(),
        ffmpeg_binary: "ffmpeg".into(),
        ffprobe_binary: "ffprobe".into(),
        reasoning_model: "gpt-6-astra".into(),
        image_model: "gpt-image-2.5-sunburst".into(),
        realtime_model: "gpt-realtime-2.1".into(),
        transcription_model: "gpt-live-transcribe".into(),
        nemotron_model: "nvidia/Nemotron-3.5-Super-VL".into(),
        parse_model: "nvidia/nemotron-3.5-parse".into(),
        embed_model: "nvidia/llama-nemotron-embed-vl-1b-v2".into(),
        rerank_model: "nvidia/llama-nemotron-rerank-vl-1b-v2".into(),
        voice: "marin".into(),
        log_filter: "off".into(),
        request_timeout_seconds: 5,
        max_concurrent_runs: 16,
        max_output_tokens: 4096,
    }
}
pub async fn fixture() -> (AppState, tokio::task::JoinHandle<()>) {
    let router=Router::new().route("/v1/retrieve",post(||async{Json(json!({"status":"ok","candidates":[{"document_id":"b1","title":"Air","corpus":"britannica","text":"Historical observed source.","source_url":"https://digital.nls.uk/1"},{"document_id":"w1","title":"Air","corpus":"wikipedia","text":"Modern observed source.","source_url":"https://en.wikipedia.org/wiki/Air"}]}))}))
        .route("/v1/compare",post(|Json(body):Json<Value>|async move{
            assert_eq!(body["question"],"Compare shipbuilding. Only compare shelf-pieces and shipyards.");
            assert_eq!(body["discovery_query"],"Shipbuilding techniques");
            Json(json!({"status":"ok","answer":"Fixture narrow comparison.","candidates":[{"document_id":"b1","title":"Shipbuilding","corpus":"britannica","text":"Fixture source.","source_url":"https://digital.nls.uk/1"}]}))
        }))
        .route("/chat/completions",post(|Json(body):Json<Value>|async move{
            if body["model"]=="nvidia/Nemotron-3.5-Super-VL" {
                assert_eq!(body["reasoning_effort"],"none");
                return Json(json!({"choices":[{"message":{"content":"Shipbuilding techniques"}}]})).into_response();
            }
            assert_eq!(body["model"],"gpt-6-astra");
            if body["stream"]==true{(StatusCode::OK,[("content-type","text/event-stream")],"data: {\"choices\":[{\"delta\":{\"content\":\"Fixture comparison [b1] [w1].\"}}]}\n\ndata: [DONE]\n\n").into_response()}
            else{Json(json!({"choices":[{"message":{"content":"Fixture response"}}]})).into_response()}
        })).route("/responses",post(|Json(body):Json<Value>|async move{
            assert_eq!(body["model"],"gpt-6-astra");
            assert_eq!(body["max_output_tokens"],4096);
            assert_eq!(body["store"],false);
            assert_eq!(body["background"],false);
            if body["stream"]==true{(StatusCode::OK,[("content-type","text/event-stream")],"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n").into_response()}
            else{Json(json!({"object":"response","status":"completed","output":[]})).into_response()}
        })).route("/models",get(||async{Json(json!({"data":[{"id":"gpt-6-astra"},{"id":"nvidia/nemotron-3.5-parse"},{"id":"nvidia/llama-nemotron-embed-vl-1b-v2"},{"id":"nvidia/llama-nemotron-rerank-vl-1b-v2"}]}))}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (
        AppState::new(config(&format!("http://{addr}"))).unwrap(),
        server,
    )
}
use axum::response::IntoResponse;
