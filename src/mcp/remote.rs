use super::AnnotatedR2rMcpServer;
use crate::store::json_file::JsonFileEventStore;
use crate::store::EventStore;
use rmcp::{
    model::{CallToolRequestParams, CallToolResponse, ListToolsResult, PaginatedRequestParams, ServerInfo, Tool},
    service::RequestContext,
    ErrorData, RoleServer, ServerHandler,
};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct RemotePrincipalServer {
    inner: AnnotatedR2rMcpServer,
    principal: Arc<str>,
    allowed_scopes: Arc<HashSet<String>>,
    store_path: Arc<PathBuf>,
}

impl RemotePrincipalServer {
    pub fn new(
        inner: AnnotatedR2rMcpServer,
        principal: impl Into<String>,
        allowed_scopes: HashSet<String>,
        store_path: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let principal = principal.into();
        if principal.trim().is_empty() {
            return Err("remote principal must not be empty".to_string());
        }
        if allowed_scopes.is_empty() {
            return Err("remote scope allowlist must not be empty".to_string());
        }
        Ok(Self {
            inner,
            principal: Arc::from(principal),
            allowed_scopes: Arc::new(allowed_scopes),
            store_path: Arc::new(store_path.into()),
        })
    }

    fn authorize_request(&self, request: &mut CallToolRequestParams) -> Result<(), String> {
        match request.name.as_ref() {
            "r2r_observe" | "r2r_decide" | "r2r_replay" => {
                let arguments = request.arguments.get_or_insert_default();
                let scope = arguments
                    .get("scope")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "remote governance request requires scope".to_string())?;
                self.ensure_scope(scope)?;
                arguments.insert(
                    "subject".to_string(),
                    Value::String(self.principal.to_string()),
                );
                Ok(())
            }
            "r2r_explain" | "r2r_record_outcome" | "r2r_validate_execution" => {
                let decision_id = request
                    .arguments
                    .as_ref()
                    .and_then(|args| args.get("decision_id"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "remote request requires decision_id".to_string())?;
                self.ensure_decision_access(decision_id)
            }
            _ => Ok(()),
        }
    }

    fn ensure_scope(&self, scope: &str) -> Result<(), String> {
        if self.allowed_scopes.contains(scope) {
            Ok(())
        } else {
            Err(format!("scope not authorized for remote principal: {scope}"))
        }
    }

    fn ensure_decision_access(&self, decision_id: &str) -> Result<(), String> {
        let store = JsonFileEventStore::open(self.store_path.as_ref().clone())?;
        store.health()?;
        let decision = store
            .decision(decision_id)
            .ok_or_else(|| format!("unknown decision_id: {decision_id}"))?;
        if decision.domain.subject != self.principal.as_ref() {
            return Err("decision belongs to a different authenticated principal".to_string());
        }
        self.ensure_scope(&decision.domain.scope)
    }
}

fn denied_tool_response(message: String) -> CallToolResponse {
    CallToolResponse::Complete(rmcp::model::CallToolResult::error(vec![
        rmcp::model::ContentBlock::text(message),
    ]))
}

impl ServerHandler for RemotePrincipalServer {
    fn get_info(&self) -> ServerInfo {
        ServerHandler::get_info(&self.inner)
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        ServerHandler::list_tools(&self.inner, request, context).await
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        ServerHandler::get_tool(&self.inner, name)
    }

    async fn call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if let Err(error) = self.authorize_request(&mut request) {
            return Ok(denied_tool_response(error));
        }
        ServerHandler::call_tool(&self.inner, request, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::R2rMcpServer;
    use crate::store::{DomainKey, StoredDecision};
    use rmcp::model::CallToolRequestParams;
    use serde_json::{Map, Value};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_store_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("r2r-remote-auth-{}-{nanos}.json", std::process::id()))
    }

    fn server(path: &PathBuf) -> RemotePrincipalServer {
        std::env::set_var("R2R_STORE_PATH", path);
        let base = R2rMcpServer::new();
        let annotated = AnnotatedR2rMcpServer::new(base);
        RemotePrincipalServer::new(
            annotated,
            "agent:authenticated",
            HashSet::from(["repo:alpha".to_string()]),
            path,
        )
        .expect("remote server")
    }

    #[test]
    fn subject_is_server_resolved_and_scope_is_enforced() {
        let path = temp_store_path();
        let server = server(&path);
        let mut arguments = Map::new();
        arguments.insert("subject".to_string(), Value::String("agent:spoofed".to_string()));
        arguments.insert("scope".to_string(), Value::String("repo:alpha".to_string()));
        let mut request = CallToolRequestParams::new("r2r_decide").with_arguments(arguments);
        server.authorize_request(&mut request).expect("authorized");
        assert_eq!(
            request.arguments.as_ref().and_then(|a| a.get("subject")),
            Some(&Value::String("agent:authenticated".to_string()))
        );

        let mut denied_args = Map::new();
        denied_args.insert("scope".to_string(), Value::String("repo:beta".to_string()));
        let mut denied = CallToolRequestParams::new("r2r_replay").with_arguments(denied_args);
        assert!(server.authorize_request(&mut denied).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn decision_lookup_enforces_principal_and_scope() {
        let path = temp_store_path();
        let mut store = JsonFileEventStore::open(&path).expect("store");
        store.record_decision(StoredDecision {
            decision_id: "decision-000001".to_string(),
            domain: DomainKey::new("agent:other", "repo:alpha"),
            action: "github.merge_pull_request".to_string(),
            verdict: "ALLOW".to_string(),
            reason_code: "AUTHORIZATION_ACTIVE".to_string(),
            governing_relation_id: None,
            state_version: "state-000000".to_string(),
            provenance: vec![],
        });
        assert!(store.health().is_ok());

        let server = server(&path);
        let mut args = Map::new();
        args.insert(
            "decision_id".to_string(),
            Value::String("decision-000001".to_string()),
        );
        let mut request = CallToolRequestParams::new("r2r_explain").with_arguments(args);
        assert!(server.authorize_request(&mut request).is_err());
        let _ = fs::remove_file(path);
    }
}
