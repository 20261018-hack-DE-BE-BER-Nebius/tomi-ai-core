use crate::api::types::ContextMessage;
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub struct SessionHistory {
    messages: HashMap<String, Vec<ContextMessage>>,
    order: VecDeque<String>,
}
impl SessionHistory {
    pub fn get(&self, session: &str) -> Vec<ContextMessage> {
        self.messages.get(session).cloned().unwrap_or_default()
    }
    pub fn remember(&mut self, session: &str, question: &str, answer: &str) {
        if !self.messages.contains_key(session) {
            if self.order.len() == 16
                && let Some(oldest) = self.order.pop_front()
            {
                self.messages.remove(&oldest);
            }
            self.order.push_back(session.into());
        }
        let history = self.messages.entry(session.into()).or_default();
        history.push(ContextMessage {
            role: "user".into(),
            content: question.into(),
        });
        history.push(ContextMessage {
            role: "assistant".into(),
            content: answer.chars().take(12000).collect(),
        });
        if history.len() > 30 {
            history.drain(..history.len() - 30);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_context_does_not_cross_conversations() {
        let mut history = SessionHistory::default();
        history.remember("owner:one", "private question", "answer");
        assert!(history.get("owner:two").is_empty());
        assert_eq!(history.get("owner:one")[0].content, "private question");
    }
    #[test]
    fn history_is_bounded() {
        let mut history = SessionHistory::default();
        for index in 0..17 {
            history.remember(&format!("session-{index}"), "question", "answer");
        }
        assert!(history.get("session-0").is_empty());
        for _ in 0..50 {
            history.remember("session-16", "question", "answer");
        }
        assert_eq!(history.get("session-16").len(), 30);
    }
}
