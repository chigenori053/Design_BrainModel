use design_cli::control_event::ControlEvent;
use serde::{Deserialize, Serialize};

pub const MAX_HISTORY: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub run_id: String,
    pub task: String,
    pub history: Vec<serde_json::Value>,
}

impl Session {
    pub fn new(run_id: impl Into<String>, task: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            task: task.into(),
            history: Vec::new(),
        }
    }

    pub fn record_event_value(&mut self, event: serde_json::Value) {
        self.history.push(event);
        if self.history.len() > MAX_HISTORY {
            let overflow = self.history.len() - MAX_HISTORY;
            self.history.drain(0..overflow);
        }
    }

    pub fn record_control_event(&mut self, event: &ControlEvent) -> Result<(), String> {
        let value = serde_json::to_value(event).map_err(|err| err.to_string())?;
        self.record_event_value(value);
        Ok(())
    }

    pub fn compressed_history(&self) -> Vec<serde_json::Value> {
        self.history.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_history_with_fifo_eviction() {
        let mut session = Session::new("run", "task");
        for index in 0..(MAX_HISTORY + 3) {
            session.record_event_value(serde_json::json!({ "index": index }));
        }

        assert_eq!(session.history.len(), MAX_HISTORY);
        assert_eq!(session.history[0]["index"], serde_json::json!(3));
        assert_eq!(
            session.history[MAX_HISTORY - 1]["index"],
            serde_json::json!(MAX_HISTORY + 2)
        );
    }
}
