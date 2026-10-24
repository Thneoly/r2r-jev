use super::config::GatewayConfig;
use super::job;
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, ErrorCode, ListToolsResult,
        PaginatedRequestParams, Tool,
    },
    service::{RequestContext, RunningService},
    transport::TokioChildProcess,
    ErrorData, RoleClient, RoleServer, ServerHandler, ServiceExt,
};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::process::Command;

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

/// The P0 gateway: spawn upstreams, merge their tool catalogs, proxy calls.
///
/// Tool name collisions fail closed at startup — the gateway refuses to
/// serve a catalog where two upstreams expose the same tool name, because
/// silent shadowing would make enforcement ambiguous.
pub struct GatewayServer {
    upstreams: Vec<Upstream>,
    /// tool name -> upstream index; ownership is fixed at startup.
    owners: BTreeMap<String, usize>,
    /// Cached merged catalog (refreshed on restart, not per call).
    catalog: Vec<Tool>,
    call_timeout: Duration,
}

impl GatewayServer {
    /// Spawn every configured upstream, initialize the client session, and
    /// build the merged tool catalog. Any failure aborts the gateway: an
    /// upstream that cannot start must not be silently skipped, because the
    /// agent's expectation of the tool surface would go stale.
    pub async fn start(config: &GatewayConfig) -> Result<Self, String> {
        let mut upstreams = Vec::new();
        for up in &config.upstreams {
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
                        "tool name collision: {name:?} is exposed by more than one upstream \
                         ({} and {idx}); refusing to start — disambiguate upstream tool names",
                        upstreams[idx].name
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
        })
    }

    fn owner_of(&self, tool: &str) -> Option<usize> {
        self.owners.get(tool).copied()
    }
}

fn gateway_error(message: String) -> ErrorData {
    ErrorData::new(ErrorCode::INTERNAL_ERROR, message, None)
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
        let upstream = &self.upstreams[idx];
        let mut forwarded = CallToolRequestParams::new(name.clone());
        if let Some(arguments) = request.arguments {
            forwarded = forwarded.with_arguments(arguments);
        }
        let call = tokio::time::timeout(self.call_timeout, upstream.client.call_tool(forwarded));
        match call.await {
            Ok(Ok(result)) => Ok(CallToolResponse::Complete(result)),
            Ok(Err(error)) => Err(gateway_error(format!(
                "upstream {} call {name} failed: {error}",
                upstream.name
            ))),
            Err(_) => Err(gateway_error(format!(
                "upstream {} call {name} timed out after {}ms",
                upstream.name,
                self.call_timeout.as_millis()
            ))),
        }
    }
}
