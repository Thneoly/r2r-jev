// The demo binary exercises a subset of the admission module's public
// surface; the reference-case and compare binaries exercise the rest.
#[allow(dead_code)]
mod admission;
mod jev;
mod model;
mod r2r;

use admission::AdmissionContext;
use model::{ppm_to_display, JudgmentObserved};
use r2r::{Admission, Governance};

const FIXTURE_PATH: &str = "demo/fixtures/coding-agent.json";

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "fixture".to_string());

    match mode.as_str() {
        "fixture" => {
            let mut provider_name = "jev".to_string();
            while let Some(flag) = args.next() {
                let value = args
                    .next()
                    .ok_or_else(|| format!("missing value for {flag}"))?;
                if flag == "--provider" {
                    provider_name = value;
                } else {
                    return Err(format!("unknown argument: {flag}"));
                }
            }
            run_fixture_demo(&provider_name)
        }
        "live" => run_live(&mut args),
        other => Err(format!("unknown mode: {other}. Use `fixture` or `live`.")),
    }
}

/// Full three-act demo:
/// judgment -> typed evidence -> Admission v0.1 -> R2R -> persistent state -> repair.
fn run_fixture_demo(provider_name: &str) -> Result<(), String> {
    let mut governance = Governance::new();
    let fixture = load_fixture(provider_name)?;
    let risky = fixture.risky();
    let outcome = governance.observe_judgment(&risky, AdmissionContext::demo(1));
    println!("Act 1: admission separates evidence from authority");
    print_judgment(&risky, &outcome);
    print_changes(&outcome);
    print_decision(&outcome);

    // Act 2: a later, low-scoring call produces no admitted evidence, but the
    // authorization state created by Act 1 still governs the call.
    let benign = fixture.benign();
    let outcome = governance.observe_judgment(&benign, AdmissionContext::demo(2));
    println!();
    println!("Act 2: rejected evidence does not erase persistent governance state");
    print_judgment(&benign, &outcome);
    if outcome.relation_changes.is_empty() {
        println!("  Relations         unchanged (no new evidence was admitted)");
    } else {
        print_changes(&outcome);
    }
    print_decision(&outcome);

    // Act 3: a human repairs the relation; the repair is supervised.
    let outcome = governance.human_override("human-1", "merge_pull_request");
    println!();
    println!("Act 3: human override repairs the relation, under supervision");
    println!(
        "  GovernanceEvent   event={} kind=human_override supervisor=human-1",
        outcome.event_id
    );
    print_changes(&outcome);
    print_decision(&outcome);

    println!();
    println!("Provenance");
    for line in governance.provenance() {
        println!("  {line}");
    }

    Ok(())
}

/// Live mode covers a single judgment against the real Jev API, but still
/// routes it through the same deterministic Admission v0.1 boundary.
fn run_live(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let mut task = "Fix the login redirect bug".to_string();
    let mut tool = "merge_pull_request".to_string();
    let mut scope = "repo-alpha".to_string();
    let mut intent = "Merge a pull request containing unrelated repository changes".to_string();
    let mut subject = "coder".to_string();
    let mut source_reliability_ppm: u32 = 850_000;
    let mut corroborators: u8 = 0;
    let mut provider_name = "jev".to_string();

    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--task" => task = value,
            "--tool" => tool = value,
            "--scope" => scope = value,
            "--intent" => intent = value,
            "--subject" => subject = value,
            "--source-reliability-ppm" => {
                source_reliability_ppm = value
                    .parse::<u32>()
                    .map_err(|_| "--source-reliability-ppm must be an integer".to_string())?;
                if source_reliability_ppm > 1_000_000 {
                    return Err("--source-reliability-ppm must be <= 1000000".to_string());
                }
            }
            "--corroborators" => {
                corroborators = value
                    .parse::<u8>()
                    .map_err(|_| "--corroborators must be an integer 0..255".to_string())?;
            }
            "--provider" => provider_name = value,
            _ => return Err(format!("unknown argument: {flag}")),
        }
    }

    let provider = jev::JudgeProvider::by_name(&provider_name)?;
    let api_key = std::env::var(provider.api_key_env).map_err(|_| {
        format!(
            "{} is required for live mode with provider {:?}",
            provider.api_key_env, provider_name
        )
    })?;
    let judgment = jev::judge_live(&provider, &api_key, &subject, &scope, &task, &tool, &intent)?;

    // This is configured trust metadata, not a probability returned by Jev and
    // not a claim about measured model accuracy.
    let context = AdmissionContext::new(source_reliability_ppm, corroborators, 1, 11);

    let mut governance = Governance::new();
    let outcome = governance.observe_judgment(&judgment, context);
    println!(
        "Admission context source_reliability_ppm={} corroborators={} (caller-configured)",
        source_reliability_ppm, corroborators
    );
    print_judgment(&judgment, &outcome);
    print_changes(&outcome);
    print_decision(&outcome);
    Ok(())
}

#[derive(serde::Deserialize)]
struct BenignFixture {
    tool: String,
    intent: String,
    beyond_scope: f64,
    destructive: f64,
}

#[derive(serde::Deserialize)]
struct FixtureFile {
    provider: String,
    subject: String,
    scope: String,
    task: String,
    tool: String,
    intent: String,
    beyond_scope: f64,
    destructive: f64,
    benign: Option<BenignFixture>,
}

impl FixtureFile {
    fn risky(&self) -> JudgmentObserved {
        JudgmentObserved::from_probabilities(
            &self.provider,
            &self.subject,
            &self.scope,
            &self.task,
            &self.tool,
            &self.intent,
            self.beyond_scope,
            self.destructive,
        )
    }

    /// The Act 2 judgment. Fixtures may carry a recorded benign follow-up;
    /// older fixtures fall back to the original recorded values.
    fn benign(&self) -> JudgmentObserved {
        let fallback_tool = "read_file";
        let fallback_intent = "Read a source file in the same repository";
        match &self.benign {
            Some(b) => JudgmentObserved::from_probabilities(
                &self.provider,
                &self.subject,
                &self.scope,
                &self.task,
                &b.tool,
                &b.intent,
                b.beyond_scope,
                b.destructive,
            ),
            None => JudgmentObserved::from_probabilities(
                &self.provider,
                &self.subject,
                &self.scope,
                &self.task,
                fallback_tool,
                fallback_intent,
                0.18,
                0.12,
            ),
        }
    }
}

/// Recorded judgment sets used by the fixture demo. `jev` is the original
/// recording; `d3` was recorded live from the vLLM Semantic Router free API
/// on 2026-10-24 — same scenario, a second judge, a different admission
/// outcome. Run from the repository root.
fn fixture_path(provider_name: &str) -> Result<String, String> {
    match provider_name {
        "jev" => Ok(FIXTURE_PATH.to_string()),
        "d3" => Ok("demo/fixtures/coding-agent-d3.json".to_string()),
        other => Err(format!(
            "unknown fixture provider {other:?}; available: jev, d3"
        )),
    }
}

fn load_fixture(provider_name: &str) -> Result<FixtureFile, String> {
    let path = fixture_path(provider_name)?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {path}: {e} (run from the repository root)"))?;
    serde_json::from_str(&raw).map_err(|e| format!("invalid fixture {path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use admission::AdmissionDecision;
    use r2r::Decision;

    #[test]
    fn jev_fixture_admits_the_strong_signal() {
        let fixture = load_fixture("jev").expect("jev fixture");
        let mut governance = Governance::new();
        let outcome = governance.observe_judgment(&fixture.risky(), AdmissionContext::demo(1));
        assert_eq!(outcome.decision, Decision::Deny);
    }

    #[test]
    fn d3_fixture_holds_the_uncertain_signal_same_scenario() {
        let fixture = load_fixture("d3").expect("d3 fixture");
        assert!((fixture.beyond_scope - 0.702451).abs() < 1e-9);

        let mut governance = Governance::new();
        let outcome = governance.observe_judgment(&fixture.risky(), AdmissionContext::demo(1));

        // Same scenario, a more cautious judge: both signals land in the Hold
        // band, nothing is admitted, the call is allowed and nothing persists.
        assert!(outcome
            .evidence
            .iter()
            .all(|e| matches!(e.decision, AdmissionDecision::Hold(_))));
        assert_eq!(outcome.decision, Decision::Allow);
        assert!(outcome.relation_changes.is_empty());
    }

    #[test]
    fn unknown_fixture_provider_fails_closed() {
        assert!(fixture_path("bogus").is_err());
    }
}

fn print_judgment(judgment: &JudgmentObserved, outcome: &Admission) {
    println!(
        "  JudgmentObserved  event={} provider={}",
        outcome.event_id, judgment.provider
    );
    println!(
        "  judgment          beyond_scope={} destructive={} tool={}",
        ppm_to_display(judgment.beyond_scope_ppm),
        ppm_to_display(judgment.destructive_ppm),
        judgment.tool
    );
    for evidence in &outcome.evidence {
        println!(
            "  Evidence          {} kind={} confidence={}",
            evidence.id,
            evidence.kind.as_str(),
            ppm_to_display(evidence.confidence_ppm)
        );
        println!(
            "  Admission         policy={} {}",
            evidence.policy_version,
            evidence.decision.render()
        );
    }
}

fn print_changes(outcome: &Admission) {
    for change in &outcome.relation_changes {
        println!(
            "  {:<17} {:<29} {} (caused by {})",
            change.relation, change.transition, change.id, change.caused_by
        );
    }
}

fn print_decision(outcome: &Admission) {
    println!(
        "  Decision          {} {} ({})",
        outcome.decision.as_str(),
        outcome.action,
        outcome.reason
    );
}
