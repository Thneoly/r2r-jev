//! Executable reference implementation for Evidence Admission Semantics v0.1.
//!
//! The probabilistic model contributes `confidence_ppm` only. Reliability,
//! corroboration provenance, virtual time, and expiry belong to the trusted
//! adapter/runtime boundary.
//!
//! The admission result is NOT an authorization decision. `Accept` emits an
//! `EvidenceAdmitted`-shaped record that may then be consumed by R2R rules.

const PPM: u32 = 1_000_000;
const MIN_SOURCE_RELIABILITY: u32 = 600_000;
const MIN_REVIEW_CONFIDENCE: u32 = 600_000;
const STRONG_CONFIDENCE: u32 = 900_000;
const STRONG_RELIABILITY: u32 = 800_000;
const CORROBORATED_CONFIDENCE: u32 = 700_000;
const CORROBORATED_RELIABILITY: u32 = 700_000;
const MIN_CORROBORATORS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvidenceKind {
    BeyondScope,
    DestructiveAction,
    Unsupported,
}

impl EvidenceKind {
    fn is_supported(self) -> bool {
        matches!(self, Self::BeyondScope | Self::DestructiveAction)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::BeyondScope => "BeyondScope",
            Self::DestructiveAction => "DestructiveAction",
            Self::Unsupported => "Unsupported",
        }
    }
}

#[derive(Debug, Clone)]
struct EvidenceEnvelope {
    evidence_id: &'static str,
    kind: EvidenceKind,
    subject: &'static str,
    scope: &'static str,
    source: &'static str,
    confidence_ppm: u32,
    source_reliability_ppm: u32,
    independent_corroborators: u8,
    observed_vtick: u64,
    expires_vtick: u64,
}

impl EvidenceEnvelope {
    fn validate(&self) {
        assert!(self.confidence_ppm <= PPM, "confidence_ppm out of range");
        assert!(
            self.source_reliability_ppm <= PPM,
            "source_reliability_ppm out of range"
        );
        assert!(
            self.expires_vtick >= self.observed_vtick,
            "expiry must not precede observation"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdmissionClass {
    Strong,
    Corroborated,
}

impl AdmissionClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Strong => "Strong",
            Self::Corroborated => "Corroborated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RejectReason {
    UnsupportedKind,
    Expired,
    SourceBelowTrustFloor,
    InsufficientSupport,
}

impl RejectReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedKind => "UnsupportedKind",
            Self::Expired => "Expired",
            Self::SourceBelowTrustFloor => "SourceBelowTrustFloor",
            Self::InsufficientSupport => "InsufficientSupport",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldReason {
    NeedsCorroborationOrReview,
}

impl HoldReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::NeedsCorroborationOrReview => "NeedsCorroborationOrReview",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdmissionDecision {
    Reject(RejectReason),
    Hold(HoldReason),
    Accept(AdmissionClass),
}

impl AdmissionDecision {
    fn render(self) -> String {
        match self {
            Self::Reject(reason) => format!("Reject({})", reason.as_str()),
            Self::Hold(reason) => format!("Hold({})", reason.as_str()),
            Self::Accept(class) => format!("Accept({})", class.as_str()),
        }
    }
}

#[derive(Debug, Clone)]
struct EvidenceAdmitted {
    evidence_id: &'static str,
    kind: EvidenceKind,
    subject: &'static str,
    scope: &'static str,
    class: AdmissionClass,
    admission_policy_version: &'static str,
}

fn admit(e: &EvidenceEnvelope, now_vtick: u64) -> AdmissionDecision {
    e.validate();

    // Ordering is normative for v0.1.
    if !e.kind.is_supported() {
        return AdmissionDecision::Reject(RejectReason::UnsupportedKind);
    }

    if now_vtick > e.expires_vtick {
        return AdmissionDecision::Reject(RejectReason::Expired);
    }

    if e.source_reliability_ppm < MIN_SOURCE_RELIABILITY {
        return AdmissionDecision::Reject(RejectReason::SourceBelowTrustFloor);
    }

    if e.confidence_ppm >= STRONG_CONFIDENCE
        && e.source_reliability_ppm >= STRONG_RELIABILITY
    {
        return AdmissionDecision::Accept(AdmissionClass::Strong);
    }

    if e.confidence_ppm >= CORROBORATED_CONFIDENCE
        && e.source_reliability_ppm >= CORROBORATED_RELIABILITY
        && e.independent_corroborators >= MIN_CORROBORATORS
    {
        return AdmissionDecision::Accept(AdmissionClass::Corroborated);
    }

    if e.confidence_ppm >= MIN_REVIEW_CONFIDENCE {
        return AdmissionDecision::Hold(HoldReason::NeedsCorroborationOrReview);
    }

    AdmissionDecision::Reject(RejectReason::InsufficientSupport)
}

fn to_admitted(e: &EvidenceEnvelope, decision: AdmissionDecision) -> Option<EvidenceAdmitted> {
    match decision {
        AdmissionDecision::Accept(class) => Some(EvidenceAdmitted {
            evidence_id: e.evidence_id,
            kind: e.kind,
            subject: e.subject,
            scope: e.scope,
            class,
            admission_policy_version: "0.1",
        }),
        AdmissionDecision::Hold(_) | AdmissionDecision::Reject(_) => None,
    }
}

fn examples() -> Vec<(&'static str, EvidenceEnvelope, u64)> {
    vec![
        (
            "A strong trusted signal",
            EvidenceEnvelope {
                evidence_id: "ev-a",
                kind: EvidenceKind::BeyondScope,
                subject: "coding-agent",
                scope: "repo-alpha",
                source: "jev",
                confidence_ppm: 940_000,
                source_reliability_ppm: 850_000,
                independent_corroborators: 0,
                observed_vtick: 10,
                expires_vtick: 20,
            },
            10,
        ),
        (
            "B high confidence, untrusted source",
            EvidenceEnvelope {
                evidence_id: "ev-b",
                kind: EvidenceKind::BeyondScope,
                subject: "coding-agent",
                scope: "repo-alpha",
                source: "untrusted-judge",
                confidence_ppm: 970_000,
                source_reliability_ppm: 400_000,
                independent_corroborators: 0,
                observed_vtick: 10,
                expires_vtick: 20,
            },
            10,
        ),
        (
            "C medium signal, independently corroborated",
            EvidenceEnvelope {
                evidence_id: "ev-c",
                kind: EvidenceKind::DestructiveAction,
                subject: "coding-agent",
                scope: "repo-alpha",
                source: "jev",
                confidence_ppm: 760_000,
                source_reliability_ppm: 800_000,
                independent_corroborators: 2,
                observed_vtick: 11,
                expires_vtick: 20,
            },
            11,
        ),
        (
            "D medium signal, no corroboration",
            EvidenceEnvelope {
                evidence_id: "ev-d",
                kind: EvidenceKind::DestructiveAction,
                subject: "coding-agent",
                scope: "repo-alpha",
                source: "jev",
                confidence_ppm: 760_000,
                source_reliability_ppm: 800_000,
                independent_corroborators: 0,
                observed_vtick: 11,
                expires_vtick: 20,
            },
            11,
        ),
        (
            "E expired evidence",
            EvidenceEnvelope {
                evidence_id: "ev-e",
                kind: EvidenceKind::BeyondScope,
                subject: "coding-agent",
                scope: "repo-alpha",
                source: "jev",
                confidence_ppm: 990_000,
                source_reliability_ppm: 990_000,
                independent_corroborators: 3,
                observed_vtick: 1,
                expires_vtick: 5,
            },
            6,
        ),
        (
            "F low support",
            EvidenceEnvelope {
                evidence_id: "ev-f",
                kind: EvidenceKind::BeyondScope,
                subject: "coding-agent",
                scope: "repo-alpha",
                source: "jev",
                confidence_ppm: 420_000,
                source_reliability_ppm: 900_000,
                independent_corroborators: 3,
                observed_vtick: 12,
                expires_vtick: 20,
            },
            12,
        ),
        (
            "G unsupported evidence kind",
            EvidenceEnvelope {
                evidence_id: "ev-g",
                kind: EvidenceKind::Unsupported,
                subject: "coding-agent",
                scope: "repo-alpha",
                source: "jev",
                confidence_ppm: 999_000,
                source_reliability_ppm: 999_000,
                independent_corroborators: 9,
                observed_vtick: 12,
                expires_vtick: 20,
            },
            12,
        ),
    ]
}

fn main() {
    println!("Evidence Admission Semantics v0.1");
    println!("Judgment -> Evidence -> Admission -> R2R -> Enforcement\n");

    for (label, evidence, now_vtick) in examples() {
        let decision = admit(&evidence, now_vtick);
        println!("{label}");
        println!(
            "  source={} kind={} confidence={} reliability={} corroborators={} now={} expires={}",
            evidence.source,
            evidence.kind.as_str(),
            evidence.confidence_ppm,
            evidence.source_reliability_ppm,
            evidence.independent_corroborators,
            now_vtick,
            evidence.expires_vtick
        );
        println!("  admission={}", decision.render());

        if let Some(admitted) = to_admitted(&evidence, decision) {
            println!(
                "  emits=EvidenceAdmitted(id={}, kind={}, subject={}, scope={}, class={}, policy={})",
                admitted.evidence_id,
                admitted.kind.as_str(),
                admitted.subject,
                admitted.scope,
                admitted.class.as_str(),
                admitted.admission_policy_version
            );
            println!("  relation_mutation=NONE (R2R rules decide downstream effects)");
        } else {
            println!("  emits=no governance-active evidence");
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> EvidenceEnvelope {
        EvidenceEnvelope {
            evidence_id: "test",
            kind: EvidenceKind::BeyondScope,
            subject: "agent",
            scope: "repo",
            source: "jev",
            confidence_ppm: 940_000,
            source_reliability_ppm: 850_000,
            independent_corroborators: 0,
            observed_vtick: 10,
            expires_vtick: 20,
        }
    }

    #[test]
    fn strong_trusted_signal_is_accepted() {
        assert_eq!(
            admit(&base(), 10),
            AdmissionDecision::Accept(AdmissionClass::Strong)
        );
    }

    #[test]
    fn high_confidence_does_not_override_low_source_reliability() {
        let mut e = base();
        e.confidence_ppm = 999_000;
        e.source_reliability_ppm = 400_000;
        assert_eq!(
            admit(&e, 10),
            AdmissionDecision::Reject(RejectReason::SourceBelowTrustFloor)
        );
    }

    #[test]
    fn corroboration_can_admit_medium_signal() {
        let mut e = base();
        e.confidence_ppm = 760_000;
        e.source_reliability_ppm = 800_000;
        e.independent_corroborators = 2;
        assert_eq!(
            admit(&e, 10),
            AdmissionDecision::Accept(AdmissionClass::Corroborated)
        );
    }

    #[test]
    fn medium_single_source_signal_is_held_not_authorized() {
        let mut e = base();
        e.confidence_ppm = 760_000;
        e.independent_corroborators = 0;
        assert_eq!(
            admit(&e, 10),
            AdmissionDecision::Hold(HoldReason::NeedsCorroborationOrReview)
        );
        assert!(to_admitted(&e, admit(&e, 10)).is_none());
    }

    #[test]
    fn expired_high_confidence_evidence_is_rejected() {
        let mut e = base();
        e.confidence_ppm = 999_000;
        e.source_reliability_ppm = 999_000;
        e.expires_vtick = 11;
        assert_eq!(
            admit(&e, 12),
            AdmissionDecision::Reject(RejectReason::Expired)
        );
    }

    #[test]
    fn unsupported_kind_is_rejected_before_any_score_logic() {
        let mut e = base();
        e.kind = EvidenceKind::Unsupported;
        e.confidence_ppm = 1_000_000;
        e.source_reliability_ppm = 1_000_000;
        e.independent_corroborators = 99;
        assert_eq!(
            admit(&e, 10),
            AdmissionDecision::Reject(RejectReason::UnsupportedKind)
        );
    }

    #[test]
    fn admission_is_deterministic() {
        let e = base();
        assert_eq!(admit(&e, 10), admit(&e, 10));
    }
}
