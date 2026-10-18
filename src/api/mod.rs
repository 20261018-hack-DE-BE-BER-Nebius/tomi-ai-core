pub mod error;
pub mod evidence;
pub mod gateway;
pub mod grpc;
pub mod media;
pub mod models;
pub mod responses;
pub mod types;
pub mod ws;

use crate::AppState;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use serde_json::json;
use subtle::ConstantTimeEq;

pub fn authorized(value: Option<&str>, secret: &str) -> bool {
    value
        .and_then(|s| s.strip_prefix("Bearer "))
        .is_some_and(|s| s.as_bytes().ct_eq(secret.as_bytes()).into())
}

async fn auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !authorized(
        request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok()),
        &state.config.core_token,
    ) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    Ok(response)
}

pub fn router(state: AppState) -> Router {
    let grpc_routes = grpc::http_routes(state.clone());
    let protected = Router::new()
        .route("/v1/models", get(models::list))
        .route("/v1/evidence/search", post(evidence::search))
        .route("/v1/chat/completions", post(gateway::chat))
        .route("/v1/responses", post(responses::create))
        .route("/v1/embeddings", post(gateway::embeddings))
        .route("/v1/rerank", post(gateway::rerank))
        .route("/v1/parse", post(gateway::parse))
        .route("/v1/diarize", post(gateway::diarize))
        .route("/v1/ws", get(ws::upgrade))
        .route("/v1/compare", post(ws::compare))
        .route("/v1/images/generations", post(media::image))
        .route("/v1/realtime/session", post(media::session))
        .route("/v1/realtime/connect", post(media::connect))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth));
    Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"status":"ok","service":"tomi-ai-core"})) }),
        )
        .route("/ready", get(models::ready))
        .merge(protected)
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .with_state(state)
        .merge(grpc_routes)
}
