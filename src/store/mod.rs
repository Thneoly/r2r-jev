use serde::{Deserialize, Serialize};

pub mod json_file;
pub mod memory;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayObservation {
    pub provider: String,
    pub task: String,
    pub action: String,
    pub intent: String,
    pub beyond_scope_ppm: u32,
    pub destructive_ppm: u32,
    pub source_reliability_ppm: u32,
    pub independent_corroborators: u8,
    pub now_vtick: u64,
    pub expires_vtick: u64,
    pub admission_policy_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    /// Store-wide stable id exposed through MCP.
    pub event_id: String,
    /// Deterministic id produced by the domain-local R2R kernel.
    pub kernel_event_id: String,
    pub domain: DomainKey,
    pub action: String,
    pub state_version: String,
    pub relation_transitions: Vec<String>,
    pub provenance: Vec<String>,
    pub replay_observation: Option<ReplayObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredOutcome {
    pub outcome_id: String,
    pub decision_id: String,
    pub domain: DomainKey,
    pub outcome: String,
    pub detail: Option<String>,
    pub state_version: String,
}

/// Persistence boundary for the governance runtime.
///
/// State versions are scoped to a governance domain. Event, decision, and
/// outcome identifiers are store-wide so MCP resources remain unambiguous
/// when one server hosts multiple domains.
pub trait EventStore: Send {
    /// Persistent stores fail closed after an unrecoverable write failure.
    fn health(&self) -> Result<(), String> {
        Ok(())
    }

    fn current_state_version(&self, domain: &DomainKey) -> String;
    fn advance_state_version(&mut self, domain: &DomainKey) -> String;

    fn next_event_id(&mut self) -> String;
    fn record_event(&mut self, event: StoredEvent);
    fn event(&self, event_id: &str) -> Option<StoredEvent>;
    fn events_for_domain(&self, domain: &DomainKey) -> Vec<StoredEvent>;
    fn all_events(&self) -> Vec<StoredEvent>;

    fn next_decision_id(&mut self) -> String;
    fn record_decision(&mut self, decision: StoredDecision);
    fn decision(&self, decision_id: &str) -> Option<StoredDecision>;

    fn next_outcome_id(&mut self) -> String;
    fn record_outcome(&mut self, outcome: StoredOutcome);
    fn outcome(&self, outcome_id: &str) -> Option<StoredOutcome>;
}
