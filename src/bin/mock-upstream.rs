use rmcp::{
    handler::server::common::schema_for_input,
    model::{CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Tool},
    schemars::JsonSchema,
    serde::Deserialize,
    service::RequestContext,
    transport::stdio,
    ErrorData, RoleServer, ServerHandler, ServiceExt,
};

/// Minimal MCP server exposing one non-governance tool ("echo") for
/// testing the gateway's gating path with a real upstream that has
/// tools outside the `r2r_` namespace.
#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct EchoParams {
    message: String,
}

struct MockUpstream;

impl ServerHandler for MockUpstream {
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        let schema = schema_for_input::<EchoParams>().expect("EchoParams schema");
        let tool = Tool::new(
            "echo",
            "Echo the message back. Used for testing the gateway gating path.",
            schema,
        );
        let mut result = rmcp::model::ListToolsResult::default();
        result.tools = vec![tool];
        Ok(result)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name.as_ref() != "echo" {
            return Err(ErrorData::invalid_params(
                format!("unknown tool: {}", request.name),
                None,
            ));
        }
        let args = request.arguments.unwrap_or_default();
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("(no message)");
        let mut result = CallToolResult::default();
        result.content = vec![ContentBlock::text(format!("echo: {message}"))];
        Ok(CallToolResponse::Complete(result))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = MockUpstream.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
