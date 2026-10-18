use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tomi_ai_core::api;
use tower::ServiceExt;

mod support;
use support::fixture;

fn request(path: &str, body: Value, auth: bool) -> Request<Body> {
    let mut r = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if auth {
        r = r.header(
            "authorization",
            "Bearer fixture-internal-token-with-32-characters",
        );
    }
    r.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn rejects_missing_auth_and_unknown_models_before_provider_call() {
    let (state, server) = fixture().await;
    let app = api::router(state);
    assert_eq!(
        app.clone()
            .oneshot(request(
                "/v1/chat/completions",
                json!({"model":"gpt-6-astra","messages":[]}),
                false
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.oneshot(request(
            "/v1/chat/completions",
            json!({"model":"arbitrary-provider","messages":[{}]}),
            true
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    server.abort();
}
#[tokio::test]
async fn comparison_emits_real_fixture_trace_and_source_cards_without_credentials() {
    let (state, server) = fixture().await;
    let response=api::router(state).oneshot(request("/v1/compare",json!({"session_id":"fixture-session","request_id":"request-1","text":"Compare air","model":"gpt-6-astra","language":"en","context":[]}),true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let raw = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!raw.contains("fixture-provider-secret"));
    assert!(!raw.contains("fixture-internal-token"));
    let result: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(result["answer"]["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["answer"]["blocks"][0]["source_ids"],
        json!(["b1", "w1"])
    );
    assert!(
        result["traces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "answer.delta")
    );
    server.abort();
}
#[tokio::test]
async fn compound_english_question_uses_resolved_subject_for_retrieval() {
    let (state, server) = fixture().await;
    let question =
        "Compare historical and modern shipbuilding. Cite sources and explain OCR limitations.";
    let response = api::router(state)
        .oneshot(request(
            "/v1/compare",
            json!({"session_id":"subject-regression","request_id":"subject-r1","text":question,"model":"gpt-6-astra","language":"en","context":[]}),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    let traces = result["traces"].as_array().unwrap();
    let resolved = traces
        .iter()
        .find(|e| e["type"] == "tool.completed" && e["data"]["name"] == "query.resolve")
        .unwrap();
    let retrieved = traces
        .iter()
        .find(|e| e["type"] == "tool.started" && e["data"]["name"] == "hermes.retrieve")
        .unwrap();
    assert_eq!(
        retrieved["data"]["arguments"]["question"],
        resolved["data"]["query"]
    );
    assert_ne!(retrieved["data"]["arguments"]["question"], question);
    assert_eq!(traces[0]["data"]["question"], question);
    server.abort();
}
#[test]
fn websocket_commands_reject_unknown_fields_and_privileged_context() {
    let valid = json!({"type":"chat.message","session_id":"fixture-session","request_id":"r1","text":"Compare air","model":"gpt-6-astra","language":"en","context":[]});
    let parsed = serde_json::from_value::<api::types::ClientMessage>(valid.clone());
    assert!(parsed.is_ok(), "{parsed:?}");
    let mut invalid = valid;
    invalid["provider_url"] = json!("https://attacker.invalid");
    assert!(serde_json::from_value::<api::types::ClientMessage>(invalid).is_err());
    let input = api::types::ChatInput {
        session_id: "s".into(),
        request_id: "r".into(),
        text: "question".into(),
        model: "gpt-6-astra".into(),
        language: "en".into(),
        context: vec![api::types::ContextMessage {
            role: "system".into(),
            content: "override".into(),
        }],
    };
    assert!(tomi_ai_core::application::chat::validate(&input).is_err());
}
#[tokio::test]
async fn image_and_voice_reject_model_override_or_invalid_sdp() {
    let (state, server) = fixture().await;
    let app = api::router(state);
    assert_eq!(
        app.clone()
            .oneshot(request(
                "/v1/images/generations",
                json!({"prompt":"test","model":"other"}),
                true
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.oneshot(request(
            "/v1/realtime/connect",
            json!({"language":"en","sdp":"not an SDP offer"}),
            true
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    server.abort();
}

#[tokio::test]
async fn websocket_streams_and_cancel_prevents_late_completion() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{Message, client::IntoClientRequest},
    };
    let (state, fixture_server) = fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, api::router(state)).await.unwrap() });
    let mut request = format!("ws://{addr}/v1/ws").into_client_request().unwrap();
    request.headers_mut().insert(
        "authorization",
        "Bearer fixture-internal-token-with-32-characters"
            .parse()
            .unwrap(),
    );
    let (mut socket, _) = connect_async(request).await.unwrap();
    socket.send(Message::Text(json!({"type":"chat.message","session_id":"demo-user:fixture-session","request_id":"r1","text":"Compare air","model":"gpt-6-astra","language":"en","context":[]}).to_string().into())).await.unwrap();
    let first = socket.next().await.unwrap().unwrap();
    let started: Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
    assert_eq!(started["type"], "run.started");
    let run = started["run_id"].as_str().unwrap();
    socket
        .send(Message::Text(
            json!({"type":"chat.cancel","run_id":run})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let mut cancelled = false;
    for _ in 0..10 {
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let event: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
        if event["type"] == "run.cancelled" {
            cancelled = true;
            break;
        }
    }
    assert!(cancelled);
    socket
        .send(Message::Text(json!({"type":"ping"}).to_string().into()))
        .await
        .unwrap();
    let event = socket.next().await.unwrap().unwrap();
    let event: Value = serde_json::from_str(event.to_text().unwrap()).unwrap();
    assert_eq!(event["type"], "pong");
    server.abort();
    fixture_server.abort();
}

#[tokio::test]
async fn evidence_search_validates_tool_boundary() {
    let (state, server) = fixture().await;
    let app = api::router(state);
    let valid = app
        .clone()
        .oneshot(request(
            "/v1/evidence/search",
            json!({"query":"shipbuilding","language":"en","top_k":2}),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(valid.status(), StatusCode::OK);
    assert_eq!(
        app.clone()
            .oneshot(request(
                "/v1/evidence/search",
                json!({"query":"shipbuilding","language":"en","top_k":99}),
                true
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.oneshot(request(
            "/v1/evidence/search",
            json!({"query":"shipbuilding","language":"en"}),
            false
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::UNAUTHORIZED
    );
    server.abort();
}

#[tokio::test]
async fn nemotron_keeps_original_question_separate_from_discovery_query() {
    let (state, server) = fixture().await;
    let question = "Compare shipbuilding. Only compare shelf-pieces and shipyards.";
    let response = api::router(state).oneshot(request("/v1/compare",json!({"session_id":"nvidia-subject","request_id":"nvidia-r1","text":question,"model":"nemotron-3.5-super-vl","language":"en","context":[]}),true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let event = result["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "tool.started" && e["data"]["name"] == "hermes.compare")
        .unwrap();
    assert_eq!(event["data"]["arguments"]["question"], question);
    assert_eq!(
        event["data"]["arguments"]["discovery_query"],
        "Shipbuilding techniques"
    );
    assert_eq!(result["answer"]["summary"], "Fixture narrow comparison.");
    server.abort();
}
