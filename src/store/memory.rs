use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct StoredEvent {
    pub event_id: String,
    pub subject: String,
    pub scope: String,
    pub action: String,
    pub state_version: String,
    pub relation_transitions: Vec<String>,
    pub provenance: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredDecision {
    pub decision_id: String,
    pub subject: String,
    pub scope: String,
    pub action: String,
    pub verdict: String,
    pub reason_code: String,
    pub governing_relation_id: Option<String>,
    pub state_version: String,
    pub provenance: Vec<String>,
}

#[derive(Debug, Default)]
pub struct MemoryEventStore {
    events: Vec<StoredEvent>,
    decisions: Vec<StoredDecision>,
    state_version: u64,
    decision_counter: u64,
}

impl MemoryEventStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_state_version(&self) -> String {
        format!("state-{:06}", self.state_version)
    }

    pub fn advance_state_version(&mut self) -> String {
        self.state_version += 1;
        self.current_state_version()
    }

    pub fn record_event(&mut self, event: StoredEvent) {
        self.events.push(event);
    }

    pub fn next_decision_id(&mut self) -> String {
        self.decision_counter += 1;
        format!("decision-{:06}", self.decision_counter)
    }

    pub fn record_decision(&mut self, decision: StoredDecision) {
        self.decisions.push(decision);
    }

    pub fn decision(&self, decision_id: &str) -> Option<&StoredDecision> {
        self.decisions.iter().find(|d| d.decision_id == decision_id)
    }

    pub fn event(&self, event_id: &str) -> Option<&StoredEvent> {
        self.events.iter().find(|e| e.event_id == event_id)
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn decision_count(&self) -> usize {
        self.decisions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_version_only_moves_when_explicitly_advanced() {
        let mut store = MemoryEventStore::new();
        assert_eq!(store.current_state_version(), "state-000000");
        assert_eq!(store.current_state_version(), "state-000000");
        assert_eq!(store.advance_state_version(), "state-000001");
    }

    #[test]
    fn decision_ids_are_deterministic() {
        let mut store = MemoryEventStore::new();
        assert_eq!(store.next_decision_id(), "decision-000001");
        assert_eq!(store.next_decision_id(), "decision-000002");
    }
}
