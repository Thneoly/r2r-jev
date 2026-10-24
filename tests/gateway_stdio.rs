use rmcp::{model::CallToolRequestParams, object, transport::TokioChildProcess, ServiceExt};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

fn temp_config(label: &str, upstream_command: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "r2r-jev-gateway-{label}-{}-{nanos}.json",
        std::process::id()
    ));
    let upstream_command = upstream_command.replace('\\', "/");
    let config = format!(
        r#"{{"upstreams":[{{"name":"control-plane","command":"{upstream_command}"}}],"init_timeout_ms":120000}}"#
    );
    std::fs::write(&path, config).expect("write gateway config");
    path
}

async fn connect(
    label: &str,
) -> anyhow::Result<rmcp::service::RunningService<rmcp::RoleClient, ()>> {
    let config = temp_config(label, env!("CARGO_BIN_EXE_r2r-mcp"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_r2r-gateway"));
    command.arg(&config);
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    Ok(client)
}

#[tokio::test]
async fn gateway_merges_upstream_catalog_and_proxies_calls() -> anyhow::Result<()> {
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
    let text = decision
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("decision should return text content");
    assert!(
        text.text.contains("ALLOW"),
        "proxied decide should ALLOW on a fresh domain, got: {}",
        text.text
    );

    Ok(())
}

#[tokio::test]
async fn gateway_fails_closed_on_unknown_tool() -> anyhow::Result<()> {
    let client = connect("unknown").await?;

    let result = client
        .call_tool(CallToolRequestParams::new("definitely_not_a_tool"))
        .await;
    // rmcp surfaces server errors as Err(ServiceError); either way the call
    // must NOT succeed — we accept Err or an error result, never a success.
    match result {
        Err(_) => { /* transport-level error: acceptable fail-closed */ }
        Ok(response) => {
            assert!(
                response.is_error == Some(true),
                "unknown tool should fail closed with isError=true"
            );
        }
    }

    Ok(())
}
