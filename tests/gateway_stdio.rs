use rmcp::{model::CallToolRequestParams, object, transport::TokioChildProcess, ServiceExt};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

fn temp_config(label: &str) -> PathBuf {
    temp_config_with(label, false)
}

fn temp_config_with(label: &str, with_mock_upstream: bool) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "r2r-jev-gateway-{label}-{}-{nanos}.json",
        std::process::id()
    ));
    let upstream = env!("CARGO_BIN_EXE_r2r-mcp").replace('\\', "/");
    let mock = env!("CARGO_BIN_EXE_mock-upstream").replace('\\', "/");
    let extra_upstream = if with_mock_upstream {
        format!(
            r#",
                {{"name": "tools", "command": "{mock}"}}"#
        )
    } else {
        String::new()
    };
    let config = format!(
        r#"{{
            "upstreams": [
                {{"name": "control-plane", "command": "{upstream}", "control_plane": true}}{extra_upstream}
            ],
            "governance": {{"subject": "agent:coder-1", "scope": "repo:alpha"}},
            "init_timeout_ms": 120000
        }}"#
    );
    std::fs::write(&path, config).expect("write gateway config");
    path
}

async fn connect(
    label: &str,
) -> anyhow::Result<rmcp::service::RunningService<rmcp::RoleClient, ()>> {
    let config = temp_config(label);
    let mut command = Command::new(env!("CARGO_BIN_EXE_r2r-gateway"));
    command.arg(&config);
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    Ok(client)
}

fn text_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn gateway_proxies_catalog_and_allows_fresh_domain() -> anyhow::Result<()> {
    let client = connect("proxy").await?;

    let tools = client.list_all_tools().await?;
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    for expected in ["r2r_observe", "r2r_decide", "r2r_explain", "r2r_replay"] {
        assert!(
            names.iter().any(|n| n == expected),
            "gateway catalog missing {expected}; got {names:?}"
        );
    }

    let decision = client
        .call_tool(
            CallToolRequestParams::new("r2r_decide").with_arguments(object!({
                "subject": "agent:coder-1",
                "scope": "repo:alpha",
                "action": "github.merge_pull_request",
                "task": "fix login redirect"
            })),
        )
        .await?;
    let text = text_of(&decision);
    assert!(
        text.contains("ALLOW"),
        "fresh domain decide should ALLOW, got: {text}"
    );

    Ok(())
}

#[tokio::test]
async fn governance_suspension_blocks_after_beyond_scope_evidence() -> anyhow::Result<()> {
    let client = connect("gated").await?;

    // Step 1: observe strong beyond-scope evidence → Trust/Delegation/Authorization degrade.
    let observe = client
        .call_tool(
            CallToolRequestParams::new("r2r_observe").with_arguments(object!({
                "provider": "fixture:jev-style",
                "subject": "agent:coder-1",
                "scope": "repo:alpha",
                "task": "fix login redirect",
                "action": "github.merge_pull_request",
                "intent": "merge unrelated changes",
                "beyond_scope_ppm": 940000,
                "destructive_ppm": 720000,
                "source_reliability_ppm": 850000,
                "independent_corroborators": 0
            })),
        )
        .await?;
    let observe_text = text_of(&observe);
    assert!(
        observe_text.contains("Suspended") || observe_text.contains("suspended"),
        "strong beyond-scope evidence should suspend authorization, got: {observe_text}"
    );

    // Step 2: decide on the same domain → DENY with reason code.
    let decision = client
        .call_tool(
            CallToolRequestParams::new("r2r_decide").with_arguments(object!({
                "subject": "agent:coder-1",
                "scope": "repo:alpha",
                "action": "github.merge_pull_request"
            })),
        )
        .await?;
    let deny_text = text_of(&decision);
    assert!(
        deny_text.contains("DENY"),
        "after suspension, decide should DENY, got: {deny_text}"
    );
    assert!(
        deny_text.contains("AUTHORIZATION_SUSPENDED"),
        "deny should carry reason_code, got: {deny_text}"
    );

    Ok(())
}

#[tokio::test]
async fn gateway_fails_closed_on_unknown_tool() -> anyhow::Result<()> {
    let client = connect("unknown").await?;

    let result = client
        .call_tool(CallToolRequestParams::new("definitely_not_a_tool"))
        .await;

    match result {
        Err(_) => { /* transport-level: acceptable */ }
        Ok(response) => {
            assert!(
                response.is_error == Some(true),
                "unknown tool should fail closed with isError=true"
            );
        }
    }

    Ok(())
}

/// Full gating test with a real non-governance upstream: the gateway must
/// authorize a non-r2r_ tool call before forwarding, and deny it after
/// authorization is suspended.
#[tokio::test]
async fn gateway_gates_non_r2r_tool_against_real_upstream() -> anyhow::Result<()> {
    let config = temp_config_with("real-gate", true);
    let mut command = Command::new(env!("CARGO_BIN_EXE_r2r-gateway"));
    command.arg(&config);
    let client = ().serve(TokioChildProcess::new(command)?).await?;

    // The merged catalog should include both r2r_* tools AND "echo"
    // from the mock upstream.
    let tools = client.list_all_tools().await?;
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    assert!(
        names.iter().any(|n| n == "echo"),
        "catalog should include 'echo' from mock upstream; got {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "r2r_decide"),
        "catalog should include 'r2r_decide' from control plane; got {names:?}"
    );

    // Step 1: on a fresh domain, "echo" should be authorized (ALLOW)
    // and forwarded to the mock upstream.
    let echo_result = client
        .call_tool(CallToolRequestParams::new("echo").with_arguments(object!({
            "message": "hello through the gateway"
        })))
        .await?;
    let echo_text = text_of(&echo_result);
    assert!(
        echo_text.contains("echo: hello through the gateway"),
        "echo should be forwarded through the gateway, got: {echo_text}"
    );
    assert!(
        echo_result.is_error != Some(true),
        "fresh domain echo should not be an error"
    );

    // Step 2: observe strong beyond-scope evidence → authorization suspends.
    let observe = client
        .call_tool(
            CallToolRequestParams::new("r2r_observe").with_arguments(object!({
                "provider": "fixture:jev-style",
                "subject": "agent:coder-1",
                "scope": "repo:alpha",
                "task": "fix login redirect",
                "action": "github.merge_pull_request",
                "intent": "merge unrelated changes",
                "beyond_scope_ppm": 940000,
                "destructive_ppm": 720000,
                "source_reliability_ppm": 850000,
                "independent_corroborators": 0
            })),
        )
        .await?;
    let observe_text = text_of(&observe);
    assert!(
        observe_text.contains("Suspended") || observe_text.contains("suspended"),
        "strong beyond-scope evidence should suspend, got: {observe_text}"
    );

    // Step 3: "echo" should now be DENIED by the gateway with evidence
    // — the gating path fires for a real non-r2r tool.
    let denied = client
        .call_tool(CallToolRequestParams::new("echo").with_arguments(object!({
            "message": "should be blocked"
        })))
        .await?;
    assert!(
        denied.is_error == Some(true),
        "echo after suspension should be denied with isError=true"
    );
    let deny_text = text_of(&denied);
    assert!(
        deny_text.contains("DENY") || deny_text.contains("blocked"),
        "deny evidence should explain the block, got: {deny_text}"
    );

    Ok(())
}
