use super::schema::{
    AdmissionSummary, DecideParams, DecideResponse, ErrorResponse, ExplainParams, ExplainResponse,
    GoverningRelation, ObserveParams, ObserveResponse,
};
use crate::admission::{AdmissionContext, PPM};
use crate::model::JudgmentObserved;
use crate::r2r::{Decision, Governance};
use crate::store::memory::MemoryEventStore;
use crate::store::{DomainKey, EventStore, StoredDecision, StoredEvent};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
        Self::with_store(Box::new(MemoryEventStore::new()))
    }

    fn with_store(store: Box<dyn EventStore>) -> Self {
        Self {
            domains: HashMap::new(),
            store,
        }
    }

    fn observe(&mut self, params: ObserveParams) -> Result<ObserveResponse, String> {
        if params.beyond_scope_ppm > PPM || params.destructive_ppm > PPM {
            return Err(format!("confidence_ppm must be in 0..={PPM}"));
        }

        let domain_key = DomainKey::new(params.subject.clone(), params.scope.clone());
        let judgment = JudgmentObserved {
            provider: params.provider,
            subject: params.subject,
            scope: params.scope,
            task: params.task,
            tool: params.action.clone(),
            intent: params.intent,
            beyond_scope_ppm: params.beyond_scope_ppm,
            destructive_ppm: params.destructive_ppm,
        };

        let (outcome, relation_transitions, provenance) = {
            let domain = self
                .domains
                .entry(domain_key.clone())
                .or_insert_with(DomainRuntime::new);

            // v0.1 trust boundary: reliability/expiry are server-bound. The MCP caller
            // cannot self-assert source reliability or corroborator count.
            let context = AdmissionContext::demo(domain.next_vtick);
            domain.next_vtick += 1;

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
                .map(|change| {
                    format!(
                        "{}:{}:{}",
                        change.id, change.relation, change.transition
                    )
                })
                .collect();
            let provenance = domain.governance.provenance()[provenance_start..].to_vec();
            (outcome, relation_transitions, provenance)
        };

        let state_version = if outcome.relation_changes.is_empty() {
            self.store.current_state_version(&domain_key)
        } else {
            self.store.advance_state_version(&domain_key)
        };

        self.store.record_event(StoredEvent {
            event_id: outcome.event_id.clone(),
            domain: domain_key,
            action: params.action,
            state_version: state_version.clone(),
            relation_transitions: relation_transitions.clone(),
            provenance,
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
            event_id: outcome.event_id,
            admissions,
            relation_transitions,
            state_version,
        })
    }

    fn decide(&mut self, params: DecideParams) -> Result<DecideResponse, String> {
        let domain_key = DomainKey::new(params.subject, params.scope);
        let (projected_decision, governing_authorization, mut provenance) = {
            let domain = self
                .domains
                .entry(domain_key.clone())
                .or_insert_with(DomainRuntime::new);
            (
                domain.projected_decision,
                domain.governing_authorization.clone(),
                domain.governance.provenance().to_vec(),
            )
        };

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
}

#[derive(Clone)]
pub struct R2rMcpServer {
    runtime: Arc<Mutex<GovernanceRuntime>>,
}

impl R2rMcpServer {
    pub fn new() -> Self {
        Self::with_store(Box::new(MemoryEventStore::new()))
    }

    pub fn with_store(store: Box<dyn EventStore>) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(GovernanceRuntime::with_store(store))),
        }
    }

    fn with_runtime<T: Serialize>(
        &self,
        f: impl FnOnce(&mut GovernanceRuntime) -> Result<T, String>,
    ) -> String {
        match self.runtime.lock() {
            Ok(mut runtime) => match f(&mut runtime) {
                Ok(value) => serde_json::to_string_pretty(&value)
                    .unwrap_or_else(|e| error_json(format!("serialization error: {e}"))),
                Err(error) => error_json(error),
            },
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
}

fn error_json(error: String) -> String {
    serde_json::to_string_pretty(&ErrorResponse { error })
        .unwrap_or_else(|_| "{\"error\":\"serialization failure\"}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn domains_are_isolated_inside_one_mcp_process() {
        let mut runtime = GovernanceRuntime::new();
        runtime
            .observe(risky_observation("agent:coder-1", "repo:alpha"))
            .expect("observe alpha");

        let alpha = runtime
            .decide(decide("agent:coder-1", "repo:alpha"))
            .expect("decide alpha");
        let beta = runtime
            .decide(decide("agent:coder-2", "repo:beta"))
            .expect("decide beta");

        assert_eq!(alpha.decision, "DENY");
        assert_eq!(alpha.state_version, "state-000001");
        assert_eq!(beta.decision, "ALLOW");
        assert_eq!(beta.state_version, "state-000000");

        let beta_observed = runtime
            .observe(risky_observation("agent:coder-2", "repo:beta"))
            .expect("observe beta");
        assert_eq!(beta_observed.state_version, "state-000001");

        let alpha_again = runtime
            .decide(decide("agent:coder-1", "repo:alpha"))
            .expect("decide alpha again");
        assert_eq!(alpha_again.decision, "DENY");
        assert_eq!(alpha_again.state_version, "state-000001");
    }
}
