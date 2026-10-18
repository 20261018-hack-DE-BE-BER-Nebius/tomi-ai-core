use crate::{AppState, config::Endpoint, infrastructure::provider};
use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

async fn discover(state: &AppState, endpoint: &Endpoint) -> Option<Vec<String>> {
    if endpoint.base_url.is_empty() {
        return None;
    }
    let request = state
        .client
        .get(format!("{}/models", endpoint.base_url))
        .timeout(std::time::Duration::from_secs(8));
    let response = provider::authenticate(request, endpoint)
        .send()
        .await
        .ok()?;
    let data = provider::json(response).await.ok()?;
    Some(
        data.get("data")?
            .as_array()?
            .iter()
            .filter_map(|m| m.get("id")?.as_str().map(String::from))
            .collect(),
    )
}

fn item(
    id: &str,
    actual: &str,
    provider: &str,
    capability: &str,
    configured: bool,
    listed: &Option<Vec<String>>,
) -> Value {
    let discovered = listed
        .as_ref()
        .is_some_and(|models| models.iter().any(|m| m == actual));
    let available = configured && discovered;
    json!({"id":id,"name":id,"object":"model","provider":provider,"upstream_model":actual,"capabilities":[capability],"available":available,
        "availability_reason":if discovered {"listed_by_provider; invocation still subject to deployment and quota"} else if configured {"configured_but_not_verified_in_provider_catalog"} else {"provider_not_configured"}})
}

pub async fn catalog(state: &AppState) -> Value {
    let (openai, nebius, parse) = tokio::join!(
        discover(state, &state.config.openai),
        discover(state, &state.config.nebius),
        discover(state, &state.config.parse)
    );
    let c = &state.config;
    let mut data: Vec<Value> = [c.reasoning_model.as_str(), "gpt-5.6-luna", "gpt-5.6-terra"]
        .iter()
        .map(|m| {
            item(
                m,
                m,
                "openai-compatible",
                "chat",
                !c.openai.key.is_empty(),
                &openai,
            )
        })
        .collect();
    for (id, cap) in [
        (&c.image_model, "image"),
        (&c.realtime_model, "realtime"),
        (&c.transcription_model, "transcription"),
    ] {
        data.push(item(
            id,
            id,
            "openai-compatible",
            cap,
            !c.openai.key.is_empty(),
            &openai,
        ));
    }
    data.push(item(
        "nemotron-3.5-super-vl",
        &c.nemotron_model,
        "nebius",
        "chat",
        !c.nebius.key.is_empty(),
        &nebius,
    ));
    for (id, model, endpoint, cap) in [
        ("nemotron-parse-3.5", &c.parse_model, &c.parse, "parse"),
        ("nemotron-3-embed-vl", &c.embed_model, &c.embed, "embedding"),
        ("nemotron-3-rerank-vl", &c.rerank_model, &c.rerank, "rerank"),
    ] {
        data.push(item(
            id,
            model,
            "nvidia",
            cap,
            !endpoint.base_url.is_empty(),
            if cap == "parse" { &parse } else { &None },
        ));
    }
    json!({"object":"list","data":data,"checked_at":chrono::Utc::now().to_rfc3339()})
}

pub async fn list(State(state): State<AppState>) -> Json<Value> {
    Json(catalog(&state).await)
}

pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let c = &state.config;
    let configured = !c.openai.key.is_empty() || !c.nebius.key.is_empty();
    let challenge_configured = !c.nebius.key.is_empty()
        && !c.parse.base_url.is_empty()
        && !c.embed.base_url.is_empty()
        && !c.rerank.base_url.is_empty();
    (
        if configured {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(
            json!({"status":if configured {"configured"} else {"blocked"},"provider_credentials_configured":configured,"challenge_endpoints_configured":challenge_configured,"live_inference_verified":false}),
        ),
    )
}
