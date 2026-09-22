//! Baseline (direct-threshold persistence) vs Evidence Admission v0.1,
//! fed identical judgment traces, per the protocol in
//! `experiments/evidence-admission-v0.1/README.md`.
//!
//! Arms:
//!   baseline   score >= threshold -> suspend authorization until repair
//!   admission  Evidence Admission v0.1 -> only Accept suspends; Hold does not
//!
//! The falsification direction: admission should shrink the false-deny
//! blast radius of uncertain single-source judgments (EA-H1) without
//! losing history-sensitive enforcement for corroborated violations
//! (EA-H2). Traces also include a band where admission is expected to
//! change nothing (strong trusted false positive) — that is intentional.

#[path = "../admission.rs"]
#[allow(dead_code)]
mod admission;

use admission::{admit, AdmissionContext, AdmissionDecision, EvidenceKind};
use std::{env, fs, process};

const BASELINE_THRESHOLD_PPM: u32 = 850_000;

struct Row {
    step: u32,
    incident: String,
    kind: String,
    confidence_ppm: u32,
    reliability_ppm: u32,
    corroborators: u8,
    observed_vtick: u64,
    expires_vtick: u64,
    truth: String,
    #[allow(dead_code)]
    note: String,
}

#[derive(Default)]
struct Metrics {
    tool_calls: u32,
    allowed: u32,
    denied: u32,
    /// benign calls denied under a suspension caused by a false positive
    /// (the temporal blast radius metric).
    false_deny_calls: u32,
    /// benign calls denied under a suspension caused by a true violation
    /// (legitimate enforcement, reported separately on purpose).
    enforcement_denied: u32,
    /// true-violation calls allowed through.
    violation_allowed: u32,
    suspensions: u32,
    false_suspensions: u32,
    effective_repairs: u32,
    held_total: u32,
    held_resolved_accept: u32,
    held_resolved_expired: u32,
    held_pending: u32,
    provenance_links: u32,
}

fn parse_rows(path: &str) -> Result<Vec<Row>, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut rows = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        if idx == 0 || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.splitn(10, ',').collect();
        if cols.len() != 10 {
            return Err(format!("bad CSV row {idx}: {line}"));
        }
        let parse = |v: &str, what: &str| -> Result<u64, String> {
            v.parse()
                .map_err(|_| format!("bad {what} in row {idx}: {v}"))
        };
        rows.push(Row {
            step: parse(cols[0], "step")? as u32,
            incident: cols[1].to_string(),
            kind: cols[2].to_string(),
            confidence_ppm: parse(cols[3], "confidence_ppm")? as u32,
            reliability_ppm: parse(cols[4], "reliability_ppm")? as u32,
            corroborators: parse(cols[5], "corroborators")? as u8,
            observed_vtick: parse(cols[6], "observed_vtick")?,
            expires_vtick: parse(cols[7], "expires_vtick")?,
            truth: cols[8].to_string(),
            note: cols[9].to_string(),
        });
    }
    Ok(rows)
}

fn run_baseline(rows: &[Row], log: &mut Vec<String>) -> Metrics {
    let mut m = Metrics::default();
    let mut suspended_by: Option<String> = None; // incident truth that caused the suspension
    for r in rows {
        if r.kind == "repair" {
            if suspended_by.is_some() {
                m.effective_repairs += 1;
                m.provenance_links += 2; // event + authorization transition
            }
            suspended_by = None;
            log.push(format!(
                "{:<4} {:<6} {:<14} {:<9} {:<9}",
                r.step, r.incident, "-", "repair", "repair"
            ));
            continue;
        }
        m.tool_calls += 1;
        m.provenance_links += 2; // event -> decision
        let risky = r.confidence_ppm >= BASELINE_THRESHOLD_PPM;
        let just_suspended = risky && suspended_by.is_none();
        if just_suspended {
            m.suspensions += 1;
            m.provenance_links += 1;
            let cause = r.truth.clone();
            if cause == "false_positive" {
                m.false_suspensions += 1;
            }
            suspended_by = Some(cause);
        }
        let allowed = suspended_by.is_none();
        if allowed {
            m.allowed += 1;
            if r.truth == "violation" {
                m.violation_allowed += 1;
            }
        } else {
            m.denied += 1;
            if r.truth == "benign" {
                if suspended_by.as_deref() == Some("false_positive") {
                    m.false_deny_calls += 1;
                } else {
                    m.enforcement_denied += 1;
                }
            }
        }
        log.push(format!(
            "{:<4} {:<6} {:<14} {:<9} {:<9}",
            r.step,
            r.incident,
            format!("{}k", r.confidence_ppm / 1000),
            if allowed { "ALLOW" } else { "DENY" },
            if just_suspended { "SUSPEND" } else { "-" },
        ));
    }
    m
}

fn run_admission(rows: &[Row], log: &mut Vec<String>) -> Metrics {
    let mut m = Metrics::default();
    let mut suspended_by: Option<String> = None;
    // pending holds: (incident, expires_vtick)
    let mut pending: Vec<(String, u64)> = Vec::new();
    for r in rows {
        if r.kind == "repair" {
            if suspended_by.is_some() {
                m.effective_repairs += 1;
                m.provenance_links += 2;
            }
            suspended_by = None;
            log.push(format!(
                "{:<4} {:<6} {:<14} {:<9} {:<9}",
                r.step, r.incident, "-", "repair", "repair"
            ));
            continue;
        }
        m.tool_calls += 1;
        let decision = admit(
            EvidenceKind::BeyondScope,
            r.confidence_ppm,
            AdmissionContext::new(
                r.reliability_ppm,
                r.corroborators,
                r.observed_vtick,
                r.expires_vtick,
            ),
        );

        // Resolve pending holds for this incident.
        pending.retain(|(incident, expires)| {
            if incident != &r.incident {
                return true;
            }
            if decision.is_accepted() {
                m.held_resolved_accept += 1;
                return false;
            }
            if r.observed_vtick > *expires {
                m.held_resolved_expired += 1;
                return false;
            }
            true
        });

        if let AdmissionDecision::Hold(_) = decision {
            m.held_total += 1;
            pending.push((r.incident.clone(), r.expires_vtick));
        }

        if decision.is_accepted() && suspended_by.is_none() {
            m.suspensions += 1;
            let cause = r.truth.clone();
            if cause == "false_positive" {
                m.false_suspensions += 1;
            }
            suspended_by = Some(cause);
        }

        let allowed = suspended_by.is_none();
        // event -> evidence -> admission -> decision (+ authorization when it fires)
        m.provenance_links += if decision.is_accepted() { 5 } else { 4 };
        if allowed {
            m.allowed += 1;
            if r.truth == "violation" {
                m.violation_allowed += 1;
            }
        } else {
            m.denied += 1;
            if r.truth == "benign" {
                if suspended_by.as_deref() == Some("false_positive") {
                    m.false_deny_calls += 1;
                } else {
                    m.enforcement_denied += 1;
                }
            }
        }
        let verdict = decision.render();
        let cell = if allowed {
            format!("{verdict}/ALLOW")
        } else {
            format!("{verdict}/DENY")
        };
        log.push(format!(
            "{:<4} {:<6} {:<14} {:<9} {:<9} {}",
            r.step,
            r.incident,
            format!("{}k", r.confidence_ppm / 1000),
            "-",
            "-",
            cell,
        ));
    }
    m.held_pending = pending.len() as u32;
    m
}

fn report(path: &str, rows: &[Row]) -> String {
    let mut out = String::new();
    out.push_str(&format!("scenario: {path}\n"));
    out.push_str(&format!(
        "baseline threshold: >= {} ppm | admission: policy v0.1\n\n",
        BASELINE_THRESHOLD_PPM
    ));
    out.push_str(&format!(
        "{:<4} {:<6} {:<14} {:<9} {:<9} {}\n",
        "step", "inc", "conf", "baseline", "final", "admission"
    ));
    out.push_str(&"-".repeat(76));
    out.push('\n');
    let mut log_b = Vec::new();
    let mut log_a = Vec::new();
    let base = run_baseline(rows, &mut log_b);
    let adm = run_admission(rows, &mut log_a);
    for (b, a) in log_b.iter().zip(log_a.iter()) {
        // merge: step/inc/conf from baseline line; admission verdict from the admission line tail
        let tail = a.trim_end().rsplit_once(' ').map(|(_, t)| t).unwrap_or("-");
        out.push_str(&format!("{b} {tail}\n"));
    }
    out.push_str("\nsummary\n");
    for (name, m) in [("baseline", &base), ("admission", &adm)] {
        out.push_str(&format!(
            "  {name:<9}: calls={} allowed={} denied={} | false_deny_calls={} enforcement_denied={} violation_allowed={} | suspensions={} (false {}) effective_repairs={} | links={}\n",
            m.tool_calls,
            m.allowed,
            m.denied,
            m.false_deny_calls,
            m.enforcement_denied,
            m.violation_allowed,
            m.suspensions,
            m.false_suspensions,
            m.effective_repairs,
            m.provenance_links,
        ));
    }
    out.push_str(&format!(
        "  held      : total={} resolved_accept={} resolved_expired={} pending={}\n",
        adm.held_total, adm.held_resolved_accept, adm.held_resolved_expired, adm.held_pending
    ));
    out
}

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: cargo run --bin admission-compare -- <trace.csv>");
        process::exit(2);
    });
    let rows = parse_rows(&path).unwrap_or_else(|e| {
        eprintln!("{e}");
        process::exit(2);
    });
    print!("{}", report(&path, &rows));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(name: &str) -> Vec<Row> {
        parse_rows(&format!(
            "experiments/evidence-admission-v0.1/traces/{name}"
        ))
        .expect("trace file readable (run from repository root)")
    }

    fn both(name: &str) -> (Metrics, Metrics) {
        let mut lb = Vec::new();
        let mut la = Vec::new();
        let b = run_baseline(&trace(name), &mut lb);
        let a = run_admission(&trace(name), &mut la);
        (b, a)
    }

    #[test]
    fn ea_h1_uncertain_single_source_admission_shrinks_false_deny() {
        let (b, a) = both("uncertain-single-source.csv");
        assert_eq!(b.false_deny_calls, 2, "baseline inherits the blast radius");
        assert_eq!(a.false_deny_calls, 0, "admission holds the uncertain flag");
        assert_eq!(a.suspensions, 0);
        assert_eq!(a.held_total, 1);
        assert_eq!(a.effective_repairs, 0, "nothing to repair");
    }

    #[test]
    fn strong_trusted_false_positive_unchanged_honest_limit() {
        let (b, a) = both("strong-trusted-false-positive.csv");
        assert_eq!(b.false_deny_calls, 1);
        assert_eq!(a.false_deny_calls, 1, "admission does not save this band");
        assert_eq!(a.suspensions, b.suspensions);
    }

    #[test]
    fn ea_h2_corroborated_violation_stops_earlier() {
        let (_b, a) = both("corroborated-violation.csv");
        assert!(a.violation_allowed <= 1, "corroboration suspends early");
        assert_eq!(
            a.held_resolved_accept, 1,
            "the held signal resolved via corroboration"
        );
        assert_eq!(a.false_deny_calls, 0);
    }

    #[test]
    fn hold_lifecycle_resolves_by_expiry_or_corroboration() {
        let (_b, a) = both("hold-lifecycle.csv");
        assert_eq!(a.held_resolved_accept, 1);
        assert_eq!(a.held_resolved_expired, 1);
        assert_eq!(a.held_pending, 1, "the last held evidence stays pending");
    }

    #[test]
    fn replay_is_deterministic() {
        for name in [
            "uncertain-single-source.csv",
            "strong-trusted-false-positive.csv",
            "corroborated-violation.csv",
            "hold-lifecycle.csv",
        ] {
            let rows = trace(name);
            assert_eq!(
                report(name, &rows),
                report(name, &rows),
                "two runs must be byte-identical: {name}"
            );
        }
    }
}
