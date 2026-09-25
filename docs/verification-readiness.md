# MCP Verification Readiness

This document makes repository-level verification signals explicit for scanners and directories that do not yet perform deep Rust MCP analysis.

## Executable MCP surface

The normative tool declarations are implemented with the Rust MCP SDK (`rmcp`) in `src/mcp/server.rs`. Input schemas live in `src/mcp/schema.rs`.

A machine-readable discovery catalog is also published at the repository root as `mcp-tools.json`. It is descriptive only; the Rust `#[tool]` declarations remain normative.

| Tool | State mutation | Purpose |
| --- | --- | --- |
| `r2r_observe` | Yes, but only through Admission + R2R rules | Ingest an untrusted probabilistic observation and admit eligible evidence into governance |
| `r2r_decide` | No | Evaluate a proposed action against persistent relation state |
| `r2r_explain` | No | Return durable causal/provenance information for a recorded decision |
| `r2r_record_outcome` | No direct relation mutation | Record an execution outcome as untrusted audit input |
| `r2r_replay` | No | Replay durable observation events and detect divergence |

The integration test `tests/mcp_stdio.rs` launches the actual `r2r-mcp` child process, calls MCP `tools/list`, asserts the five-tool surface, and executes the tool flow.

## Transport profile

Current reference transport:

```text
MCP host -> local child process -> stdio -> r2r-mcp
```

There is no public HTTP listener in this profile. Consequently, network authentication and network rate limiting are not implemented in the stdio reference server. Local process launch and access control are the responsibility of the MCP host and operating system.

This is a deliberate deployment boundary, not a claim that anonymous remote access is safe.

## Remote deployment gate

A future Streamable HTTP profile should not be considered production-ready until it provides at least:

- authenticated client identity;
- domain-scoped authorization;
- TLS;
- request/body limits;
- rate limiting;
- idempotency/replay protection for mutations;
- privileged-operation audit logging;
- secret-management integration.

## Privacy and security

Repository policies:

- `PRIVACY.md` — local MCP data handling, durable-store contents, opt-in live Jev behavior, deletion and telemetry;
- `SECURITY.md` — trust boundary, transport profiles, persistence integrity, fail-closed behavior and vulnerability reporting.

## Persistence and restart integrity

Durable mode is enabled with `R2R_STORE_PATH` and currently uses the local `JsonFileEventStore` reference backend.

On startup, the runtime does not blindly trust serialized projected state. It replays stored observation events using the trusted Admission snapshots captured at ingestion and checks:

1. kernel event identity;
2. relation transitions;
3. per-event state version;
4. final domain state version.

Startup fails if the available policy cannot reproduce the durable event log.

A real stdio integration test covers process restart: process 1 suspends authorization and exits; process 2 opens the same durable store and must still return `DENY` at the recovered state version.

## Third-party scanner limitations

Repository scanners may report `0 tools` when they do not understand Rust procedural macros or the `rmcp` registration pattern. For this project, the executable evidence is the actual MCP `tools/list` integration test rather than source-pattern inference alone.

Verification scores should always be tied to the exact repository commit/code hash that was scanned. Any repository change invalidates an earlier code hash until the verifier is run again.
