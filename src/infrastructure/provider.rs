use crate::{AppState, api::error::ApiError, config::Endpoint};
use futures_util::StreamExt;
use serde_json::Value;

pub const MAX_UPSTREAM_BYTES: usize = 24 * 1024 * 1024;

pub fn request(
    state: &AppState,
    endpoint: &Endpoint,
    path: &str,
) -> Result<reqwest::RequestBuilder, ApiError> {
    if endpoint.base_url.is_empty() {
        return Err(ApiError::unavailable(
            "Required model endpoint has not been configured",
        ));
    }
    let request = state.client.post(format!("{}{path}", endpoint.base_url));
    Ok(authenticate(request, endpoint))
}

pub fn authenticate(
    request: reqwest::RequestBuilder,
    endpoint: &Endpoint,
) -> reqwest::RequestBuilder {
    if endpoint.key.is_empty() {
        return request;
    }
    if endpoint.auth_header == "api-key" {
        request.header("api-key", &endpoint.key)
    } else {
        request.bearer_auth(&endpoint.key)
    }
}

pub async fn checked(response: reqwest::Response) -> Result<reqwest::Response, ApiError> {
    if !response.status().is_success() {
        return Err(ApiError::provider(response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_UPSTREAM_BYTES as u64)
    {
        return Err(ApiError::bad("Upstream response exceeded size limit"));
    }
    Ok(response)
}

pub async fn bytes(response: reqwest::Response) -> Result<Vec<u8>, ApiError> {
    read_bytes(checked(response).await?).await
}

async fn read_bytes(response: reqwest::Response) -> Result<Vec<u8>, ApiError> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::transport())?;
        if bytes.len() + chunk.len() > MAX_UPSTREAM_BYTES {
            return Err(ApiError::bad("Upstream response exceeded size limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn json(response: reqwest::Response) -> Result<Value, ApiError> {
    serde_json::from_slice(&bytes(response).await?).map_err(|_| ApiError::transport())
}

pub async fn post(
    state: &AppState,
    endpoint: &Endpoint,
    path: &str,
    body: &Value,
) -> Result<Value, ApiError> {
    let response = request(state, endpoint, path)?
        .json(body)
        .send()
        .await
        .map_err(|_| ApiError::transport())?;
    json(response).await
}

pub async fn hermes(state: &AppState, path: &str, body: &Value) -> Result<Value, ApiError> {
    let response = state
        .client
        .post(format!("{}{path}", state.config.hermes_url))
        .bearer_auth(&state.config.core_token)
        .json(body)
        .send()
        .await
        .map_err(|_| ApiError::transport())?;
    if response.status().as_u16() == 503 {
        let value: Value = serde_json::from_slice(&read_bytes(response).await?)
            .map_err(|_| ApiError::transport())?;
        if value.get("status").and_then(Value::as_str) == Some("blocked")
            && value.get("blockers").is_some_and(Value::is_array)
        {
            return Ok(value);
        }
        return Err(ApiError::unavailable("The evidence service is unavailable"));
    }
    json(response).await
}

pub fn chat_target<'a>(state: &'a AppState, id: &str) -> Result<(&'a Endpoint, String), ApiError> {
    let c = &state.config;
    if id == "nemotron-3.5-super-vl" || id == c.nemotron_model {
        if c.nebius.key.is_empty() {
            return Err(ApiError::unavailable(
                "Nebius Token Factory credentials are not configured",
            ));
        }
        return Ok((&c.nebius, c.nemotron_model.clone()));
    }
    if [c.reasoning_model.as_str(), "gpt-5.6-luna", "gpt-5.6-terra"].contains(&id) {
        if c.openai.key.is_empty() {
            return Err(ApiError::unavailable(
                "OpenAI-compatible credentials are not configured",
            ));
        }
        return Ok((&c.openai, id.into()));
    }
    Err(ApiError::bad(
        "Model is not in the configured chat allowlist",
    ))
}
