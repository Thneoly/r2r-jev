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
async fn durable_mcp_validation_rejects_stale_allow_decision() -> anyhow::Result<()> {
    let store_path = temp_store_path("mcp-enforcement");
    let mut command = Command::new(env!("CARGO_BIN_EXE_r2r-mcp"));
    command.env("R2R_STORE_PATH", &store_path);

    let client = ()
        .serve(TokioChildProcess::new(command)?)
        .await?;

    let tools = client.list_all_tools().await?;
    let validate_tool = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "r2r_validate_execution")
        .expect("durable mode should expose r2r_validate_execution");
    let annotations = validate_tool
        .annotations
        .as_ref()
        .expect("validation tool annotations");
    assert_eq!(annotations.read_only_hint, Some(true));
    assert_eq!(annotations.destructive_hint, Some(false));
    assert_eq!(annotations.idempotent_hint, Some(true));
    assert_eq!(annotations.open_world_hint, Some(false));

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
    assert_eq!(decision["decision"], "ALLOW");
    assert_eq!(decision["state_version"], "state-000000");
    let decision_id = decision["decision_id"]
        .as_str()
        .expect("decision id")
        .to_string();

    let valid = client
        .call_tool(
            CallToolRequestParams::new("r2r_validate_execution").with_arguments(object!({
                "decision_id": decision_id.clone(),
                "action": "github.merge_pull_request",
                "expected_state_version": "state-000000"
            })),
        )
        .await?;
    let valid = text_json(&valid);
    assert_eq!(valid["allowed"], true);
    assert_eq!(valid["reason_code"], "EXECUTION_ALLOWED");
    assert_eq!(valid["decision_state_version"], "state-000000");
    assert_eq!(valid["current_state_version"], "state-000000");

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
    assert_eq!(observed["state_version"], "state-000001");

    let stale = client
        .call_tool(
            CallToolRequestParams::new("r2r_validate_execution").with_arguments(object!({
                "decision_id": decision_id,
                "action": "github.merge_pull_request",
                "expected_state_version": "state-000000"
            })),
        )
        .await?;
    let stale = text_json(&stale);
    assert_eq!(stale["allowed"], false);
    assert_eq!(stale["reason_code"], "STATE_VERSION_STALE");
    assert_eq!(stale["decision_state_version"], "state-000000");
    assert_eq!(stale["current_state_version"], "state-000001");

    client.cancel().await?;
    let _ = std::fs::remove_file(store_path);
    Ok(())
}
