use crate::{AppState, api::error::ApiError, infrastructure::provider};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::header,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde_json::Value;

pub async fn chat(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> Result<Response, ApiError> {
    let permit = state
        .capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::capacity())?;
    let id = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad("model is required"))?;
    let (endpoint, model) = provider::chat_target(&state, id)?;
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::bad("messages must be an array"))?;
    if messages.is_empty() || messages.len() > 100 {
        return Err(ApiError::bad("messages must contain 1 to 100 entries"));
    }
    body["model"] = model.into();
    clamp_tokens(&state, &mut body);
    let response = provider::request(&state, endpoint, "/chat/completions")?
        .json(&body)
        .send()
        .await
        .map_err(|_| ApiError::transport())?;
    let response = provider::checked(response).await?;
    if body.get("stream").and_then(Value::as_bool).unwrap_or(false) {
        let stream = response.bytes_stream().map(move |item| {
            let _keep_permit = &permit;
            item
        });
        return Ok((
            [
                (header::CONTENT_TYPE, "text/event-stream"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            Body::from_stream(stream),
        )
            .into_response());
    }
    Ok(Json(provider::json(response).await?).into_response())
}

fn clamp_tokens(state: &AppState, body: &mut Value) {
    let key = if body.get("max_tokens").is_some() {
        "max_tokens"
    } else {
        "max_completion_tokens"
    };
    let max = body
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(state.config.max_output_tokens)
        .min(state.config.max_output_tokens);
    body[key] = max.into();
}

pub async fn embeddings(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _permit = state
        .capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::capacity())?;
    if body.get("input").is_none() {
        return Err(ApiError::bad("input is required"));
    }
    body["model"] = state.config.embed_model.clone().into();
    provider::post(&state, &state.config.embed, "/embeddings", &body)
        .await
        .map(Json)
}

pub async fn rerank(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _permit = state
        .capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::capacity())?;
    if body.get("query").is_none() || body.get("documents").is_none() {
        return Err(ApiError::bad("query and documents are required"));
    }
    body["model"] = state.config.rerank_model.clone().into();
    provider::post(&state, &state.config.rerank, "/rerank", &body)
        .await
        .map(Json)
}

pub async fn parse(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _permit = state
        .capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::capacity())?;
    if body.get("messages").is_none() {
        return Err(ApiError::bad(
            "messages with document image content are required",
        ));
    }
    body["model"] = state.config.parse_model.clone().into();
    provider::post(&state, &state.config.parse, "/chat/completions", &body)
        .await
        .map(Json)
}

pub async fn diarize(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _permit = state
        .capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::capacity())?;
    if body.get("audio").is_none() {
        return Err(ApiError::bad("audio is required"));
    }
    provider::post(&state, &state.config.diarize, "/diarize", &body)
        .await
        .map(Json)
}
