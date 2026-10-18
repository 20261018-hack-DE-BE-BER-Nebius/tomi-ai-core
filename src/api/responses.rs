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

pub async fn create(
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
    if model.starts_with("nvidia/") {
        return Err(ApiError::bad("Use Chat Completions for the Nemotron model"));
    }
    let valid_input = body.get("input").is_some_and(|input| {
        input.as_str().is_some_and(|text| !text.trim().is_empty())
            || input
                .as_array()
                .is_some_and(|items| !items.is_empty() && items.len() <= 200)
    });
    if !valid_input {
        return Err(ApiError::bad("input must be text or 1 to 200 input items"));
    }
    if body.get("tools").is_some_and(|tools| {
        tools.as_array().is_none_or(|items| {
            items.len() > 128 || items.iter().any(|tool| tool["type"] != "function")
        })
    }) {
        return Err(ApiError::bad(
            "Only caller-executed function tools are enabled",
        ));
    }
    body["model"] = model.into();
    body["store"] = false.into();
    body["background"] = false.into();
    body["max_output_tokens"] = body
        .get("max_output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(state.config.max_output_tokens)
        .min(state.config.max_output_tokens)
        .into();
    let response = provider::request(&state, endpoint, "/responses")?
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
