use rmcp::{
    model::CallToolRequestParams,
    object,
    transport::TokioChildProcess,
    ServiceExt,
};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

fn text_json(result: &rmcp::model::CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .expect("tool should return text content");
    serde_json::from_str(&text.text).expect("tool text should contain JSON")
}

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

#[tokio::test]
async fn stdio_server_runs_observe_decide_explain_outcome_and_replay() -> anyhow::Result<()> {
    let client = ()
        .serve(TokioChildProcess::new(Command::new(env!("CARGO_BIN_EXE_r2r-mcp")))?)
        .await?;

    let tools = client.list_all_tools().await?;
    let mut names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "r2r_decide".to_string(),
            "r2r_explain".to_string(),
            "r2r_observe".to_string(),
            "r2r_record_outcome".to_string(),
            "r2r_replay".to_string(),
        ]
    );

    let observed = client
        .call_tool(
            CallToolRequestParams::new("r2r_observe").with_arguments(object!({
                "provider": "fixture:jev-style",
                "subject": "agent:coder-1",
                "scope": "repo:alpha",
                "task": "fix login redirect",
                "action": "github.merge_pull_request",
                "intent": "merge unrelated changes",
                "beyond_scope_ppm": 940000,
                "destructive_ppm": 720000
            })),
        )
        .await?;
    let observed = text_json(&observed);
    assert_eq!(observed["event_id"], "event-000001");
    assert_eq!(observed["state_version"], "state-000001");
    assert!(observed["relation_transitions"]
        .as_array()
        .expect("transitions")
        .iter()
        .any(|value| value.as_str().is_some_and(|s| s.contains("Authorization:Active -> Suspended"))));

    let decision = client
        .call_tool(
            CallToolRequestParams::new("r2r_decide").with_arguments(object!({
                "subject": "agent:coder-1",
                "scope": "repo:alpha",
                "action": "github.merge_pull_request",
                "resource": "pr:42",
                "task": "fix login redirect"
            })),
        )
        .await?;
    let decision = text_json(&decision);
    assert_eq!(decision["decision"], "DENY");
    assert_eq!(decision["reason_code"], "AUTHORIZATION_SUSPENDED");
    let decision_id = decision["decision_id"]
        .as_str()
        .expect("decision id")
        .to_string();

    let explanation = client
        .call_tool(
            CallToolRequestParams::new("r2r_explain").with_arguments(object!({
                "decision_id": decision_id.clone()
            })),
        )
        .await?;
    let explanation = text_json(&explanation);
    assert_eq!(explanation["verdict"], "DENY");
    assert!(explanation["causal_chain"]
        .as_array()
        .expect("causal chain")
        .iter()
        .any(|value| value.as_str().is_some_and(|s| s.contains("event-000001"))));

    let outcome = client
        .call_tool(
            CallToolRequestParams::new("r2r_record_outcome").with_arguments(object!({
                "decision_id": decision_id,
                "outcome": "blocked",
                "detail": "enforcement adapter blocked execution"
            })),
        )
        .await?;
    let outcome = text_json(&outcome);
    assert_eq!(outcome["outcome_id"], "outcome-000001");
    assert_eq!(outcome["outcome"], "blocked");
    assert_eq!(outcome["state_version"], "state-000001");
    assert_eq!(outcome["relation_transitions"].as_array().unwrap().len(), 0);

    let replay = client
        .call_tool(
            CallToolRequestParams::new("r2r_replay").with_arguments(object!({
                "subject": "agent:coder-1",
                "scope": "repo:alpha"
            })),
        )
        .await?;
    let replay = text_json(&replay);
    assert_eq!(replay["replay_match"], true);
    assert_eq!(replay["recorded_state_version"], "state-000001");
    assert_eq!(replay["replayed_state_version"], "state-000001");
    assert_eq!(replay["replayed_events"], 1);
    assert!(replay["first_divergent_event"].is_null());

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn stdio_server_recovers_governance_state_across_process_restart() -> anyhow::Result<()> {
    let store_path = temp_store_path("restart");

    let mut first_command = Command::new(env!("CARGO_BIN_EXE_r2r-mcp"));
    first_command.env("R2R_STORE_PATH", &store_path);
    let first = ()
        .serve(TokioChildProcess::new(first_command)?)
        .await?;

    let observed = first
        .call_tool(
            CallToolRequestParams::new("r2r_observe").with_arguments(object!({
                "provider": "fixture:jev-style",
                "subject": "agent:coder-1",
                "scope": "repo:alpha",
                "task": "fix login redirect",
                "action": "github.merge_pull_request",
                "intent": "merge unrelated changes",
                "beyond_scope_ppm": 940000,
                "destructive_ppm": 720000
            })),
        )
        .await?;
    let observed = text_json(&observed);
    assert_eq!(observed["state_version"], "state-000001");
    first.cancel().await?;

    let mut second_command = Command::new(env!("CARGO_BIN_EXE_r2r-mcp"));
    second_command.env("R2R_STORE_PATH", &store_path);
    let second = ()
        .serve(TokioChildProcess::new(second_command)?)
        .await?;

    let decision = second
        .call_tool(
            CallToolRequestParams::new("r2r_decide").with_arguments(object!({
                "subject": "agent:coder-1",
                "scope": "repo:alpha",
                "action": "github.merge_pull_request",
                "resource": "pr:42",
                "task": "fix login redirect"
            })),
        )
        .await?;
    let decision = text_json(&decision);
    assert_eq!(decision["decision"], "DENY");
    assert_eq!(decision["state_version"], "state-000001");

    let replay = second
        .call_tool(
            CallToolRequestParams::new("r2r_replay").with_arguments(object!({
                "subject": "agent:coder-1",
                "scope": "repo:alpha"
            })),
        )
        .await?;
    let replay = text_json(&replay);
    assert_eq!(replay["replay_match"], true);
    assert_eq!(replay["recorded_state_version"], "state-000001");
    assert_eq!(replay["replayed_state_version"], "state-000001");

    second.cancel().await?;
    let _ = std::fs::remove_file(store_path);
    Ok(())
}
