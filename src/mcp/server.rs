use super::schema::{
    AdmissionSummary, DecideParams, DecideResponse, ErrorResponse, ExplainParams, ExplainResponse,
    GoverningRelation, ObserveParams, ObserveResponse,
};
use crate::admission::{AdmissionContext, PPM};
use crate::model::JudgmentObserved;
use crate::r2r::{Decision, Governance};
use crate::store::memory::{MemoryEventStore, StoredDecision, StoredEvent};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use serde::Serialize;
use std::sync::{Arc, Mutex};

struct GovernanceRuntime {
    governance: Governance,
    store: MemoryEventStore,
    next_vtick: u64,
    projected_decision: Decision,
    governing_authorization: Option<String>,
    bound_domain: Option<(String, String)>,
}

impl GovernanceRuntime {
    fn new() -> Self {
        Self {
            governance: Governance::new(),
            store: MemoryEventStore::new(),
            next_vtick: 1,
            projected_decision: Decision::Allow,
            governing_authorization: None,
            bound_domain: None,
        }
    }

    fn ensure_or_bind_domain(&mut self, subject: &str, scope: &str) -> Result<(), String> {
        match &self.bound_domain {
            Some((bound_subject, bound_scope))
                if bound_subject != subject || bound_scope != scope =>
            {
                Err(format!(
                    "prototype is bound to subject={bound_subject}, scope={bound_scope}; start a separate r2r-mcp process for subject={subject}, scope={scope}"
                ))
            }
            Some(_) => Ok(()),
            None => {
                self.bound_domain = Some((subject.to_string(), scope.to_string()));
                Ok(())
            }
        }
    }

    fn ensure_domain(&self, subject: &str, scope: &str) -> Result<(), String> {
        match &self.bound_domain {
            Some((bound_subject, bound_scope))
                if bound_subject != subject || bound_scope != scope =>
            {
                Err(format!(
                    "prototype is bound to subject={bound_subject}, scope={bound_scope}; requested subject={subject}, scope={scope}"
                ))
            }
            _ => Ok(()),
        }
    }

    fn observe(&mut self, params: ObserveParams) -> Result<ObserveResponse, String> {
        if params.beyond_scope_ppm > PPM || params.destructive_ppm > PPM {
            return Err(format!("confidence_ppm must be in 0..={PPM}"));
        }
        self.ensure_or_bind_domain(&params.subject, &params.scope)?;

        let judgment = JudgmentObserved {
            provider: params.provider,
            subject: params.subject.clone(),
            scope: params.scope.clone(),
            task: params.task,
            tool: params.action.clone(),
            intent: params.intent,
            beyond_scope_ppm: params.beyond_scope_ppm,
            destructive_ppm: params.destructive_ppm,
        };

        // v0.1 trust boundary: reliability/expiry are server-bound. The MCP caller
        // cannot self-assert source reliability or corroborator count.
        let context = AdmissionContext::demo(self.next_vtick);
        self.next_vtick += 1;

        let provenance_start = self.governance.provenance().len();
        let outcome = self.governance.observe_judgment(&judgment, context);
        self.projected_decision = outcome.decision;

        for change in &outcome.relation_changes {
            if change.relation == "Authorization" {
                self.governing_authorization = Some(change.id.clone());
            }
        }

        let state_version = if outcome.relation_changes.is_empty() {
            self.store.current_state_version()
        } else {
            self.store.advance_state_version()
        };

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

        let provenance = self.governance.provenance()[provenance_start..].to_vec();
        self.store.record_event(StoredEvent {
            event_id: outcome.event_id.clone(),
            subject: params.subject,
            scope: params.scope,
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
        self.ensure_domain(&params.subject, &params.scope)?;

        let decision_id = self.store.next_decision_id();
        let state_version = self.store.current_state_version();
        let (verdict, reason_code, required_next_step) = match self.projected_decision {
            Decision::Allow => ("ALLOW", "AUTHORIZATION_ACTIVE", None),
            Decision::Deny => (
                "DENY",
                "AUTHORIZATION_SUSPENDED",
                Some("human_review".to_string()),
            ),
        };

        let governing_relations = self
            .governing_authorization
            .as_ref()
            .map(|relation_id| {
                vec![GoverningRelation {
                    relation_id: relation_id.clone(),
                    relation_type: "Authorization".to_string(),
                    state: if self.projected_decision == Decision::Deny {
                        "Suspended".to_string()
                    } else {
                        "Active".to_string()
                    },
                }]
            })
            .unwrap_or_default();

        let mut provenance = self.governance.provenance().to_vec();
        provenance.push(format!(
            "{} [state={}] -> {}({})",
            decision_id, state_version, verdict, params.action
        ));

        self.store.record_decision(StoredDecision {
            decision_id: decision_id.clone(),
            subject: params.subject,
            scope: params.scope,
            action: params.action,
            verdict: verdict.to_string(),
            reason_code: reason_code.to_string(),
            governing_relation_id: self.governing_authorization.clone(),
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
                "{} was denied because persistent authorization state was suspended at {}.",
                decision.action, decision.state_version
            ),
            _ => format!(
                "{} was allowed because no suspended authorization governed the action at {}.",
                decision.action, decision.state_version
            ),
        };

        Ok(ExplainResponse {
            decision_id: decision.decision_id.clone(),
            verdict: decision.verdict.clone(),
            reason_code: decision.reason_code.clone(),
            state_version: decision.state_version.clone(),
            governing_relation_id: decision.governing_relation_id.clone(),
            causal_chain: decision.provenance.clone(),
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
        Self {
            runtime: Arc::new(Mutex::new(GovernanceRuntime::new())),
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
        description = "Submit an untrusted probabilistic observation. The server binds trusted admission metadata, runs Evidence Admission, and applies only admitted evidence to R2R governance state."
    )]
    fn r2r_observe(&self, Parameters(params): Parameters<ObserveParams>) -> String {
        self.with_runtime(|runtime| runtime.observe(params))
    }

    #[tool(
        description = "Evaluate a proposed action against the current persistent R2R governance state without mutating relation state."
    )]
    fn r2r_decide(&self, Parameters(params): Parameters<DecideParams>) -> String {
        self.with_runtime(|runtime| runtime.decide(params))
    }

    #[tool(
        description = "Explain a previously issued R2R decision using its recorded state version and causal provenance chain."
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

    fn risky_observation() -> ObserveParams {
        ObserveParams {
            provider: "fixture:jev-style".to_string(),
            subject: "agent:coder-1".to_string(),
            scope: "repo:alpha".to_string(),
            task: "fix login redirect".to_string(),
            action: "github.merge_pull_request".to_string(),
            intent: "merge unrelated changes".to_string(),
            beyond_scope_ppm: 940_000,
            destructive_ppm: 720_000,
        }
    }

    #[test]
    fn observe_then_decide_then_explain_is_causal() {
        let mut runtime = GovernanceRuntime::new();
        let observed = runtime.observe(risky_observation()).expect("observe");
        assert_eq!(observed.state_version, "state-000001");
        assert!(observed
            .relation_transitions
            .iter()
            .any(|change| change.contains("Authorization:Active -> Suspended")));

        let decision = runtime
            .decide(DecideParams {
                subject: "agent:coder-1".to_string(),
                scope: "repo:alpha".to_string(),
                action: "github.merge_pull_request".to_string(),
                resource: Some("pr:42".to_string()),
                task: Some("fix login redirect".to_string()),
            })
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
    fn rejected_observation_does_not_advance_relation_state_version() {
        let mut runtime = GovernanceRuntime::new();
        let first = runtime.observe(risky_observation()).expect("observe risky");
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
    fn prototype_refuses_cross_domain_state_bleed() {
        let mut runtime = GovernanceRuntime::new();
        runtime.observe(risky_observation()).expect("observe");

        let err = runtime
            .decide(DecideParams {
                subject: "agent:coder-2".to_string(),
                scope: "repo:beta".to_string(),
                action: "github.merge_pull_request".to_string(),
                resource: None,
                task: None,
            })
            .expect_err("different domain must fail closed");

        assert!(err.contains("prototype is bound"));
    }
}
