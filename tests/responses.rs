mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use support::fixture;
use tomi_ai_core::{AppState, api};
use tower::ServiceExt;

fn request(body: Value, auth: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header("content-type", "application/json");
    if auth {
        builder = builder.header(
            "authorization",
            "Bearer fixture-internal-token-with-32-characters",
        );
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn responses_requires_auth_and_keeps_function_tools_with_bounded_stateless_output() {
    let (state, server) = fixture().await;
    let app = api::router(state);
    let body = json!({"model":"gpt-6-astra","input":"Look up air","max_output_tokens":999999,"store":true,"background":true,"tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}}}]});
    assert_eq!(
        app.clone()
            .oneshot(request(body.clone(), false))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = app
        .clone()
        .oneshot(request(body.clone(), true))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["status"], "completed");
    let mut streaming = body;
    streaming["stream"] = true.into();
    let response = app.clone().oneshot(request(streaming, true)).await.unwrap();
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .contains("response.completed")
    );
    for body in [
        json!({"model":"unapproved","input":"hi"}),
        json!({"model":"gpt-6-astra","input":"hi","tools":[{"type":"mcp","server_url":"https://untrusted.invalid"}]}),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(body, true))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    server.abort();
}

#[tokio::test]
async fn catalog_verifies_parse_and_both_retrieval_endpoints() {
    let (state, server) = fixture().await;
    let mut config = (*state.config).clone();
    config.parse.key.clear();
    let result = api::models::catalog(&AppState::new(config).unwrap()).await;
    let parse = result["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "nemotron-parse-3.5")
        .unwrap();
    assert_eq!(parse["available"], true);
    assert_eq!(parse["upstream_model"], "nvidia/nemotron-3.5-parse");
    for id in ["nemotron-3-embed-vl", "nemotron-3-rerank-vl"] {
        assert_eq!(
            result["data"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["id"] == id)
                .unwrap()["available"],
            true
        );
    }
    server.abort();
}
