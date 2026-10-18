use crate::{AppState, api::error::ApiError, domain::policy, infrastructure::provider};
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchInput {
    pub query: String,
    pub language: String,
    pub top_k: Option<usize>,
}

pub async fn search(
    State(state): State<AppState>,
    Json(input): Json<SearchInput>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let _permit = state
        .run_capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::capacity())?;
    let top_k = input.top_k.unwrap_or(6);
    if !policy::validate_text(&input.query)
        || !policy::valid_language(&input.language)
        || !(1..=12).contains(&top_k)
    {
        return Err(ApiError::bad(
            "Provide query, language en/es/de/fr, and top_k between 1 and 12",
        ));
    }
    let result = provider::hermes(
        &state,
        "/v1/retrieve",
        &json!({"question":input.query,"locale":input.language,"top_k":top_k}),
    )
    .await?;
    if !result.is_object() || !result.get("candidates").is_some_and(Value::is_array) {
        return Err(ApiError::transport());
    }
    let status = if result["status"] == "blocked" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    Ok((status, Json(result)))
}
