use crate::model::JudgmentObserved;

const BEYOND_SCOPE_THRESHOLD_PPM: u32 = 850_000;
const DESTRUCTIVE_THRESHOLD_PPM: u32 = 900_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPhase {
    Active,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationPhase {
    Active,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationPhase {
    Active,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supervision {
    pub supervisor: String,
    pub created_by: String,
}

#[derive(Debug)]
pub struct GovernanceState {
    pub trust: TrustPhase,
    pub delegation: DelegationPhase,
    pub authorization: AuthorizationPhase,
    pub supervision: Option<Supervision>,
}

impl Default for GovernanceState {
    fn default() -> Self {
        Self {
            trust: TrustPhase::Active,
            delegation: DelegationPhase::Active,
            authorization: AuthorizationPhase::Active,
            supervision: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "ALLOW",
            Self::Deny => "DENY",
        }
    }
}

/// One admitted governance event and everything it changed.
#[derive(Debug)]
pub struct Admission {
    pub event_id: String,
    pub evidence_id: Option<String>,
    pub reason: &'static str,
    pub relation_changes: Vec<RelationChange>,
    pub decision: Decision,
    pub action: String,
}

#[derive(Debug)]
pub struct RelationChange {
    pub id: String,
    pub relation: &'static str,
    pub transition: String,
    pub caused_by: String,
}

impl RelationChange {
    fn new(
        counters: &mut Counters,
        kind: &'static str,
        relation: &'static str,
        transition: &str,
        caused_by: &str,
    ) -> Self {
        Self {
            id: counters.next(kind),
            relation,
            transition: transition.to_string(),
            caused_by: caused_by.to_string(),
        }
    }
}

#[derive(Default)]
struct Counters {
    event: usize,
    evidence: usize,
    trust: usize,
    delegation: usize,
    authorization: usize,
    supervision: usize,
}

impl Counters {
    /// Deterministic, sequential ids: ev-0001, evidence-0002, trust-0001, ...
    /// No clocks, no randomness: the same event sequence always produces
    /// the same ids and the same provenance lines.
    fn next(&mut self, kind: &'static str) -> String {
        let n = match kind {
            "event" => {
                self.event += 1;
                self.event
            }
            "evidence" => {
                self.evidence += 1;
                self.evidence
            }
            "trust" => {
                self.trust += 1;
                self.trust
            }
            "delegation" => {
                self.delegation += 1;
                self.delegation
            }
            "authorization" => {
                self.authorization += 1;
                self.authorization
            }
            "supervision" => {
                self.supervision += 1;
                self.supervision
            }
            _ => unreachable!("unknown id kind: {kind}"),
        };
        let prefix = match kind {
            "event" => "ev",
            other => other,
        };
        format!("{prefix}-{n:04}")
    }
}

/// Minimal demo governance kernel: integer judgments in, admitted relation
/// transitions and a causal provenance chain out.
///
/// This is intentionally not the full R2R runtime; it exists to make the
/// boundary "judgment -> evidence -> relation state -> future decisions"
/// runnable and inspectable.
pub struct Governance {
    state: GovernanceState,
    counters: Counters,
    provenance: Vec<String>,
    /// Id of the authorization relation instance currently in force.
    governing_authorization: Option<String>,
}

impl Governance {
    pub fn new() -> Self {
        Self {
            state: GovernanceState::default(),
            counters: Counters::default(),
            provenance: Vec::new(),
            governing_authorization: None,
        }
    }

    pub fn provenance(&self) -> &[String] {
        &self.provenance
    }

    /// Admit a judgment as evidence and let it act on persistent relations.
    pub fn observe_judgment(&mut self, judgment: &JudgmentObserved) -> Admission {
        let event_id = self.counters.next("event");
        let evidence_id = self.counters.next("evidence");

        let risky = judgment.beyond_scope_ppm >= BEYOND_SCOPE_THRESHOLD_PPM
            || judgment.destructive_ppm >= DESTRUCTIVE_THRESHOLD_PPM;

        let mut changes = Vec::new();
        if risky {
            if self.state.trust != TrustPhase::Warning {
                self.state.trust = TrustPhase::Warning;
                changes.push(RelationChange::new(
                    &mut self.counters,
                    "trust",
                    "Trust",
                    "Active -> Warning",
                    &evidence_id,
                ));
            }
            if self.state.delegation != DelegationPhase::Degraded {
                self.state.delegation = DelegationPhase::Degraded;
                changes.push(RelationChange::new(
                    &mut self.counters,
                    "delegation",
                    "Delegation",
                    "Active -> Degraded",
                    &evidence_id,
                ));
            }
            if self.state.authorization != AuthorizationPhase::Suspended {
                self.state.authorization = AuthorizationPhase::Suspended;
                let change = RelationChange::new(
                    &mut self.counters,
                    "authorization",
                    "Authorization",
                    "Active -> Suspended",
                    &evidence_id,
                );
                self.governing_authorization = Some(change.id.clone());
                changes.push(change);
            }
        }

        let decision = if self.state.authorization == AuthorizationPhase::Active {
            Decision::Allow
        } else {
            Decision::Deny
        };

        let reason = if risky {
            "threshold-crossing judgment admitted as evidence"
        } else if self.state.authorization == AuthorizationPhase::Suspended {
            "authorization remains suspended by earlier evidence"
        } else {
            "judgment below threshold"
        };

        let mut line = format!("{event_id} -> {evidence_id}");
        for change in &changes {
            line.push_str(" -> ");
            line.push_str(&change.id);
        }
        if decision == Decision::Deny {
            if let Some(governing) = &self.governing_authorization {
                let already_listed = changes.iter().any(|change| &change.id == governing);
                if !already_listed {
                    line.push_str(&format!(" [via {governing}]"));
                }
            }
            line.push_str(&format!(" -> DENY({})", judgment.tool));
        }
        self.provenance.push(line);

        Admission {
            event_id,
            evidence_id: Some(evidence_id),
            reason,
            relation_changes: changes,
            decision,
            action: judgment.tool.clone(),
        }
    }

    /// A human repairs the suspended authorization; the repair is itself
    /// recorded as a governance event and creates a supervision relation.
    pub fn human_override(&mut self, supervisor: &str, action: &str) -> Admission {
        let event_id = self.counters.next("event");

        let mut changes = Vec::new();
        if self.state.authorization != AuthorizationPhase::Active {
            self.state.authorization = AuthorizationPhase::Active;
            let change = RelationChange::new(
                &mut self.counters,
                "authorization",
                "Authorization",
                "Suspended -> Active",
                &event_id,
            );
            self.governing_authorization = Some(change.id.clone());
            changes.push(change);
        }

        if self.state.supervision.is_none() {
            let change = RelationChange::new(
                &mut self.counters,
                "supervision",
                "Supervision",
                "created",
                &event_id,
            );
            self.state.supervision = Some(Supervision {
                supervisor: supervisor.to_string(),
                created_by: event_id.clone(),
            });
            changes.push(change);
        }

        let mut line = event_id.clone();
        for change in &changes {
            line.push_str(" -> ");
            line.push_str(&change.id);
        }
        line.push_str(&format!(" -> ALLOW({action})"));
        self.provenance.push(line);

        Admission {
            event_id,
            evidence_id: None,
            reason: "human override restores authorization under supervision",
            relation_changes: changes,
            decision: Decision::Allow,
            action: action.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn judgment(beyond_scope: f64, destructive: f64, tool: &str) -> JudgmentObserved {
        JudgmentObserved::from_probabilities(
            "fixture",
            "coder",
            "repo-alpha",
            "fix login redirect",
            tool,
            "demo intent",
            beyond_scope,
            destructive,
        )
    }

    #[test]
    fn act1_risky_judgment_suspends_and_denies() {
        let mut governance = Governance::new();
        let admission = governance.observe_judgment(&judgment(0.94, 0.72, "merge_pull_request"));

        assert_eq!(admission.decision, Decision::Deny);
        assert_eq!(governance.state.trust, TrustPhase::Warning);
        assert_eq!(governance.state.delegation, DelegationPhase::Degraded);
        assert_eq!(
            governance.state.authorization,
            AuthorizationPhase::Suspended
        );
        assert_eq!(admission.relation_changes.len(), 3);
    }

    #[test]
    fn act2_benign_call_inherits_suspension() {
        let mut governance = Governance::new();
        governance.observe_judgment(&judgment(0.94, 0.72, "merge_pull_request"));

        // A later, low-scoring call is still denied: history persists.
        let admission = governance.observe_judgment(&judgment(0.18, 0.12, "read_file"));

        assert_eq!(admission.decision, Decision::Deny);
        assert_eq!(
            governance.state.authorization,
            AuthorizationPhase::Suspended
        );
        assert!(admission.relation_changes.is_empty());
    }

    #[test]
    fn act3_override_restores_under_supervision() {
        let mut governance = Governance::new();
        governance.observe_judgment(&judgment(0.94, 0.72, "merge_pull_request"));

        let admission = governance.human_override("human-1", "merge_pull_request");

        assert_eq!(admission.decision, Decision::Allow);
        assert_eq!(governance.state.authorization, AuthorizationPhase::Active);
        let supervision = governance.state.supervision.as_ref().expect("supervision");
        assert_eq!(supervision.supervisor, "human-1");
        assert_eq!(supervision.created_by, admission.event_id);
    }

    #[test]
    fn benign_judgment_on_fresh_state_allows() {
        let mut governance = Governance::new();
        let admission = governance.observe_judgment(&judgment(0.20, 0.10, "read_file"));

        assert_eq!(admission.decision, Decision::Allow);
        assert_eq!(governance.state.authorization, AuthorizationPhase::Active);
    }

    #[test]
    fn provenance_is_deterministic() {
        let run = || {
            let mut governance = Governance::new();
            governance.observe_judgment(&judgment(0.94, 0.72, "merge_pull_request"));
            governance.observe_judgment(&judgment(0.18, 0.12, "read_file"));
            governance.human_override("human-1", "merge_pull_request");
            governance.provenance().to_vec()
        };

        assert_eq!(run(), run());
        assert_eq!(
            run().first().map(String::as_str),
            Some("ev-0001 -> evidence-0001 -> trust-0001 -> delegation-0001 -> authorization-0001 -> DENY(merge_pull_request)")
        );
    }
}
