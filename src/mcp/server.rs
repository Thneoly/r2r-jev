use super::schema::{
    AdmissionSummary, DecideParams, DecideResponse, ErrorResponse, ExplainParams, ExplainResponse,
    GoverningRelation, ObserveParams, ObserveResponse, RecordOutcomeParams, RecordOutcomeResponse,
    ReplayParams, ReplayResponse,
};
use crate::admission::{AdmissionContext, POLICY_VERSION, PPM};
use crate::model::JudgmentObserved;
use crate::r2r::{Decision, Governance};
use crate::store::memory::MemoryEventStore;
use crate::store::{
    DomainKey, EventStore, ReplayObservation, StoredDecision, StoredEvent, StoredOutcome,
};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const RULE_PACK_VERSION: &str = "demo-0.1";

struct DomainRuntime {
    governance: Governance,
    next_vtick: u64,
    projected_decision: Decision,
    governing_authorization: Option<String>,
}

impl DomainRuntime {
    fn new() -> Self {
        Self {
            governance: Governance::new(),
            next_vtick: 1,
            projected_decision: Decision::Allow,
            governing_authorization: None,
        }
    }
}

struct GovernanceRuntime {
    domains: HashMap<DomainKey, DomainRuntime>,
    store: Box<dyn EventStore>,
}

impl GovernanceRuntime {
    fn new() -> Self {
        Self::try_with_store(Box::new(MemoryEventStore::new()))
            .expect("empty memory store recovery must succeed")
    }

    fn try_with_store(store: Box<dyn EventStore>) -> Result<Self, String> {
        store.health()?;
        let events = store.all_events();
        let mut domains: HashMap<DomainKey, DomainRuntime> = HashMap::new();
        let mut replayed_versions: HashMap<DomainKey, u64> = HashMap::new();

        for stored_event in events {
            let domain_key = stored_event.domain.clone();
            let Some(input) = stored_event.replay_observation.clone() else {
                if !stored_event.relation_transitions.is_empty() {
                    return Err(format!(
                        "cannot recover state-changing event {} without replay input",
                        stored_event.event_id
                    ));
                }
                continue;
            };

            if input.admission_policy_version != POLICY_VERSION {
                return Err(format!(
                    "cannot recover event {}: admission policy {} is unavailable (runtime {})",
                    stored_event.event_id, input.admission_policy_version, POLICY_VERSION
                ));
            }

            let domain = domains
                .entry(domain_key.clone())
                .or_insert_with(DomainRuntime::new);
            let judgment = JudgmentObserved {
                provider: input.provider,
                subject: domain_key.subject.clone(),
                scope: domain_key.scope.clone(),
                task: input.task,
                tool: input.action,
                intent: input.intent,
                beyond_scope_ppm: input.beyond_scope_ppm,
                destructive_ppm: input.destructive_ppm,
            };
            let context = AdmissionContext::new(
                input.source_reliability_ppm,
                input.independent_corroborators,
                input.now_vtick,
                input.expires_vtick,
            );
            let outcome = domain.governance.observe_judgment(&judgment, context);
            domain.projected_decision = outcome.decision;
            domain.next_vtick = domain.next_vtick.max(context.now_vtick + 1);

            for change in &outcome.relation_changes {
                if change.relation == "Authorization" {
                    domain.governing_authorization = Some(change.id.clone());
                }
            }

            let version = replayed_versions.entry(domain_key.clone()).or_insert(0);
            if !outcome.relation_changes.is_empty() {
                *version += 1;
            }
            let replayed_state_version = format!("state-{version:06}");
            let replayed_transitions: Vec<String> = outcome
                .relation_changes
                .iter()
                .map(render_relation_change)
                .collect();

            if stored_event.kernel_event_id != outcome.event_id
                || stored_event.state_version != replayed_state_version
                || stored_event.relation_transitions != replayed_transitions
            {
                return Err(format!(
                    "durable event log diverged at {}",
                    stored_event.event_id
                ));
            }
        }

        for (domain, version) in replayed_versions {
            let replayed = format!("state-{version:06}");
            let recorded = store.current_state_version(&domain);
            if replayed != recorded {
                return Err(format!(
                    "durable state version mismatch for {}/{}: recorded={}, replayed={}",
                    domain.subject, domain.scope, recorded, replayed
                ));
            }
        }

        Ok(Self { domains, store })
    }

    fn observe(&mut self, params: ObserveParams) -> Result<ObserveResponse, String> {
        if params.beyond_scope_ppm > PPM || params.destructive_ppm > PPM {
            return Err(format!("confidence_ppm must be in 0..={PPM}"));
        }

        let domain_key = DomainKey::new(params.subject.clone(), params.scope.clone());
        let judgment = JudgmentObserved {
            provider: params.provider.clone(),
            subject: params.subject.clone(),
            scope: params.scope.clone(),
            task: params.task.clone(),
            tool: params.action.clone(),
            intent: params.intent.clone(),
            beyond_scope_ppm: params.beyond_scope_ppm,
            destructive_ppm: params.destructive_ppm,
        };

        let (outcome, relation_transitions, kernel_provenance, replay_observation) = {
            let domain = self
                .domains
                .entry(domain_key.clone())
                .or_insert_with(DomainRuntime::new);

            let context = AdmissionContext::demo(domain.next_vtick);
            domain.next_vtick += 1;

            let replay_observation = ReplayObservation {
                provider: params.provider.clone(),
                task: params.task.clone(),
                action: params.action.clone(),
                intent: params.intent.clone(),
                beyond_scope_ppm: params.beyond_scope_ppm,
                destructive_ppm: params.destructive_ppm,
                source_reliability_ppm: context.source_reliability_ppm,
                independent_corroborators: context.independent_corroborators,
                now_vtick: context.now_vtick,
                expires_vtick: context.expires_vtick,
                admission_policy_version: POLICY_VERSION.to_string(),
            };

            let provenance_start = domain.governance.provenance().len();
            let outcome = domain.governance.observe_judgment(&judgment, context);
            domain.projected_decision = outcome.decision;

            for change in &outcome.relation_changes {
                if change.relation == "Authorization" {
                    domain.governing_authorization = Some(change.id.clone());
                }
            }

            let relation_transitions: Vec<String> = outcome
                .relation_changes
                .iter()
                .map(render_relation_change)
                .collect();
            let kernel_provenance = domain.governance.provenance()[provenance_start..].to_vec();
            (
                outcome,
                relation_transitions,
                kernel_provenance,
                replay_observation,
            )
        };

        let state_version = if outcome.relation_changes.is_empty() {
            self.store.current_state_version(&domain_key)
        } else {
            self.store.advance_state_version(&domain_key)
        };

        let event_id = self.store.next_event_id();
        let mut provenance = vec![format!(
            "{} [kernel_event={} domain={}/{}]",
            event_id, outcome.event_id, domain_key.subject, domain_key.scope
        )];
        provenance.extend(kernel_provenance);

        self.store.record_event(StoredEvent {
            event_id: event_id.clone(),
            kernel_event_id: outcome.event_id.clone(),
            domain: domain_key,
            action: params.action,
            state_version: state_version.clone(),
            relation_transitions: relation_transitions.clone(),
            provenance,
            replay_observation: Some(replay_observation),
        });

        let admissions = outcome
            .evidence
            .iter()
            .map(|record| AdmissionSummary {
                evidence_id: record.id.clone(),
                kind: record.kind.as_str().to_string(),
                confidence_ppm: record.confidence_ppm,
                admission: record.decision.render(),
                policy_version: record.policy_version.to_string(),
            })
            .collect();

        Ok(ObserveResponse {
            event_id,
            admissions,
            relation_transitions,
            state_version,
        })
    }

    fn decide(&mut self, params: DecideParams) -> Result<DecideResponse, String> {
        let domain_key = DomainKey::new(params.subject, params.scope);
        let (projected_decision, governing_authorization) = {
            let domain = self
                .domains
                .entry(domain_key.clone())
                .or_insert_with(DomainRuntime::new);
            (
                domain.projected_decision,
                domain.governing_authorization.clone(),
            )
        };

        let mut provenance: Vec<String> = self
            .store
            .events_for_domain(&domain_key)
            .into_iter()
            .flat_map(|event| event.provenance)
            .collect();

        let decision_id = self.store.next_decision_id();
        let state_version = self.store.current_state_version(&domain_key);
        let (verdict, reason_code, required_next_step) = match projected_decision {
            Decision::Allow => ("ALLOW", "AUTHORIZATION_ACTIVE", None),
            Decision::Deny => (
                "DENY",
                "AUTHORIZATION_SUSPENDED",
                Some("human_review".to_string()),
            ),
        };

        let governing_relations = governing_authorization
            .as_ref()
            .map(|relation_id| {
                vec![GoverningRelation {
                    relation_id: relation_id.clone(),
                    relation_type: "Authorization".to_string(),
                    state: if projected_decision == Decision::Deny {
                        "Suspended".to_string()
                    } else {
                        "Active".to_string()
                    },
                }]
            })
            .unwrap_or_default();

        provenance.push(format!(
            "{} [domain={}/{} state={}] -> {}({})",
            decision_id,
            domain_key.subject,
            domain_key.scope,
            state_version,
            verdict,
            params.action
        ));

        self.store.record_decision(StoredDecision {
            decision_id: decision_id.clone(),
            domain: domain_key,
            action: params.action,
            verdict: verdict.to_string(),
            reason_code: reason_code.to_string(),
            governing_relation_id: governing_authorization,
            state_version: state_version.clone(),
            provenance,
        });

        Ok(DecideResponse {
            decision: verdict.to_string(),
            reason_code: reason_code.to_string(),
            decision_id,
            governing_relations,
            required_next_step,
            state_version,
        })
    }

    fn explain(&self, params: ExplainParams) -> Result<ExplainResponse, String> {
        let decision = self
            .store
            .decision(&params.decision_id)
            .ok_or_else(|| format!("unknown decision_id: {}", params.decision_id))?;

        let summary = match decision.verdict.as_str() {
            "DENY" => format!(
                "{} was denied because persistent authorization state was suspended at {} for {}/{}.",
                decision.action,
                decision.state_version,
                decision.domain.subject,
                decision.domain.scope
            ),
            _ => format!(
                "{} was allowed because no suspended authorization governed the action at {} for {}/{}.",
                decision.action,
                decision.state_version,
                decision.domain.subject,
                decision.domain.scope
            ),
        };

        Ok(ExplainResponse {
            decision_id: decision.decision_id,
            verdict: decision.verdict,
            reason_code: decision.reason_code,
            state_version: decision.state_version,
            governing_relation_id: decision.governing_relation_id,
            causal_chain: decision.provenance,
            summary,
        })
    }

    fn record_outcome(
        &mut self,
        params: RecordOutcomeParams,
    ) -> Result<RecordOutcomeResponse, String> {
        let decision = self
            .store
            .decision(&params.decision_id)
            .ok_or_else(|| format!("unknown decision_id: {}", params.decision_id))?;

        let outcome_id = self.store.next_outcome_id();
        let state_version = self.store.current_state_version(&decision.domain);
        let outcome = params.outcome.as_str().to_string();

        self.store.record_outcome(StoredOutcome {
            outcome_id: outcome_id.clone(),
            decision_id: decision.decision_id.clone(),
            domain: decision.domain,
            outcome: outcome.clone(),
            detail: params.detail,
            state_version: state_version.clone(),
        });

        Ok(RecordOutcomeResponse {
            outcome_id,
            decision_id: decision.decision_id,
            outcome,
            state_version,
            relation_transitions: Vec::new(),
        })
    }

    fn replay(&self, params: ReplayParams) -> Result<ReplayResponse, String> {
        let domain_key = DomainKey::new(params.subject, params.scope);
        let events = self.store.events_for_domain(&domain_key);
        let recorded_state_version = self.store.current_state_version(&domain_key);

        let mut governance = Governance::new();
        let mut replayed_state_counter = 0_u64;
        let mut replayed_events = 0_usize;
        let mut first_divergent_event = None;

        for stored_event in events {
            let Some(input) = stored_event.replay_observation else {
                continue;
            };
            replayed_events += 1;

            if input.admission_policy_version != POLICY_VERSION {
                first_divergent_event.get_or_insert(stored_event.event_id);
                break;
            }

            let judgment = JudgmentObserved {
                provider: input.provider,
                subject: domain_key.subject.clone(),
                scope: domain_key.scope.clone(),
                task: input.task,
                tool: input.action,
                intent: input.intent,
                beyond_scope_ppm: input.beyond_scope_ppm,
                destructive_ppm: input.destructive_ppm,
            };
            let context = AdmissionContext::new(
                input.source_reliability_ppm,
                input.independent_corroborators,
                input.now_vtick,
                input.expires_vtick,
            );
            let outcome = governance.observe_judgment(&judgment, context);
            if !outcome.relation_changes.is_empty() {
                replayed_state_counter += 1;
            }

            let replayed_state_version = format!("state-{replayed_state_counter:06}");
            let replayed_transitions: Vec<String> = outcome
                .relation_changes
                .iter()
                .map(render_relation_change)
                .collect();

            if stored_event.kernel_event_id != outcome.event_id
                || stored_event.state_version != replayed_state_version
                || stored_event.relation_transitions != replayed_transitions
            {
                first_divergent_event.get_or_insert(stored_event.event_id);
            }
        }

        let replayed_state_version = format!("state-{replayed_state_counter:06}");
        let replay_match =
            first_divergent_event.is_none() && recorded_state_version == replayed_state_version;

        Ok(ReplayResponse {
            replay_match,
            recorded_state_version,
            replayed_state_version,
            replayed_events,
            first_divergent_event,
            policy_versions: vec![
                format!("admission:{POLICY_VERSION}"),
                format!("r2r-rules:{RULE_PACK_VERSION}"),
            ],
        })
    }
}

fn render_relation_change(change: &crate::r2r::RelationChange) -> String {
    format!("{}:{}:{}", change.id, change.relation, change.transition)
}

#[derive(Clone)]
pub struct R2rMcpServer {
    runtime: Arc<Mutex<GovernanceRuntime>>,
}

impl R2rMcpServer {
    pub fn new() -> Self {
        Self::try_with_store(Box::new(MemoryEventStore::new()))
            .expect("empty memory store recovery must succeed")
    }

    pub fn with_store(store: Box<dyn EventStore>) -> Self {
        Self::try_with_store(store).expect("event store recovery failed")
    }

    pub fn try_with_store(store: Box<dyn EventStore>) -> Result<Self, String> {
        Ok(Self {
            runtime: Arc::new(Mutex::new(GovernanceRuntime::try_with_store(store)?)),
        })
    }

    fn with_runtime<T: Serialize>(
        &self,
        f: impl FnOnce(&mut GovernanceRuntime) -> Result<T, String>,
    ) -> String {
        match self.runtime.lock() {
            Ok(mut runtime) => {
                if let Err(error) = runtime.store.health() {
                    return error_json(format!("event store unavailable: {error}"));
                }

                let result = f(&mut runtime);

                if let Err(error) = runtime.store.health() {
                    return error_json(format!("event store persistence failed: {error}"));
                }

                match result {
                    Ok(value) => serde_json::to_string_pretty(&value)
                        .unwrap_or_else(|e| error_json(format!("serialization error: {e}"))),
                    Err(error) => error_json(error),
                }
            }
            Err(_) => error_json("governance runtime lock poisoned".to_string()),
        }
    }
}

impl Default for R2rMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(server_handler)]
impl R2rMcpServer {
    #[tool(
        description = "Submit an untrusted probabilistic observation. The server binds trusted admission metadata, runs Evidence Admission, and applies only admitted evidence to the subject/scope R2R governance domain."
    )]
    fn r2r_observe(&self, Parameters(params): Parameters<ObserveParams>) -> String {
        self.with_runtime(|runtime| runtime.observe(params))
    }

    #[tool(
        description = "Evaluate a proposed action against the persistent R2R governance state for its subject/scope domain without mutating relation state."
    )]
    fn r2r_decide(&self, Parameters(params): Parameters<DecideParams>) -> String {
        self.with_runtime(|runtime| runtime.decide(params))
    }

    #[tool(
        description = "Explain a previously issued R2R decision using its recorded domain, state version, and causal provenance chain."
    )]
    fn r2r_explain(&self, Parameters(params): Parameters<ExplainParams>) -> String {
        self.with_runtime(|runtime| runtime.explain(params))
    }

    #[tool(
        description = "Record the observed result of a governed action. Outcome data is untrusted audit input in v0.1 and cannot directly mutate relation state."
    )]
    fn r2r_record_outcome(&self, Parameters(params): Parameters<RecordOutcomeParams>) -> String {
        self.with_runtime(|runtime| runtime.record_outcome(params))
    }

    #[tool(
        description = "Deterministically replay all stored observation events for a subject/scope domain using their recorded trusted Admission context and report the first divergence."
    )]
    fn r2r_replay(&self, Parameters(params): Parameters<ReplayParams>) -> String {
        self.with_runtime(|runtime| runtime.replay(params))
    }
}

fn error_json(error: String) -> String {
    serde_json::to_string_pretty(&ErrorResponse { error })
        .unwrap_or_else(|_| "{\"error\":\"serialization failure\"}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::schema::OutcomeKind;

    fn risky_observation(subject: &str, scope: &str) -> ObserveParams {
        ObserveParams {
            provider: "fixture:jev-style".to_string(),
            subject: subject.to_string(),
            scope: scope.to_string(),
            task: "fix login redirect".to_string(),
            action: "github.merge_pull_request".to_string(),
            intent: "merge unrelated changes".to_string(),
            beyond_scope_ppm: 940_000,
            destructive_ppm: 720_000,
        }
    }

    fn decide(subject: &str, scope: &str) -> DecideParams {
        DecideParams {
            subject: subject.to_string(),
            scope: scope.to_string(),
            action: "github.merge_pull_request".to_string(),
            resource: Some("pr:42".to_string()),
            task: Some("fix login redirect".to_string()),
        }
    }

    #[test]
    fn observe_then_decide_then_explain_is_causal() {
        let mut runtime = GovernanceRuntime::new();
        let observed = runtime
            .observe(risky_observation("agent:coder-1", "repo:alpha"))
            .expect("observe");
        assert_eq!(observed.event_id, "event-000001");
        assert_eq!(observed.state_version, "state-000001");
        assert!(observed
            .relation_transitions
            .iter()
            .any(|change| change.contains("Authorization:Active -> Suspended")));

        let decision = runtime
            .decide(decide("agent:coder-1", "repo:alpha"))
            .expect("decide");
        assert_eq!(decision.decision, "DENY");
        assert_eq!(decision.state_version, "state-000001");

        let explanation = runtime
            .explain(ExplainParams {
                decision_id: decision.decision_id,
            })
            .expect("explain");
        assert_eq!(explanation.verdict, "DENY");
        assert!(explanation
            .causal_chain
            .iter()
            .any(|line| line.contains("event-000001")));
        assert!(explanation
            .causal_chain
            .iter()
            .any(|line| line.contains("authorization-0001")));
    }

    #[test]
    fn rejected_observation_does_not_advance_domain_state_version() {
        let mut runtime = GovernanceRuntime::new();
        let first = runtime
            .observe(risky_observation("agent:coder-1", "repo:alpha"))
            .expect("observe risky");
        assert_eq!(first.state_version, "state-000001");

        let benign = runtime
            .observe(ObserveParams {
                provider: "fixture:jev-style".to_string(),
                subject: "agent:coder-1".to_string(),
                scope: "repo:alpha".to_string(),
                task: "fix login redirect".to_string(),
                action: "read_file".to_string(),
                intent: "read source".to_string(),
                beyond_scope_ppm: 180_000,
                destructive_ppm: 120_000,
            })
            .expect("observe benign");

        assert_eq!(benign.state_version, "state-000001");
        assert!(benign.relation_transitions.is_empty());
    }

    #[test]
    fn domains_are_isolated_and_public_event_ids_remain_unique() {
        let mut runtime = GovernanceRuntime::new();
        let alpha_event = runtime
            .observe(risky_observation("agent:coder-1", "repo:alpha"))
            .expect("observe alpha");
        let beta_event = runtime
            .observe(risky_observation("agent:coder-2", "repo:beta"))
            .expect("observe beta");

        assert_eq!(alpha_event.event_id, "event-000001");
        assert_eq!(beta_event.event_id, "event-000002");
        assert_eq!(alpha_event.state_version, "state-000001");
        assert_eq!(beta_event.state_version, "state-000001");

        let alpha = runtime
            .decide(decide("agent:coder-1", "repo:alpha"))
            .expect("decide alpha");
        let fresh = runtime
            .decide(decide("agent:coder-3", "repo:gamma"))
            .expect("decide fresh");
        assert_eq!(alpha.decision, "DENY");
        assert_eq!(fresh.decision, "ALLOW");
        assert_eq!(fresh.state_version, "state-000000");
    }

    #[test]
    fn outcome_is_linked_to_decision_without_direct_relation_mutation() {
        let mut runtime = GovernanceRuntime::new();
        runtime
            .observe(risky_observation("agent:coder-1", "repo:alpha"))
            .expect("observe");
        let decision = runtime
            .decide(decide("agent:coder-1", "repo:alpha"))
            .expect("decide");

        let outcome = runtime
            .record_outcome(RecordOutcomeParams {
                decision_id: decision.decision_id,
                outcome: OutcomeKind::Blocked,
                detail: Some("enforcement adapter blocked execution".to_string()),
            })
            .expect("record outcome");

        assert_eq!(outcome.outcome_id, "outcome-000001");
        assert_eq!(outcome.outcome, "blocked");
        assert_eq!(outcome.state_version, "state-000001");
        assert!(outcome.relation_transitions.is_empty());
    }

    #[test]
    fn replay_reproduces_domain_state_and_transitions() {
        let mut runtime = GovernanceRuntime::new();
        runtime
            .observe(risky_observation("agent:coder-1", "repo:alpha"))
            .expect("observe risky");
        runtime
            .observe(ObserveParams {
                provider: "fixture:jev-style".to_string(),
                subject: "agent:coder-1".to_string(),
                scope: "repo:alpha".to_string(),
                task: "fix login redirect".to_string(),
                action: "read_file".to_string(),
                intent: "read source".to_string(),
                beyond_scope_ppm: 180_000,
                destructive_ppm: 120_000,
            })
            .expect("observe benign");

        let replay = runtime
            .replay(ReplayParams {
                subject: "agent:coder-1".to_string(),
                scope: "repo:alpha".to_string(),
            })
            .expect("replay");

        assert!(replay.replay_match);
        assert_eq!(replay.recorded_state_version, "state-000001");
        assert_eq!(replay.replayed_state_version, "state-000001");
        assert_eq!(replay.replayed_events, 2);
        assert!(replay.first_divergent_event.is_none());
    }
}
