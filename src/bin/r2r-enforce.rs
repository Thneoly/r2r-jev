use r2r_jev::enforcement::{validate_execution, ExecutionBindingRequest};
use r2r_jev::store::json_file::JsonFileEventStore;
use serde::Serialize;
use std::process::ExitCode;

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct Args {
    store: String,
    decision_id: String,
    action: String,
    state_version: String,
}

fn usage() -> &'static str {
    "usage: r2r-enforce --store <path> --decision <decision-id> --action <action> --state-version <state-version>"
}

fn parse_args() -> Result<Args, String> {
    let mut store = None;
    let mut decision_id = None;
    let mut action = None;
    let mut state_version = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        let value = match arg.as_str() {
            "--store" | "--decision" | "--action" | "--state-version" => args
                .next()
                .ok_or_else(|| format!("missing value for {arg}; {}", usage()))?,
            "--help" | "-h" => return Err(usage().to_string()),
            other => return Err(format!("unknown argument {other}; {}", usage())),
        };

        match arg.as_str() {
            "--store" => store = Some(value),
            "--decision" => decision_id = Some(value),
            "--action" => action = Some(value),
            "--state-version" => state_version = Some(value),
            _ => unreachable!(),
        }
    }

    Ok(Args {
        store: store.ok_or_else(|| format!("--store is required; {}", usage()))?,
        decision_id: decision_id.ok_or_else(|| format!("--decision is required; {}", usage()))?,
        action: action.ok_or_else(|| format!("--action is required; {}", usage()))?,
        state_version: state_version
            .ok_or_else(|| format!("--state-version is required; {}", usage()))?,
    })
}

fn render<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "{\"error\":\"serialization failure\"}".to_string())
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            println!("{}", render(&ErrorResponse { error }));
            return ExitCode::from(2);
        }
    };

    let store = match JsonFileEventStore::open(&args.store) {
        Ok(store) => store,
        Err(error) => {
            println!("{}", render(&ErrorResponse { error }));
            return ExitCode::from(2);
        }
    };

    let request = ExecutionBindingRequest::new(args.decision_id, args.action, args.state_version);
    let result = match validate_execution(&store, &request) {
        Ok(result) => result,
        Err(error) => {
            println!("{}", render(&ErrorResponse { error }));
            return ExitCode::from(2);
        }
    };

    println!("{}", render(&result));
    if result.allowed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}
