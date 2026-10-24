use serde::Deserialize;
use std::collections::BTreeMap;

/// One upstream MCP server the gateway spawns and proxies to.
#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamConfig {
    /// Short name used in logs and tool-ownership errors.
    pub name: String,
    /// Executable to run (e.g. `npx`, `node`, or a path to a binary).
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables for this upstream only (e.g. API tokens).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Mark this upstream as the governance control plane. Its tools
    /// (`r2r_*`) are served ungated; every other tool call is authorized
    /// against it before forwarding. At most one upstream may set this.
    #[serde(default)]
    pub control_plane: bool,
}

/// Governance defaults applied to every gated tool call.
#[derive(Debug, Clone, Deserialize)]
pub struct GovernanceConfig {
    /// The governed agent identity (e.g. `agent:coder-1`).
    #[serde(default = "default_subject")]
    pub subject: String,
    /// The governance scope (e.g. `repo:alpha`).
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_subject() -> String {
    "agent:default".to_string()
}

fn default_scope() -> String {
    "default".to_string()
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            subject: default_subject(),
            scope: default_scope(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub upstreams: Vec<UpstreamConfig>,
    /// Initialize timeout per upstream in ms. Generous by default: npx-backed
    /// servers download packages on first start.
    #[serde(default = "default_init_timeout_ms")]
    pub init_timeout_ms: u64,
    /// Per-call forwarding timeout in ms.
    #[serde(default = "default_call_timeout_ms")]
    pub call_timeout_ms: u64,
    /// Governance enforcement configuration. When a control-plane upstream
    /// is configured, every non-control-plane tool call is authorized
    /// against it before forwarding.
    #[serde(default)]
    pub governance: GovernanceConfig,
}

fn default_init_timeout_ms() -> u64 {
    120_000
}

fn default_call_timeout_ms() -> u64 {
    120_000
}

impl GatewayConfig {
    pub fn load(path: &str) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read gateway config {path}: {e}"))?;
        let config: GatewayConfig = serde_json::from_str(&raw)
            .map_err(|e| format!("invalid gateway config {path}: {e}"))?;
        let control_planes = config
            .upstreams
            .iter()
            .filter(|up| up.control_plane)
            .count();
        if control_planes > 1 {
            return Err(format!(
                "invalid gateway config {path}: at most one upstream may set control_plane=true, found {control_planes}"
            ));
        }
        Ok(config)
    }
}
