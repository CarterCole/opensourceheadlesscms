use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub roles: Vec<String>,
    /// If set, auto-trigger when this field has a past datetime value
    pub trigger: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub name: String,
    pub states: Vec<String>,
    pub transitions: Vec<Transition>,
}

impl WorkflowConfig {
    pub fn initial_state(&self) -> &str {
        self.states.first().map(|s| s.as_str()).unwrap_or("draft")
    }

    pub fn is_valid_state(&self, state: &str) -> bool {
        self.states.iter().any(|s| s == state)
    }

    pub fn allowed_transitions(&self, from: &str, roles: &[String]) -> Vec<&str> {
        self.transitions
            .iter()
            .filter(|t| {
                t.from == from && (t.roles.is_empty() || t.roles.iter().any(|r| roles.contains(r)))
            })
            .map(|t| t.to.as_str())
            .collect()
    }

    pub fn can_transition(&self, from: &str, to: &str, roles: &[String]) -> bool {
        self.transitions.iter().any(|t| {
            t.from == from
                && t.to == to
                && (t.roles.is_empty() || t.roles.iter().any(|r| roles.contains(r)))
        })
    }

    pub fn scheduled_transitions(&self) -> Vec<(&str, &str)> {
        self.transitions
            .iter()
            .filter(|t| t.trigger.is_some())
            .map(|t| (t.from.as_str(), t.to.as_str()))
            .collect()
    }
}
