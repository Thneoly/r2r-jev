use r2r_jev::store::json_file::JsonFileEventStore;
use r2r_jev::store::{DomainKey, EventStore, StoredDecision};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_store_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "r2r-jev-{label}-{}-{nanos}.json",
        std::process::id()
    ))
}

fn run_enforce(path: &PathBuf, decision_id: &str, state_version: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_r2r-enforce"))
        .args([
            "--store",
            path.to_str().expect("utf8 path"),
            "--decision",
            decision_id,
            "--action",
            "github.merge_pull_request",
            "--state-version",
            state_version,
        ])
        .output()
        .expect("run r2r-enforce")
}

fn json_stdout(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("JSON stdout")
}

#[test]
fn cli_allows_current_decision_then_rejects_it_after_state_change() {
    let path = temp_store_path("enforcement-cli");
    let domain = DomainKey::new("agent:coder-1", "repo:alpha");
    let decision_id;

    {
        let mut store = JsonFileEventStore::open(&path).expect("open store");
        let state_version = store.advance_state_version(&domain);
        assert_eq!(state_version, "state-000001");
        decision_id = store.next_decision_id();
        store.record_decision(StoredDecision {
            decision_id: decision_id.clone(),
            domain: domain.clone(),
            action: "github.merge_pull_request".to_string(),
            verdict: "ALLOW".to_string(),
            reason_code: "AUTHORIZATION_ACTIVE".to_string(),
            governing_relation_id: None,
            state_version: state_version.clone(),
            provenance: vec!["test decision".to_string()],
        });
        assert!(store.health().is_ok());
    }

    let allowed = run_enforce(&path, &decision_id, "state-000001");
    assert!(allowed.status.success());
    let allowed_json = json_stdout(&allowed);
    assert_eq!(allowed_json["allowed"], true);
    assert_eq!(allowed_json["reason_code"], "EXECUTION_ALLOWED");

    {
        let mut store = JsonFileEventStore::open(&path).expect("reopen store");
        assert_eq!(store.advance_state_version(&domain), "state-000002");
        let later_decision_id = store.next_decision_id();
        store.record_decision(StoredDecision {
            decision_id: later_decision_id,
            domain: domain.clone(),
            action: "read_file".to_string(),
            verdict: "DENY".to_string(),
            reason_code: "AUTHORIZATION_SUSPENDED".to_string(),
            governing_relation_id: Some("authorization-0002".to_string()),
            state_version: "state-000002".to_string(),
            provenance: vec!["state advanced".to_string()],
        });
        assert!(store.health().is_ok());
    }

    let stale = run_enforce(&path, &decision_id, "state-000001");
    assert_eq!(stale.status.code(), Some(3));
    let stale_json = json_stdout(&stale);
    assert_eq!(stale_json["allowed"], false);
    assert_eq!(stale_json["reason_code"], "STATE_VERSION_STALE");
    assert_eq!(stale_json["decision_state_version"], "state-000001");
    assert_eq!(stale_json["current_state_version"], "state-000002");

    let _ = std::fs::remove_file(path);
}
