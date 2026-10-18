use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChatInput {
    pub session_id: String,
    #[serde(default)]
    pub request_id: String,
    pub text: String,
    pub model: String,
    pub language: String,
    #[serde(default)]
    pub context: Vec<ContextMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ClientMessage {
    #[serde(rename = "chat.message")]
    Message {
        #[serde(flatten)]
        input: ChatInput,
    },
    #[serde(rename = "chat.steer")]
    Steer { run_id: String, text: String },
    #[serde(rename = "chat.cancel")]
    Cancel { run_id: String },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub run_id: String,
    pub session_id: String,
    pub timestamp: String,
    pub data: Value,
}
impl TraceEvent {
    pub fn new(kind: &str, run_id: &str, session_id: &str, mut data: Value) -> Self {
        redact(&mut data);
        Self {
            kind: kind.into(),
            run_id: run_id.into(),
            session_id: session_id.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            data,
        }
    }
}

fn redact(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if crate::domain::redaction::sensitive_field(key) {
                    *value = Value::String("[redacted]".into());
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trace_fields_redact_secrets_without_hiding_usage() {
        let event = TraceEvent::new(
            "tool.completed",
            "r",
            "s",
            serde_json::json!({"headers":{"authorization":"sensitive"},"input_tokens":12}),
        );
        assert_eq!(event.data["headers"]["authorization"], "[redacted]");
        assert_eq!(event.data["input_tokens"], 12);
    }
}
