use super::R2rMcpServer;
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, ListToolsResult, PaginatedRequestParams,
        ServerConfig, Tool, ToolAnnotations,
    },
    service::RequestContext,
    ErrorData, RoleServer, ServerHandler,
};

/// Protocol-level metadata decorator for the R2R MCP server.
///
/// The underlying governance handlers remain unchanged; this wrapper ensures
/// `tools/list` and `get_tool` expose behavioral hints that match the actual
/// runtime side effects.
#[derive(Clone)]
pub struct AnnotatedR2rMcpServer {
    inner: R2rMcpServer,
}

impl AnnotatedR2rMcpServer {
    pub fn new(inner: R2rMcpServer) -> Self {
        Self { inner }
    }
}

fn annotations_for(name: &str) -> Option<ToolAnnotations> {
    let annotations = match name {
        // May admit evidence and transition persistent governance relations.
        "r2r_observe" => ToolAnnotations::new()
            .read_only(false)
            .destructive(true)
            .idempotent(false)
            .open_world(false),
        // Relation-state read-only, but persists an audit decision and allocates
        // a new decision id, so it is not environment-read-only or idempotent.
        "r2r_decide" => ToolAnnotations::new()
            .read_only(false)
            .destructive(false)
            .idempotent(false)
            .open_world(false),
        "r2r_explain" | "r2r_replay" => ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
        // Additive audit write only; does not directly transition relations.
        "r2r_record_outcome" => ToolAnnotations::new()
            .read_only(false)
            .destructive(false)
            .idempotent(false)
            .open_world(false),
        _ => return None,
    };
    Some(annotations)
}

fn decorate_tool(mut tool: Tool) -> Tool {
    tool.annotations = annotations_for(tool.name.as_ref());
    tool
}

impl ServerHandler for AnnotatedR2rMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerHandler::get_info(&self.inner)
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut result = ServerHandler::list_tools(&self.inner, request, context).await?;
        for tool in &mut result.tools {
            tool.annotations = annotations_for(tool.name.as_ref());
        }
        Ok(result)
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        ServerHandler::get_tool(&self.inner, name).map(decorate_tool)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        ServerHandler::call_tool(&self.inner, request, context).await
    }
}
