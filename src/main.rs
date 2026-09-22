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
        "fixture" => run_fixture_demo(),
        "live" => run_live(&mut args),
        other => Err(format!("unknown mode: {other}. Use `fixture` or `live`.")),
    }
}

/// Full three-act demo:
/// judgment -> typed evidence -> Admission v0.1 -> R2R -> persistent state -> repair.
fn run_fixture_demo() -> Result<(), String> {
    let mut governance = Governance::new();

    // Act 1: one strong signal is admitted; the second is held.
    let risky = load_fixture_judgment()?;
    let outcome = governance.observe_judgment(&risky, AdmissionContext::demo(1));
    println!("Act 1: admission separates evidence from authority");
    print_judgment(&risky, &outcome);
    print_changes(&outcome);
    print_decision(&outcome);

    // Act 2: a later, low-scoring call produces no admitted evidence, but the
    // authorization state created by Act 1 still governs the call.
    let benign = JudgmentObserved::from_probabilities(
        "fixture:jev-style",
        "coder",
        "repo-alpha",
        "Fix the login redirect bug",
        "read_file",
        "Read a source file in the same repository",
        0.18,
        0.12,
    );
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
            _ => return Err(format!("unknown argument: {flag}")),
        }
    }

    let api_key = std::env::var("TYPESAFE_API_KEY")
        .map_err(|_| "TYPESAFE_API_KEY is required for live mode".to_string())?;
    let judgment = jev::judge_live(&api_key, &subject, &scope, &task, &tool, &intent)?;

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
struct FixtureFile {
    provider: String,
    subject: String,
    scope: String,
    task: String,
    tool: String,
    intent: String,
    beyond_scope: f64,
    destructive: f64,
}

/// The recorded judgment used by Act 1. Run from the repository root.
fn load_fixture_judgment() -> Result<JudgmentObserved, String> {
    let raw = std::fs::read_to_string(FIXTURE_PATH)
        .map_err(|e| format!("cannot read {FIXTURE_PATH}: {e} (run from the repository root)"))?;
    let fixture: FixtureFile =
        serde_json::from_str(&raw).map_err(|e| format!("invalid fixture {FIXTURE_PATH}: {e}"))?;

    Ok(JudgmentObserved::from_probabilities(
        fixture.provider,
        fixture.subject,
        fixture.scope,
        fixture.task,
        fixture.tool,
        fixture.intent,
        fixture.beyond_scope,
        fixture.destructive,
    ))
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
