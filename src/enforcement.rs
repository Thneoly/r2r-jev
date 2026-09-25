use crate::store::{DomainKey, EventStore};
use serde::Serialize;

/// Inputs an execution adapter must bind to before performing an external side effect.
///
/// The `expected_state_version` is the version returned by the original
/// `r2r_decide` call. The adapter must not substitute the current version:
/// doing so would allow an old decision to be silently upgraded after the
/// relation graph changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionBindingRequest {
    pub decision_id: String,
    pub action: String,
    pub expected_state_version: String,
}

impl ExecutionBindingRequest {
    pub fn new(
        decision_id: impl Into<String>,
        action: impl Into<String>,
        expected_state_version: impl Into<String>,
    ) -> Self {
        Self {
            decision_id: decision_id.into(),
            action: action.into(),
            expected_state_version: expected_state_version.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionBindingResult {
    pub allowed: bool,
    pub reason_code: &'static str,
    pub decision_id: String,
    pub subject: Option<String>,
    pub scope: Option<String>,
    pub action: String,
    pub decision_state_version: Option<String>,
    pub current_state_version: Option<String>,
}

impl ExecutionBindingResult {
    fn deny_unknown(request: &ExecutionBindingRequest) -> Self {
        Self {
            allowed: false,
            reason_code: "UNKNOWN_DECISION",
            decision_id: request.decision_id.clone(),
            subject: None,
            scope: None,
            action: request.action.clone(),
            decision_state_version: None,
            current_state_version: None,
        }
    }

    fn for_domain(
        request: &ExecutionBindingRequest,
        domain: &DomainKey,
        decision_state_version: &str,
        current_state_version: &str,
        allowed: bool,
        reason_code: &'static str,
    ) -> Self {
        Self {
            allowed,
            reason_code,
            decision_id: request.decision_id.clone(),
            subject: Some(domain.subject.clone()),
            scope: Some(domain.scope.clone()),
            action: request.action.clone(),
            decision_state_version: Some(decision_state_version.to_string()),
            current_state_version: Some(current_state_version.to_string()),
        }
    }
}

/// Validate that a previously issued decision is still executable *now*.
///
/// This is the reusable enforcement gate between R2R governance and an
/// external execution adapter. It is deliberately read-only: it neither
/// mutates relation state nor performs the downstream action.
///
/// Required invariants:
/// - the decision exists;
/// - the decision verdict was `ALLOW`;
/// - the requested action is exactly the action that was decided;
/// - the adapter presents the exact state version bound to the decision;
/// - the domain's current relation state version is still that same version.
///
/// The final check prevents a stale ALLOW decision from being reused after an
/// intervening R -> R transition. Persistent-store health failures are errors
/// rather than permissive results, preserving fail-closed behavior.
pub fn validate_execution(
    store: &dyn EventStore,
    request: &ExecutionBindingRequest,
) -> Result<ExecutionBindingResult, String> {
    store.health()?;

    let Some(decision) = store.decision(&request.decision_id) else {
        return Ok(ExecutionBindingResult::deny_unknown(request));
    };

    let current_state_version = store.current_state_version(&decision.domain);

    if decision.verdict != "ALLOW" {
        return Ok(ExecutionBindingResult::for_domain(
            request,
            &decision.domain,
            &decision.state_version,
            &current_state_version,
            false,
            "DECISION_NOT_ALLOW",
        ));
    }

    if decision.action != request.action {
        return Ok(ExecutionBindingResult::for_domain(
            request,
            &decision.domain,
            &decision.state_version,
            &current_state_version,
            false,
            "ACTION_MISMATCH",
        ));
    }

    if decision.state_version != request.expected_state_version {
        return Ok(ExecutionBindingResult::for_domain(
            request,
            &decision.domain,
            &decision.state_version,
            &current_state_version,
            false,
            "EXPECTED_STATE_VERSION_MISMATCH",
        ));
    }

    if current_state_version != decision.state_version {
        return Ok(ExecutionBindingResult::for_domain(
            request,
            &decision.domain,
            &decision.state_version,
            &current_state_version,
            false,
            "STATE_VERSION_STALE",
        ));
    }

    Ok(ExecutionBindingResult::for_domain(
        request,
        &decision.domain,
        &decision.state_version,
        &current_state_version,
        true,
        "EXECUTION_ALLOWED",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::memory::MemoryEventStore;
    use crate::store::StoredDecision;

    fn store_with_decision(verdict: &str) -> (MemoryEventStore, String) {
        let mut store = MemoryEventStore::new();
        let domain = DomainKey::new("agent:coder-1", "repo:alpha");
        let state_version = store.advance_state_version(&domain);
        let decision_id = store.next_decision_id();
        store.record_decision(StoredDecision {
            decision_id: decision_id.clone(),
            domain,
            action: "github.merge_pull_request".to_string(),
            verdict: verdict.to_string(),
            reason_code: if verdict == "ALLOW" {
                "AUTHORIZATION_ACTIVE".to_string()
            } else {
                "AUTHORIZATION_SUSPENDED".to_string()
            },
            governing_relation_id: None,
            state_version,
            provenance: Vec::new(),
        });
        (store, decision_id)
    }

    #[test]
    fn current_allow_decision_is_executable() {
        let (store, decision_id) = store_with_decision("ALLOW");
        let result = validate_execution(
            &store,
            &ExecutionBindingRequest::new(
                decision_id,
                "github.merge_pull_request",
                "state-000001",
            ),
        )
        .expect("validation");

        assert!(result.allowed);
        assert_eq!(result.reason_code, "EXECUTION_ALLOWED");
        assert_eq!(result.current_state_version.as_deref(), Some("state-000001"));
    }

    #[test]
    fn stale_allow_is_rejected_after_relation_state_advances() {
        let (mut store, decision_id) = store_with_decision("ALLOW");
        let domain = DomainKey::new("agent:coder-1", "repo:alpha");
        assert_eq!(store.advance_state_version(&domain), "state-000002");

        let result = validate_execution(
            &store,
            &ExecutionBindingRequest::new(
                decision_id,
                "github.merge_pull_request",
                "state-000001",
            ),
        )
        .expect("validation");

        assert!(!result.allowed);
        assert_eq!(result.reason_code, "STATE_VERSION_STALE");
        assert_eq!(result.decision_state_version.as_deref(), Some("state-000001"));
        assert_eq!(result.current_state_version.as_deref(), Some("state-000002"));
    }

    #[test]
    fn caller_cannot_substitute_a_different_expected_version() {
        let (store, decision_id) = store_with_decision("ALLOW");
        let result = validate_execution(
            &store,
            &ExecutionBindingRequest::new(
                decision_id,
                "github.merge_pull_request",
                "state-000999",
            ),
        )
        .expect("validation");

        assert!(!result.allowed);
        assert_eq!(result.reason_code, "EXPECTED_STATE_VERSION_MISMATCH");
    }

    #[test]
    fn denied_decision_never_becomes_executable() {
        let (store, decision_id) = store_with_decision("DENY");
        let result = validate_execution(
            &store,
            &ExecutionBindingRequest::new(
                decision_id,
                "github.merge_pull_request",
                "state-000001",
            ),
        )
        .expect("validation");

        assert!(!result.allowed);
        assert_eq!(result.reason_code, "DECISION_NOT_ALLOW");
    }

    #[test]
    fn decision_cannot_be_reused_for_another_action() {
        let (store, decision_id) = store_with_decision("ALLOW");
        let result = validate_execution(
            &store,
            &ExecutionBindingRequest::new(decision_id, "github.delete_repository", "state-000001"),
        )
        .expect("validation");

        assert!(!result.allowed);
        assert_eq!(result.reason_code, "ACTION_MISMATCH");
    }

    #[test]
    fn unknown_decision_fails_closed() {
        let store = MemoryEventStore::new();
        let result = validate_execution(
            &store,
            &ExecutionBindingRequest::new(
                "decision-does-not-exist",
                "github.merge_pull_request",
                "state-000000",
            ),
        )
        .expect("validation");

        assert!(!result.allowed);
        assert_eq!(result.reason_code, "UNKNOWN_DECISION");
        assert!(result.current_state_version.is_none());
    }
}
