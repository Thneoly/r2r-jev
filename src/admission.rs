//! Evidence Admission Semantics v0.1.
//!
//! This module is the deterministic boundary between probabilistic judgments
//! and R2R relation transitions.
//!
//! Judgment -> Evidence -> Admission -> R2R -> Enforcement

pub const POLICY_VERSION: &str = "0.1";
pub const PPM: u32 = 1_000_000;
pub const MIN_SOURCE_RELIABILITY: u32 = 600_000;
pub const MIN_REVIEW_CONFIDENCE: u32 = 600_000;
pub const STRONG_CONFIDENCE: u32 = 900_000;
pub const STRONG_RELIABILITY: u32 = 800_000;
pub const CORROBORATED_CONFIDENCE: u32 = 700_000;
pub const CORROBORATED_RELIABILITY: u32 = 700_000;
pub const MIN_CORROBORATORS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    BeyondScope,
    DestructiveAction,
    Unsupported,
}

impl EvidenceKind {
    pub fn is_supported(self) -> bool {
        matches!(self, Self::BeyondScope | Self::DestructiveAction)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::BeyondScope => "BeyondScope",
            Self::DestructiveAction => "DestructiveAction",
            Self::Unsupported => "Unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionClass {
    Strong,
    Corroborated,
}

impl AdmissionClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strong => "Strong",
            Self::Corroborated => "Corroborated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    UnsupportedKind,
    Expired,
    SourceBelowTrustFloor,
    InsufficientSupport,
}

impl RejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedKind => "UnsupportedKind",
            Self::Expired => "Expired",
            Self::SourceBelowTrustFloor => "SourceBelowTrustFloor",
            Self::InsufficientSupport => "InsufficientSupport",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldReason {
    NeedsCorroborationOrReview,
}

impl HoldReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsCorroborationOrReview => "NeedsCorroborationOrReview",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionDecision {
    Reject(RejectReason),
    Hold(HoldReason),
    Accept(AdmissionClass),
}

impl AdmissionDecision {
    pub fn render(self) -> String {
        match self {
            Self::Reject(reason) => format!("Reject({})", reason.as_str()),
            Self::Hold(reason) => format!("Hold({})", reason.as_str()),
            Self::Accept(class) => format!("Accept({})", class.as_str()),
        }
    }

    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Accept(_))
    }
}

/// Metadata bound by the trusted adapter/runtime, not by the probabilistic
/// model itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionContext {
    pub source_reliability_ppm: u32,
    pub independent_corroborators: u8,
    pub now_vtick: u64,
    pub expires_vtick: u64,
}

impl AdmissionContext {
    pub fn new(
        source_reliability_ppm: u32,
        independent_corroborators: u8,
        now_vtick: u64,
        expires_vtick: u64,
    ) -> Self {
        assert!(
            source_reliability_ppm <= PPM,
            "source reliability out of range"
        );
        assert!(
            expires_vtick >= now_vtick,
            "expiry precedes observation time"
        );
        Self {
            source_reliability_ppm,
            independent_corroborators,
            now_vtick,
            expires_vtick,
        }
    }

    /// Reproducible demo configuration only. 850k is a configured trust weight,
    /// not a measured claim about Jev accuracy.
    pub fn demo(now_vtick: u64) -> Self {
        Self::new(850_000, 0, now_vtick, now_vtick + 10)
    }
}

/// Deterministic v0.1 admission policy.
pub fn admit(
    kind: EvidenceKind,
    confidence_ppm: u32,
    context: AdmissionContext,
) -> AdmissionDecision {
    assert!(confidence_ppm <= PPM, "confidence_ppm out of range");

    // Ordering is normative for v0.1.
    if !kind.is_supported() {
        return AdmissionDecision::Reject(RejectReason::UnsupportedKind);
    }

    if context.now_vtick > context.expires_vtick {
        return AdmissionDecision::Reject(RejectReason::Expired);
    }

    if context.source_reliability_ppm < MIN_SOURCE_RELIABILITY {
        return AdmissionDecision::Reject(RejectReason::SourceBelowTrustFloor);
    }

    if confidence_ppm >= STRONG_CONFIDENCE && context.source_reliability_ppm >= STRONG_RELIABILITY {
        return AdmissionDecision::Accept(AdmissionClass::Strong);
    }

    if confidence_ppm >= CORROBORATED_CONFIDENCE
        && context.source_reliability_ppm >= CORROBORATED_RELIABILITY
        && context.independent_corroborators >= MIN_CORROBORATORS
    {
        return AdmissionDecision::Accept(AdmissionClass::Corroborated);
    }

    if confidence_ppm >= MIN_REVIEW_CONFIDENCE {
        return AdmissionDecision::Hold(HoldReason::NeedsCorroborationOrReview);
    }

    AdmissionDecision::Reject(RejectReason::InsufficientSupport)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_trusted_signal_is_accepted() {
        assert_eq!(
            admit(
                EvidenceKind::BeyondScope,
                940_000,
                AdmissionContext::demo(10)
            ),
            AdmissionDecision::Accept(AdmissionClass::Strong)
        );
    }

    #[test]
    fn high_confidence_does_not_override_low_source_reliability() {
        let context = AdmissionContext::new(400_000, 0, 10, 20);
        assert_eq!(
            admit(EvidenceKind::BeyondScope, 999_000, context),
            AdmissionDecision::Reject(RejectReason::SourceBelowTrustFloor)
        );
    }

    #[test]
    fn corroboration_can_admit_medium_signal() {
        let context = AdmissionContext::new(800_000, 2, 10, 20);
        assert_eq!(
            admit(EvidenceKind::DestructiveAction, 760_000, context),
            AdmissionDecision::Accept(AdmissionClass::Corroborated)
        );
    }

    #[test]
    fn medium_single_source_signal_is_held() {
        let context = AdmissionContext::new(800_000, 0, 10, 20);
        assert_eq!(
            admit(EvidenceKind::DestructiveAction, 760_000, context),
            AdmissionDecision::Hold(HoldReason::NeedsCorroborationOrReview)
        );
    }

    #[test]
    fn expired_high_confidence_evidence_is_rejected() {
        let context = AdmissionContext {
            source_reliability_ppm: 999_000,
            independent_corroborators: 3,
            now_vtick: 12,
            expires_vtick: 11,
        };
        assert_eq!(
            admit(EvidenceKind::BeyondScope, 999_000, context),
            AdmissionDecision::Reject(RejectReason::Expired)
        );
    }

    #[test]
    fn unsupported_kind_is_rejected_before_score_logic() {
        let context = AdmissionContext::new(1_000_000, 9, 10, 20);
        assert_eq!(
            admit(EvidenceKind::Unsupported, 1_000_000, context),
            AdmissionDecision::Reject(RejectReason::UnsupportedKind)
        );
    }

    #[test]
    fn admission_is_deterministic() {
        let context = AdmissionContext::demo(10);
        assert_eq!(
            admit(EvidenceKind::BeyondScope, 940_000, context),
            admit(EvidenceKind::BeyondScope, 940_000, context)
        );
    }
}
