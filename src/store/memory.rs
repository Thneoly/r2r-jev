use super::{DomainKey, EventStore, StoredDecision, StoredEvent, StoredOutcome};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DomainStateVersion {
    domain: DomainKey,
    version: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MemoryEventStore {
    events: Vec<StoredEvent>,
    decisions: Vec<StoredDecision>,
    outcomes: Vec<StoredOutcome>,
    state_versions: Vec<DomainStateVersion>,
    event_counter: u64,
    decision_counter: u64,
    outcome_counter: u64,
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

    pub fn outcome_count(&self) -> usize {
        self.outcomes.len()
    }

    fn state_counter(&self, domain: &DomainKey) -> u64 {
        self.state_versions
            .iter()
            .find(|entry| &entry.domain == domain)
            .map(|entry| entry.version)
            .unwrap_or(0)
    }
}

impl EventStore for MemoryEventStore {
    fn current_state_version(&self, domain: &DomainKey) -> String {
        format!("state-{:06}", self.state_counter(domain))
    }

    fn advance_state_version(&mut self, domain: &DomainKey) -> String {
        if let Some(entry) = self
            .state_versions
            .iter_mut()
            .find(|entry| &entry.domain == domain)
        {
            entry.version += 1;
            return format!("state-{:06}", entry.version);
        }

        self.state_versions.push(DomainStateVersion {
            domain: domain.clone(),
            version: 1,
        });
        "state-000001".to_string()
    }

    fn next_event_id(&mut self) -> String {
        self.event_counter += 1;
        format!("event-{:06}", self.event_counter)
    }

    fn record_event(&mut self, event: StoredEvent) {
        self.events.push(event);
    }

    fn event(&self, event_id: &str) -> Option<StoredEvent> {
        self.events.iter().find(|e| e.event_id == event_id).cloned()
    }

    fn events_for_domain(&self, domain: &DomainKey) -> Vec<StoredEvent> {
        self.events
            .iter()
            .filter(|event| &event.domain == domain)
            .cloned()
            .collect()
    }

    fn all_events(&self) -> Vec<StoredEvent> {
        self.events.clone()
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

    fn next_outcome_id(&mut self) -> String {
        self.outcome_counter += 1;
        format!("outcome-{:06}", self.outcome_counter)
    }

    fn record_outcome(&mut self, outcome: StoredOutcome) {
        self.outcomes.push(outcome);
    }

    fn outcome(&self, outcome_id: &str) -> Option<StoredOutcome> {
        self.outcomes
            .iter()
            .find(|o| o.outcome_id == outcome_id)
            .cloned()
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
    fn snapshot_round_trip_preserves_domain_versions_and_counters() {
        let mut store = MemoryEventStore::new();
        let alpha = DomainKey::new("agent:a", "repo:alpha");
        assert_eq!(store.advance_state_version(&alpha), "state-000001");
        assert_eq!(store.next_event_id(), "event-000001");
        assert_eq!(store.next_decision_id(), "decision-000001");
        assert_eq!(store.next_outcome_id(), "outcome-000001");

        let encoded = serde_json::to_string(&store).expect("serialize");
        let mut recovered: MemoryEventStore = serde_json::from_str(&encoded).expect("deserialize");

        assert_eq!(recovered.current_state_version(&alpha), "state-000001");
        assert_eq!(recovered.next_event_id(), "event-000002");
        assert_eq!(recovered.next_decision_id(), "decision-000002");
        assert_eq!(recovered.next_outcome_id(), "outcome-000002");
    }

    #[test]
    fn public_ids_are_store_wide_and_deterministic() {
        let mut store = MemoryEventStore::new();
        assert_eq!(store.next_event_id(), "event-000001");
        assert_eq!(store.next_event_id(), "event-000002");
        assert_eq!(store.next_decision_id(), "decision-000001");
        assert_eq!(store.next_decision_id(), "decision-000002");
        assert_eq!(store.next_outcome_id(), "outcome-000001");
        assert_eq!(store.next_outcome_id(), "outcome-000002");
    }
}
