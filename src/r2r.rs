use crate::admission::{admit, AdmissionContext, AdmissionDecision, EvidenceKind, POLICY_VERSION};
use crate::model::JudgmentObserved;

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

#[derive(Debug)]
pub struct EvidenceAdmissionRecord {
    pub id: String,
    pub kind: EvidenceKind,
    pub confidence_ppm: u32,
    pub decision: AdmissionDecision,
    pub policy_version: &'static str,
}

/// One governance event and everything it changed after evidence admission.
#[derive(Debug)]
pub struct Admission {
    pub event_id: String,
    pub evidence: Vec<EvidenceAdmissionRecord>,
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

/// Minimal demo governance kernel.
///
/// Probabilistic judgments do not mutate relation state. Each typed evidence
/// candidate first passes Evidence Admission v0.1. Only `Accept` becomes a
/// governance-active input to the deterministic demo R2R rule pack.
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

    /// Observe a probabilistic judgment, turn it into typed evidence candidates,
    /// run Admission v0.1, then let only accepted evidence reach R2R rules.
    pub fn observe_judgment(
        &mut self,
        judgment: &JudgmentObserved,
        context: AdmissionContext,
    ) -> Admission {
        let event_id = self.counters.next("event");
        let candidates = [
            (EvidenceKind::BeyondScope, judgment.beyond_scope_ppm),
            (EvidenceKind::DestructiveAction, judgment.destructive_ppm),
        ];

        let mut evidence = Vec::with_capacity(candidates.len());
        let mut all_changes = Vec::new();
        let mut accepted_any = false;

        for (kind, confidence_ppm) in candidates {
            let evidence_id = self.counters.next("evidence");
            let admission_decision = admit(kind, confidence_ppm, context);
            let mut local_changes = Vec::new();

            if admission_decision.is_accepted() {
                accepted_any = true;
                local_changes = self.apply_admitted_evidence(&evidence_id, kind);
                all_changes.extend(local_changes.iter().map(|change| RelationChange {
                    id: change.id.clone(),
                    relation: change.relation,
                    transition: change.transition.clone(),
                    caused_by: change.caused_by.clone(),
                }));
            }

            let mut line = format!(
                "{event_id} -> {evidence_id}[{}:{}]",
                kind.as_str(),
                admission_decision.render()
            );
            for change in &local_changes {
                line.push_str(" -> ");
                line.push_str(&change.id);
            }
            self.provenance.push(line);

            evidence.push(EvidenceAdmissionRecord {
                id: evidence_id,
                kind,
                confidence_ppm,
                decision: admission_decision,
                policy_version: POLICY_VERSION,
            });
        }

        let decision = if self.state.authorization == AuthorizationPhase::Active {
            Decision::Allow
        } else {
            Decision::Deny
        };

        let reason = if accepted_any && !all_changes.is_empty() {
            "accepted evidence changed persistent governance state"
        } else if accepted_any && self.state.authorization == AuthorizationPhase::Suspended {
            "evidence admitted; authorization remains suspended"
        } else if self.state.authorization == AuthorizationPhase::Suspended {
            "no evidence admitted; authorization remains suspended by earlier evidence"
        } else {
            "no evidence admitted; authorization remains active"
        };

        let mut decision_line = event_id.clone();
        if let Some(governing) = &self.governing_authorization {
            decision_line.push_str(&format!(" [via {governing}]"));
        }
        decision_line.push_str(&format!(" -> {}({})", decision.as_str(), judgment.tool));
        self.provenance.push(decision_line);

        Admission {
            event_id,
            evidence,
            reason,
            relation_changes: all_changes,
            decision,
            action: judgment.tool.clone(),
        }
    }

    /// The demo R2R rule pack. Admission answers whether evidence may enter
    /// governance; this method answers how accepted evidence affects the current
    /// Relation Graph. v0.1 maps accepted BeyondScope/DestructiveAction evidence
    /// to the same protective chain for demonstration purposes.
    fn apply_admitted_evidence(
        &mut self,
        evidence_id: &str,
        kind: EvidenceKind,
    ) -> Vec<RelationChange> {
        debug_assert!(kind.is_supported());
        let mut changes = Vec::new();

        if self.state.trust != TrustPhase::Warning {
            self.state.trust = TrustPhase::Warning;
            changes.push(RelationChange::new(
                &mut self.counters,
                "trust",
                "Trust",
                "Active -> Warning",
                evidence_id,
            ));
        }
        if self.state.delegation != DelegationPhase::Degraded {
            self.state.delegation = DelegationPhase::Degraded;
            changes.push(RelationChange::new(
                &mut self.counters,
                "delegation",
                "Delegation",
                "Active -> Degraded",
                evidence_id,
            ));
        }
        if self.state.authorization != AuthorizationPhase::Suspended {
            self.state.authorization = AuthorizationPhase::Suspended;
            let change = RelationChange::new(
                &mut self.counters,
                "authorization",
                "Authorization",
                "Active -> Suspended",
                evidence_id,
            );
            self.governing_authorization = Some(change.id.clone());
            changes.push(change);
        }

        changes
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
            evidence: Vec::new(),
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
    use crate::admission::{AdmissionClass, HoldReason, RejectReason};

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
    fn act1_admission_precedes_relation_transition() {
        let mut governance = Governance::new();
        let outcome = governance.observe_judgment(
            &judgment(0.94, 0.72, "merge_pull_request"),
            AdmissionContext::demo(1),
        );

        assert_eq!(
            outcome.evidence[0].decision,
            AdmissionDecision::Accept(AdmissionClass::Strong)
        );
        assert_eq!(
            outcome.evidence[1].decision,
            AdmissionDecision::Hold(HoldReason::NeedsCorroborationOrReview)
        );
        assert_eq!(outcome.decision, Decision::Deny);
        assert_eq!(governance.state.trust, TrustPhase::Warning);
        assert_eq!(governance.state.delegation, DelegationPhase::Degraded);
        assert_eq!(
            governance.state.authorization,
            AuthorizationPhase::Suspended
        );
        assert_eq!(outcome.relation_changes.len(), 3);
    }

    #[test]
    fn act2_low_signals_are_rejected_but_prior_suspension_persists() {
        let mut governance = Governance::new();
        governance.observe_judgment(
            &judgment(0.94, 0.72, "merge_pull_request"),
            AdmissionContext::demo(1),
        );

        let outcome = governance.observe_judgment(
            &judgment(0.18, 0.12, "read_file"),
            AdmissionContext::demo(2),
        );

        assert!(outcome.evidence.iter().all(|record| matches!(
            record.decision,
            AdmissionDecision::Reject(RejectReason::InsufficientSupport)
        )));
        assert_eq!(outcome.decision, Decision::Deny);
        assert_eq!(
            governance.state.authorization,
            AuthorizationPhase::Suspended
        );
        assert!(outcome.relation_changes.is_empty());
    }

    #[test]
    fn high_confidence_from_low_reliability_source_does_not_mutate_relations() {
        let mut governance = Governance::new();
        let context = AdmissionContext::new(400_000, 0, 1, 11);
        let outcome =
            governance.observe_judgment(&judgment(0.99, 0.99, "merge_pull_request"), context);

        assert!(outcome.evidence.iter().all(|record| matches!(
            record.decision,
            AdmissionDecision::Reject(RejectReason::SourceBelowTrustFloor)
        )));
        assert_eq!(outcome.decision, Decision::Allow);
        assert_eq!(governance.state.authorization, AuthorizationPhase::Active);
        assert!(outcome.relation_changes.is_empty());
    }

    #[test]
    fn medium_single_source_judgments_hold_without_mutation() {
        let mut governance = Governance::new();
        let context = AdmissionContext::new(800_000, 0, 1, 11);
        let outcome =
            governance.observe_judgment(&judgment(0.76, 0.76, "merge_pull_request"), context);

        assert!(outcome.evidence.iter().all(|record| matches!(
            record.decision,
            AdmissionDecision::Hold(HoldReason::NeedsCorroborationOrReview)
        )));
        assert_eq!(outcome.decision, Decision::Allow);
        assert!(outcome.relation_changes.is_empty());
    }

    #[test]
    fn corroborated_medium_evidence_can_enter_r2r() {
        let mut governance = Governance::new();
        let context = AdmissionContext::new(800_000, 2, 1, 11);
        let outcome =
            governance.observe_judgment(&judgment(0.76, 0.20, "merge_pull_request"), context);

        assert_eq!(
            outcome.evidence[0].decision,
            AdmissionDecision::Accept(AdmissionClass::Corroborated)
        );
        assert_eq!(outcome.decision, Decision::Deny);
        assert_eq!(
            governance.state.authorization,
            AuthorizationPhase::Suspended
        );
    }

    #[test]
    fn act3_override_restores_under_supervision() {
        let mut governance = Governance::new();
        governance.observe_judgment(
            &judgment(0.94, 0.72, "merge_pull_request"),
            AdmissionContext::demo(1),
        );

        let outcome = governance.human_override("human-1", "merge_pull_request");

        assert_eq!(outcome.decision, Decision::Allow);
        assert_eq!(governance.state.authorization, AuthorizationPhase::Active);
        let supervision = governance.state.supervision.as_ref().expect("supervision");
        assert_eq!(supervision.supervisor, "human-1");
        assert_eq!(supervision.created_by, outcome.event_id);
    }

    #[test]
    fn benign_judgment_on_fresh_state_allows() {
        let mut governance = Governance::new();
        let outcome = governance.observe_judgment(
            &judgment(0.20, 0.10, "read_file"),
            AdmissionContext::demo(1),
        );

        assert_eq!(outcome.decision, Decision::Allow);
        assert_eq!(governance.state.authorization, AuthorizationPhase::Active);
        assert!(outcome.relation_changes.is_empty());
    }

    #[test]
    fn provenance_is_deterministic() {
        let run = || {
            let mut governance = Governance::new();
            governance.observe_judgment(
                &judgment(0.94, 0.72, "merge_pull_request"),
                AdmissionContext::demo(1),
            );
            governance.observe_judgment(
                &judgment(0.18, 0.12, "read_file"),
                AdmissionContext::demo(2),
            );
            governance.human_override("human-1", "merge_pull_request");
            governance.provenance().to_vec()
        };

        assert_eq!(run(), run());
        assert!(run()
            .first()
            .expect("first provenance line")
            .contains("Accept(Strong)"));
    }
}
