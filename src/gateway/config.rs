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
        serde_json::from_str(&raw).map_err(|e| format!("invalid gateway config {path}: {e}"))
    }
}
