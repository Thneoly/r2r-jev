use super::{DomainKey, EventStore, StoredDecision, StoredEvent};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct MemoryEventStore {
    events: Vec<StoredEvent>,
    decisions: Vec<StoredDecision>,
    state_versions: HashMap<DomainKey, u64>,
    decision_counter: u64,
}

impl MemoryEventStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn decision_count(&self) -> usize {
        self.decisions.len()
    }

    fn state_counter(&self, domain: &DomainKey) -> u64 {
        *self.state_versions.get(domain).unwrap_or(&0)
    }
}

impl EventStore for MemoryEventStore {
    fn current_state_version(&self, domain: &DomainKey) -> String {
        format!("state-{:06}", self.state_counter(domain))
    }

    fn advance_state_version(&mut self, domain: &DomainKey) -> String {
        let counter = self.state_versions.entry(domain.clone()).or_insert(0);
        *counter += 1;
        format!("state-{counter:06}")
    }

    fn record_event(&mut self, event: StoredEvent) {
        self.events.push(event);
    }

    fn next_decision_id(&mut self) -> String {
        self.decision_counter += 1;
        format!("decision-{:06}", self.decision_counter)
    }

    fn record_decision(&mut self, decision: StoredDecision) {
        self.decisions.push(decision);
    }

    fn decision(&self, decision_id: &str) -> Option<StoredDecision> {
        self.decisions
            .iter()
            .find(|d| d.decision_id == decision_id)
            .cloned()
    }

    fn event(&self, event_id: &str) -> Option<StoredEvent> {
        self.events.iter().find(|e| e.event_id == event_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_versions_are_domain_scoped() {
        let mut store = MemoryEventStore::new();
        let alpha = DomainKey::new("agent:a", "repo:alpha");
        let beta = DomainKey::new("agent:b", "repo:beta");

        assert_eq!(store.current_state_version(&alpha), "state-000000");
        assert_eq!(store.advance_state_version(&alpha), "state-000001");
        assert_eq!(store.current_state_version(&beta), "state-000000");
        assert_eq!(store.advance_state_version(&beta), "state-000001");
        assert_eq!(store.current_state_version(&alpha), "state-000001");
    }

    #[test]
    fn decision_ids_are_store_wide_and_deterministic() {
        let mut store = MemoryEventStore::new();
        assert_eq!(store.next_decision_id(), "decision-000001");
        assert_eq!(store.next_decision_id(), "decision-000002");
    }
}
