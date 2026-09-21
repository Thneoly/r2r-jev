mod jev;
mod model;
mod r2r;

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

/// The full three-act demo, driven by the recorded fixture plus a benign
/// follow-up call: judgment -> evidence -> persistent state -> repair.
fn run_fixture_demo() -> Result<(), String> {
    let mut governance = Governance::new();

    // Act 1: the judgment itself.
    let risky = load_fixture_judgment()?;
    let admission = governance.observe_judgment(&risky);
    println!("Act 1: a judgment becomes evidence, not permission");
    print_judgment(&risky, &admission);
    print_changes(&admission);
    print_decision(&admission);

    // Act 2: a later, low-scoring call inherits the history.
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
    let admission = governance.observe_judgment(&benign);
    println!();
    println!("Act 2: the next call inherits history");
    print_judgment(&benign, &admission);
    if admission.relation_changes.is_empty() {
        println!("  Relations         unchanged (a stateless gate would ALLOW this call)");
    } else {
        print_changes(&admission);
    }
    print_decision(&admission);

    // Act 3: a human repairs the relation; the repair is supervised.
    let admission = governance.human_override("human-1", "merge_pull_request");
    println!();
    println!("Act 3: human override repairs the relation, under supervision");
    println!(
        "  GovernanceEvent   event={} kind=human_override supervisor=human-1",
        admission.event_id
    );
    print_changes(&admission);
    print_decision(&admission);

    println!();
    println!("Provenance");
    for line in governance.provenance() {
        println!("  {line}");
    }

    Ok(())
}

/// Live mode covers a single judgment (Act 1) against the real Jev API.
fn run_live(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let mut task = "Fix the login redirect bug".to_string();
    let mut tool = "merge_pull_request".to_string();
    let mut scope = "repo-alpha".to_string();
    let mut intent = "Merge a pull request containing unrelated repository changes".to_string();
    let mut subject = "coder".to_string();

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
            _ => return Err(format!("unknown argument: {flag}")),
        }
    }

    let api_key = std::env::var("TYPESAFE_API_KEY")
        .map_err(|_| "TYPESAFE_API_KEY is required for live mode".to_string())?;
    let judgment = jev::judge_live(&api_key, &subject, &scope, &task, &tool, &intent)?;

    let mut governance = Governance::new();
    let admission = governance.observe_judgment(&judgment);
    print_judgment(&judgment, &admission);
    print_changes(&admission);
    print_decision(&admission);
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

fn print_judgment(judgment: &JudgmentObserved, admission: &Admission) {
    println!(
        "  JudgmentObserved  event={} provider={}",
        admission.event_id, judgment.provider
    );
    println!(
        "  judgment          beyond_scope={} destructive={} tool={}",
        ppm_to_display(judgment.beyond_scope_ppm),
        ppm_to_display(judgment.destructive_ppm),
        judgment.tool
    );
    if let Some(evidence_id) = &admission.evidence_id {
        println!("  Evidence          {evidence_id} created");
    }
}

fn print_changes(admission: &Admission) {
    for change in &admission.relation_changes {
        println!(
            "  {:<17} {:<29} {} (caused by {})",
            change.relation, change.transition, change.id, change.caused_by
        );
    }
}

fn print_decision(admission: &Admission) {
    println!(
        "  Decision          {} {} ({})",
        admission.decision.as_str(),
        admission.action,
        admission.reason
    );
}
