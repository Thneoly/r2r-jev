use crate::model::{ppm_to_display, JudgmentObserved};

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

#[derive(Debug)]
pub struct GovernanceState {
    pub trust: TrustPhase,
    pub delegation: DelegationPhase,
    pub authorization: AuthorizationPhase,
}

impl Default for GovernanceState {
    fn default() -> Self {
        Self {
            trust: TrustPhase::Active,
            delegation: DelegationPhase::Active,
            authorization: AuthorizationPhase::Active,
        }
    }
}

#[derive(Debug)]
pub struct TransitionReport {
    pub evidence_created: bool,
    pub trust_changed: bool,
    pub delegation_changed: bool,
    pub authorization_changed: bool,
    pub allow: bool,
}

pub fn apply_judgment(state: &mut GovernanceState, event: &JudgmentObserved) -> TransitionReport {
    let risky = event.beyond_scope_ppm >= BEYOND_SCOPE_THRESHOLD_PPM
        || event.destructive_ppm >= DESTRUCTIVE_THRESHOLD_PPM;

    let mut report = TransitionReport {
        evidence_created: true,
        trust_changed: false,
        delegation_changed: false,
        authorization_changed: false,
        allow: true,
    };

    if risky {
        if state.trust != TrustPhase::Warning {
            state.trust = TrustPhase::Warning;
            report.trust_changed = true;
        }
        if state.delegation != DelegationPhase::Degraded {
            state.delegation = DelegationPhase::Degraded;
            report.delegation_changed = true;
        }
        if state.authorization != AuthorizationPhase::Suspended {
            state.authorization = AuthorizationPhase::Suspended;
            report.authorization_changed = true;
        }
    }

    report.allow = state.authorization == AuthorizationPhase::Active;
    report
}

pub fn print_report(event: &JudgmentObserved, before: &GovernanceState, after: &GovernanceState, report: &TransitionReport) {
    println!("JudgmentObserved");
    println!("  provider      = {}", event.provider);
    println!("  subject       = {}", event.subject);
    println!("  scope         = {}", event.scope);
    println!("  tool          = {}", event.tool);
    println!("  beyond_scope  = {}", ppm_to_display(event.beyond_scope_ppm));
    println!("  destructive   = {}", ppm_to_display(event.destructive_ppm));
    println!();
    println!("R2R causal chain");
    println!("  Evidence       {}", if report.evidence_created { "created" } else { "unchanged" });
    println!("  Trust          {:?} -> {:?}", before.trust, after.trust);
    println!("  Delegation     {:?} -> {:?}", before.delegation, after.delegation);
    println!("  Authorization  {:?} -> {:?}", before.authorization, after.authorization);
    println!();
    println!("Decision: {}", if report.allow { "ALLOW" } else { "DENY" });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::JudgmentObserved;

    fn event(beyond_scope: f64, destructive: f64) -> JudgmentObserved {
        JudgmentObserved::from_probabilities(
            "fixture",
            "coder",
            "repo-alpha",
            "fix login redirect",
            "merge_pull_request",
            "merge unrelated changes",
            beyond_scope,
            destructive,
        )
    }

    #[test]
    fn high_risk_judgment_changes_persistent_governance_state() {
        let mut state = GovernanceState::default();
        let report = apply_judgment(&mut state, &event(0.94, 0.72));
        assert_eq!(state.trust, TrustPhase::Warning);
        assert_eq!(state.delegation, DelegationPhase::Degraded);
        assert_eq!(state.authorization, AuthorizationPhase::Suspended);
        assert!(!report.allow);
    }

    #[test]
    fn low_risk_judgment_does_not_revoke_authority() {
        let mut state = GovernanceState::default();
        let report = apply_judgment(&mut state, &event(0.20, 0.10));
        assert_eq!(state.authorization, AuthorizationPhase::Active);
        assert!(report.allow);
    }
}
