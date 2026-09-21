mod jev;
mod model;
mod r2r;

use model::JudgmentObserved;
use r2r::{apply_judgment, print_report, GovernanceState};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "fixture".to_string());

    let event = match mode.as_str() {
        "fixture" => fixture_event(),
        "live" => {
            let mut task = "Fix the login redirect bug".to_string();
            let mut tool = "merge_pull_request".to_string();
            let mut scope = "repo-alpha".to_string();
            let mut intent = "Merge a pull request containing unrelated repository changes".to_string();
            let mut subject = "coder".to_string();

            while let Some(flag) = args.next() {
                let value = args.next().ok_or_else(|| format!("missing value for {flag}"))?;
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
            jev::judge_live(&api_key, &subject, &scope, &task, &tool, &intent)?
        }
        other => return Err(format!("unknown mode: {other}. Use `fixture` or `live`.")),
    };

    let before = GovernanceState::default();
    let mut after = GovernanceState::default();
    let report = apply_judgment(&mut after, &event);
    print_report(&event, &before, &after, &report);
    Ok(())
}

fn fixture_event() -> JudgmentObserved {
    JudgmentObserved::from_probabilities(
        "fixture:jev-style",
        "coder",
        "repo-alpha",
        "Fix the login redirect bug",
        "merge_pull_request",
        "Merge a pull request containing unrelated repository changes",
        0.94,
        0.72,
    )
}
