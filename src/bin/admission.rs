//! Executable examples for Evidence Admission Semantics v0.1.

#[path = "../admission.rs"]
mod admission;

use admission::{
    admit, AdmissionContext, AdmissionDecision, EvidenceKind, POLICY_VERSION,
};

struct Example {
    label: &'static str,
    evidence_id: &'static str,
    kind: EvidenceKind,
    confidence_ppm: u32,
    context: AdmissionContext,
}

fn examples() -> Vec<Example> {
    vec![
        Example {
            label: "A strong trusted signal",
            evidence_id: "ev-a",
            kind: EvidenceKind::BeyondScope,
            confidence_ppm: 940_000,
            context: AdmissionContext::new(850_000, 0, 10, 20),
        },
        Example {
            label: "B high confidence, untrusted source",
            evidence_id: "ev-b",
            kind: EvidenceKind::BeyondScope,
            confidence_ppm: 970_000,
            context: AdmissionContext::new(400_000, 0, 10, 20),
        },
        Example {
            label: "C medium signal, independently corroborated",
            evidence_id: "ev-c",
            kind: EvidenceKind::DestructiveAction,
            confidence_ppm: 760_000,
            context: AdmissionContext::new(800_000, 2, 11, 20),
        },
        Example {
            label: "D medium signal, no corroboration",
            evidence_id: "ev-d",
            kind: EvidenceKind::DestructiveAction,
            confidence_ppm: 760_000,
            context: AdmissionContext::new(800_000, 0, 11, 20),
        },
        Example {
            label: "E expired evidence",
            evidence_id: "ev-e",
            kind: EvidenceKind::BeyondScope,
            confidence_ppm: 990_000,
            context: AdmissionContext {
                source_reliability_ppm: 990_000,
                independent_corroborators: 3,
                now_vtick: 6,
                expires_vtick: 5,
            },
        },
        Example {
            label: "F low support",
            evidence_id: "ev-f",
            kind: EvidenceKind::BeyondScope,
            confidence_ppm: 420_000,
            context: AdmissionContext::new(900_000, 3, 12, 20),
        },
        Example {
            label: "G unsupported evidence kind",
            evidence_id: "ev-g",
            kind: EvidenceKind::Unsupported,
            confidence_ppm: 999_000,
            context: AdmissionContext::new(999_000, 9, 12, 20),
        },
    ]
}

fn main() {
    println!("Evidence Admission Semantics v0.1");
    println!("Judgment -> Evidence -> Admission -> R2R -> Enforcement\n");

    for example in examples() {
        let decision = admit(example.kind, example.confidence_ppm, example.context);
        println!("{}", example.label);
        println!(
            "  kind={} confidence={} reliability={} corroborators={} now={} expires={}",
            example.kind.as_str(),
            example.confidence_ppm,
            example.context.source_reliability_ppm,
            example.context.independent_corroborators,
            example.context.now_vtick,
            example.context.expires_vtick
        );
        println!("  admission={}", decision.render());

        match decision {
            AdmissionDecision::Accept(class) => {
                println!(
                    "  emits=EvidenceAdmitted(id={}, kind={}, class={}, policy={})",
                    example.evidence_id,
                    example.kind.as_str(),
                    class.as_str(),
                    POLICY_VERSION
                );
                println!("  relation_mutation=NONE (R2R rules decide downstream effects)");
            }
            AdmissionDecision::Hold(_) | AdmissionDecision::Reject(_) => {
                println!("  emits=no governance-active evidence");
            }
        }
        println!();
    }
}
