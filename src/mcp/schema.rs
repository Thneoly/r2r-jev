use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ObserveParams {
    pub provider: String,
    pub subject: String,
    pub scope: String,
    pub task: String,
    pub action: String,
    pub intent: String,
    pub beyond_scope_ppm: u32,
    pub destructive_ppm: u32,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DecideParams {
    pub subject: String,
    pub scope: String,
    pub action: String,
    pub resource: Option<String>,
    pub task: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ExplainParams {
    pub decision_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Executed,
    Blocked,
    Failed,
    RolledBack,
    UserCorrected,
    PolicyBreachConfirmed,
}

impl OutcomeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
            Self::UserCorrected => "user_corrected",
            Self::PolicyBreachConfirmed => "policy_breach_confirmed",
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RecordOutcomeParams {
    pub decision_id: String,
    pub outcome: OutcomeKind,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReplayParams {
    pub subject: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdmissionSummary {
    pub evidence_id: String,
    pub kind: String,
    pub confidence_ppm: u32,
    pub admission: String,
    pub policy_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObserveResponse {
    pub event_id: String,
    pub admissions: Vec<AdmissionSummary>,
    pub relation_transitions: Vec<String>,
    pub state_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoverningRelation {
    pub relation_id: String,
    pub relation_type: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecideResponse {
    pub decision: String,
    pub reason_code: String,
    pub decision_id: String,
    pub governing_relations: Vec<GoverningRelation>,
    pub required_next_step: Option<String>,
    pub state_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplainResponse {
    pub decision_id: String,
    pub verdict: String,
    pub reason_code: String,
    pub state_version: String,
    pub governing_relation_id: Option<String>,
    pub causal_chain: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordOutcomeResponse {
    pub outcome_id: String,
    pub decision_id: String,
    pub outcome: String,
    pub state_version: String,
    pub relation_transitions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayResponse {
    pub replay_match: bool,
    pub recorded_state_version: String,
    pub replayed_state_version: String,
    pub replayed_events: usize,
    pub first_divergent_event: Option<String>,
    pub policy_versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}
