use super::R2rMcpServer;
use crate::enforcement::{validate_execution, ExecutionBindingRequest};
use crate::store::json_file::JsonFileEventStore;
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerInfo, Tool, ToolAnnotations,
    },
    service::RequestContext,
    ErrorData, RoleServer, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

const VALIDATE_EXECUTION_TOOL: &str = "r2r_validate_execution";

#[derive(Debug, Deserialize, JsonSchema)]
struct ValidateExecutionParams {
    decision_id: String,
    action: String,
    expected_state_version: String,
}

/// Protocol-level metadata decorator for the R2R MCP server.
///
/// The underlying governance handlers remain unchanged; this wrapper ensures
/// `tools/list` and `get_tool` expose behavioral hints that match the actual
/// runtime side effects. When `R2R_STORE_PATH` is configured, it also exposes
/// the durable execution-binding gate as `r2r_validate_execution`.
#[derive(Clone)]
pub struct AnnotatedR2rMcpServer {
    inner: R2rMcpServer,
    enforcement_store_path: Option<PathBuf>,
}

impl AnnotatedR2rMcpServer {
    pub fn new(inner: R2rMcpServer) -> Self {
        let enforcement_store_path = std::env::var("R2R_STORE_PATH")
            .ok()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from);
        Self {
            inner,
            enforcement_store_path,
        }
    }

    fn validation_tool_enabled(&self) -> bool {
        self.enforcement_store_path.is_some()
    }

    fn call_validate_execution(&self, request: CallToolRequestParams) -> CallToolResponse {
        let Some(path) = &self.enforcement_store_path else {
            return tool_error(
                "r2r_validate_execution requires durable mode; configure R2R_STORE_PATH",
            );
        };

        let arguments = request.arguments.unwrap_or_default();
        let params: ValidateExecutionParams =
            match serde_json::from_value(Value::Object(arguments)) {
                Ok(params) => params,
                Err(error) => {
                    return tool_error(format!("invalid r2r_validate_execution arguments: {error}"));
                }
            };

        let store = match JsonFileEventStore::open(path) {
            Ok(store) => store,
            Err(error) => return tool_error(format!("failed to open durable event store: {error}")),
        };

        let result = match validate_execution(
            &store,
            &ExecutionBindingRequest {
                decision_id: params.decision_id,
                action: params.action,
                expected_state_version: params.expected_state_version,
            },
        ) {
            Ok(result) => result,
            Err(error) => return tool_error(format!("execution validation failed: {error}")),
        };

        match serde_json::to_string_pretty(&result) {
            Ok(json) => CallToolResponse::Complete(CallToolResult::success(vec![
                ContentBlock::text(json),
            ])),
            Err(error) => tool_error(format!("failed to serialize validation result: {error}")),
        }
    }
}

fn validation_tool() -> Tool {
    let input_schema = rmcp::handler::server::common::schema_for_input::<ValidateExecutionParams>()
        .expect("ValidateExecutionParams schema must be valid");
    Tool::new(
        VALIDATE_EXECUTION_TOOL,
        "Validate that an ALLOW decision is still executable by binding the attempted action to the exact durable R2R state_version from which the decision was derived. Stale, denied, unknown, or action-mismatched decisions fail closed.",
        input_schema,
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
}

fn tool_error(message: impl Into<String>) -> CallToolResponse {
    CallToolResponse::Complete(CallToolResult::error(vec![ContentBlock::text(message.into())]))
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
        "r2r_explain" | "r2r_replay" | VALIDATE_EXECUTION_TOOL => ToolAnnotations::new()
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
    fn get_info(&self) -> ServerInfo {
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
        if self.validation_tool_enabled() {
            result.tools.push(validation_tool());
        }
        Ok(result)
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        if name == VALIDATE_EXECUTION_TOOL && self.validation_tool_enabled() {
            return Some(validation_tool());
        }
        ServerHandler::get_tool(&self.inner, name).map(decorate_tool)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name.as_ref() == VALIDATE_EXECUTION_TOOL {
            return Ok(self.call_validate_execution(request));
        }
        ServerHandler::call_tool(&self.inner, request, context).await
    }
}
