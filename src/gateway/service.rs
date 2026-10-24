use super::config::GatewayConfig;
use super::job;
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorCode,
        ListToolsResult, PaginatedRequestParams, Tool,
    },
    object,
    service::{RequestContext, RunningService},
    transport::TokioChildProcess,
    ErrorData, RoleClient, RoleServer, ServerHandler, ServiceExt,
};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::process::Command;

/// Tools served by the governance control plane. These are never gated:
/// they are the governance surface itself.
const CONTROL_PLANE_PREFIX: &str = "r2r_";

fn merged_tools(catalog: &[Tool]) -> ListToolsResult {
    let mut result = ListToolsResult::default();
    result.tools = catalog.to_vec();
    result
}

/// One spawned upstream MCP server and our client connection to it.
struct Upstream {
    name: String,
    client: RunningService<RoleClient, ()>,
}

/// The P0/P1 gateway: spawn upstreams, merge tool catalogs, proxy calls,
/// and — when a control-plane upstream is configured — authorize every
/// non-governance tool call against it before forwarding.
///
/// Tool name collisions fail closed at startup.
pub struct GatewayServer {
    upstreams: Vec<Upstream>,
    /// tool name -> upstream index; ownership is fixed at startup.
    owners: BTreeMap<String, usize>,
    /// Cached merged catalog.
    catalog: Vec<Tool>,
    call_timeout: Duration,
    /// Index of the control-plane upstream, if configured.
    control_plane: Option<usize>,
    /// Governance subject/scope applied to every gated call.
    subject: String,
    scope: String,
}

/// Result of authorizing a tool call against the control plane.
struct Authorization {
    allowed: bool,
    reason_code: String,
    decision_id: String,
    governing_relations: Vec<(String, String, String)>, // (id, type, state)
    state_version: String,
}

impl Authorization {
    /// Evidence text carried in the isError content when a call is denied.
    /// Follows the E6 pattern: phase, governing relations, reason, state version.
    fn evidence_text(&self, tool: &str) -> String {
        let mut lines = vec![format!(
            "DENY: {} for {}/{}",
            self.reason_code, self.state_version, tool
        )];
        for (id, rel_type, state) in &self.governing_relations {
            lines.push(format!("  governing: {} {} = {}", rel_type, id, state));
        }
        lines.push(format!("  decision: {}", self.decision_id));
        lines.push(format!(
            "  repair: call r2r_observe with a human_override event, or use the control plane to repair the relation"
        ));
        lines.join("\n")
    }
}

impl GatewayServer {
    /// Spawn every configured upstream, initialize the client session, and
    /// build the merged tool catalog. Any failure aborts the gateway.
    pub async fn start(config: &GatewayConfig) -> Result<Self, String> {
        let mut upstreams = Vec::new();
        let mut control_plane = None;

        for (idx, up) in config.upstreams.iter().enumerate() {
            let mut command = Command::new(&up.command);
            command.args(&up.args);
            command.envs(&up.env);
            let transport = TokioChildProcess::new(command)
                .map_err(|e| format!("spawn upstream {}: {e}", up.name))?;
            if let Some(pid) = transport.id() {
                job::assign_child(pid);
            }
            let init = Duration::from_millis(config.init_timeout_ms);
            let client = tokio::time::timeout(init, ().serve(transport))
                .await
                .map_err(|_| {
                    format!(
                        "upstream {} initialize timed out after {}ms",
                        up.name, config.init_timeout_ms
                    )
                })?
                .map_err(|e| format!("upstream {} failed to initialize: {e}", up.name))?;
            if up.control_plane {
                control_plane = Some(idx);
            }
            upstreams.push(Upstream {
                name: up.name.clone(),
                client,
            });
        }

        let mut owners = BTreeMap::new();
        let mut catalog = Vec::new();
        for (idx, up) in upstreams.iter().enumerate() {
            let tools = up
                .client
                .list_all_tools()
                .await
                .map_err(|e| format!("upstream {} tools/list failed: {e}", up.name))?;
            for tool in tools {
                let name = tool.name.to_string();
                if owners.insert(name.clone(), idx).is_some() {
                    return Err(format!(
                        "tool name collision: {name:?} is exposed by more than one upstream; refusing to start"
                    ));
                }
                catalog.push(tool);
            }
        }

        Ok(Self {
            upstreams,
            owners,
            catalog,
            call_timeout: Duration::from_millis(config.call_timeout_ms),
            control_plane,
            subject: config.governance.subject.clone(),
            scope: config.governance.scope.clone(),
        })
    }

    fn owner_of(&self, tool: &str) -> Option<usize> {
        self.owners.get(tool).copied()
    }

    fn is_control_plane_tool(&self, tool: &str) -> bool {
        tool.starts_with(CONTROL_PLANE_PREFIX)
            && self
                .control_plane
                .is_some_and(|cp| self.owners.get(tool) == Some(&cp))
    }

    /// Ask the control plane whether the given action is currently allowed.
    /// Side-effect free on the governance state (r2r_decide semantics).
    async fn authorize(&self, tool: &str) -> Result<Authorization, String> {
        let cp_idx = self
            .control_plane
            .ok_or("governance enforcement requires a control-plane upstream")?;
        let cp = &self.upstreams[cp_idx];
        let request = CallToolRequestParams::new("r2r_decide").with_arguments(object!({
            "subject": self.subject,
            "scope": self.scope,
            "action": tool,
        }));
        let result = tokio::time::timeout(self.call_timeout, cp.client.call_tool(request))
            .await
            .map_err(|_| {
                format!(
                    "control-plane r2r_decide timed out after {}ms",
                    self.call_timeout.as_millis()
                )
            })?
            .map_err(|e| format!("control-plane r2r_decide failed: {e}"))?;

        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .ok_or("control-plane r2r_decide returned no text content")?
            .text
            .clone();
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("r2r_decide response not valid JSON: {e}"))?;

        let decision = json
            .pointer("/decision")
            .and_then(|v| v.as_str())
            .unwrap_or("DENY")
            .to_string();
        let reason_code = json
            .pointer("/reason_code")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();
        let decision_id = json
            .pointer("/decision_id")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let state_version = json
            .pointer("/state_version")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let governing_relations = json
            .pointer("/governing_relations")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|rel| {
                        Some((
                            rel.pointer("/relation_id")?.as_str()?.to_string(),
                            rel.pointer("/relation_type")?.as_str()?.to_string(),
                            rel.pointer("/state")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Authorization {
            allowed: decision == "ALLOW",
            reason_code,
            decision_id,
            governing_relations,
            state_version,
        })
    }

    /// Record the execution outcome of a gated call on the control plane.
    /// Failures are logged to stderr but never block the tool response.
    async fn record_outcome(&self, decision_id: &str, outcome: &str, detail: Option<String>) {
        let Some(cp_idx) = self.control_plane else {
            return;
        };
        let cp = &self.upstreams[cp_idx];
        let mut args = object!({
            "decision_id": decision_id,
            "outcome": outcome,
        });
        if let Some(d) = detail {
            let _ = args.insert("detail".into(), serde_json::Value::String(d));
        }
        let request = CallToolRequestParams::new("r2r_record_outcome").with_arguments(args);
        let result = tokio::time::timeout(self.call_timeout, cp.client.call_tool(request)).await;
        match result {
            Ok(Ok(response)) if response.is_error != Some(true) => {}
            Ok(Ok(response)) => {
                let text = response
                    .content
                    .first()
                    .and_then(|c| c.as_text())
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                eprintln!(
                    "[r2r-gateway:audit] record_outcome({decision_id}, {outcome}) error: {text}"
                );
            }
            Ok(Err(error)) => {
                eprintln!(
                    "[r2r-gateway:audit] record_outcome({decision_id}, {outcome}) transport error: {error}"
                );
            }
            Err(_) => {
                eprintln!("[r2r-gateway:audit] record_outcome({decision_id}, {outcome}) timed out");
            }
        }
    }
}

fn gateway_error(message: String) -> ErrorData {
    ErrorData::new(ErrorCode::INTERNAL_ERROR, message, None)
}

fn deny_result(evidence: String) -> CallToolResponse {
    let mut result = CallToolResult::default();
    result.is_error = Some(true);
    result.content = vec![ContentBlock::text(evidence)];
    CallToolResponse::Complete(result)
}

impl ServerHandler for GatewayServer {
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(merged_tools(&self.catalog))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.to_string();
        let Some(idx) = self.owner_of(&name) else {
            return Err(ErrorData::invalid_params(
                format!("unknown tool {name:?}: not exposed by any configured upstream"),
                None,
            ));
        };

        // Control-plane tools (r2r_observe, r2r_decide, ...) pass through
        // ungated: they ARE the governance surface.
        if self.is_control_plane_tool(&name) {
            let upstream = &self.upstreams[idx];
            let mut forwarded = CallToolRequestParams::new(name.clone());
            if let Some(arguments) = request.arguments {
                forwarded = forwarded.with_arguments(arguments);
            }
            let call =
                tokio::time::timeout(self.call_timeout, upstream.client.call_tool(forwarded));
            return match call.await {
                Ok(Ok(result)) => Ok(CallToolResponse::Complete(result)),
                Ok(Err(error)) => Err(gateway_error(format!(
                    "upstream {} call {name} failed: {error}",
                    upstream.name
                ))),
                Err(_) => Err(gateway_error(format!(
                    "upstream {} call {name} timed out",
                    upstream.name
                ))),
            };
        }

        // Gated tool: authorize against the control plane first.
        let auth = match self.authorize(&name).await {
            Ok(auth) => auth,
            Err(error) => {
                // Authorization infrastructure failure: fail closed.
                // No decision_id available — record with a placeholder.
                self.record_outcome("?", "blocked", Some(error.clone()))
                    .await;
                return Ok(deny_result(format!(
                    "DENY (fail-closed): authorization check failed: {error}"
                )));
            }
        };
        if !auth.allowed {
            self.record_outcome(&auth.decision_id, "blocked", None)
                .await;
            return Ok(deny_result(auth.evidence_text(&name)));
        }

        // Authorized: forward to the owning upstream.
        let upstream = &self.upstreams[idx];
        let mut forwarded = CallToolRequestParams::new(name.clone());
        if let Some(arguments) = request.arguments {
            forwarded = forwarded.with_arguments(arguments);
        }
        let call = tokio::time::timeout(self.call_timeout, upstream.client.call_tool(forwarded));
        match call.await {
            Ok(Ok(result)) => {
                let upstream_error = result.is_error == Some(true);
                let outcome = if upstream_error { "failed" } else { "executed" };
                self.record_outcome(&auth.decision_id, outcome, None).await;
                Ok(CallToolResponse::Complete(result))
            }
            Ok(Err(error)) => {
                self.record_outcome(
                    &auth.decision_id,
                    "failed",
                    Some(format!("upstream {name} error: {error}")),
                )
                .await;
                Err(gateway_error(format!(
                    "upstream {} call {name} failed: {error}",
                    upstream.name
                )))
            }
            Err(_) => {
                self.record_outcome(
                    &auth.decision_id,
                    "failed",
                    Some(format!("upstream {name} timed out")),
                )
                .await;
                Err(gateway_error(format!(
                    "upstream {} call {name} timed out after {}ms",
                    upstream.name,
                    self.call_timeout.as_millis()
                )))
            }
        }
    }
}
