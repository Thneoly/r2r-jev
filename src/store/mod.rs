use serde::Serialize;

pub mod memory;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DomainKey {
    pub subject: String,
    pub scope: String,
}

impl DomainKey {
    pub fn new(subject: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            scope: scope.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredEvent {
    pub event_id: String,
    pub domain: DomainKey,
    pub action: String,
    pub state_version: String,
    pub relation_transitions: Vec<String>,
    pub provenance: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredDecision {
    pub decision_id: String,
    pub domain: DomainKey,
    pub action: String,
    pub verdict: String,
    pub reason_code: String,
    pub governing_relation_id: Option<String>,
    pub state_version: String,
    pub provenance: Vec<String>,
}

/// Persistence boundary for the governance runtime.
///
/// State versions are scoped to a governance domain. Decision identifiers are
/// store-wide so they remain directly addressable by `r2r_explain`.
pub trait EventStore: Send {
    fn current_state_version(&self, domain: &DomainKey) -> String;
    fn advance_state_version(&mut self, domain: &DomainKey) -> String;
    fn record_event(&mut self, event: StoredEvent);
    fn next_decision_id(&mut self) -> String;
    fn record_decision(&mut self, decision: StoredDecision);
    fn decision(&self, decision_id: &str) -> Option<StoredDecision>;
    fn event(&self, event_id: &str) -> Option<StoredEvent>;
}
