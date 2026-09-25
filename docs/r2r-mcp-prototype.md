# r2r-mcp prototype

This executable is the first MCP-facing reference implementation of the R2R governance control plane.

It intentionally implements only the smallest real control loop:

```text
r2r_observe -> persistent R2R state -> r2r_decide -> r2r_explain
```

The server uses the official Rust MCP SDK (`rmcp`), stdio transport, an `EventStore` persistence boundary, and an in-memory store implementation.

## Run

```bash
cargo run --bin r2r-mcp
```

The process speaks MCP over stdin/stdout. It is intended to be launched by an MCP host rather than used as an interactive terminal program.

## Generic MCP host configuration

Build the binary first:

```bash
cargo build --bin r2r-mcp
```

Then point an MCP host at the executable. A generic configuration shape is:

```json
{
  "mcpServers": {
    "r2r": {
      "command": "/absolute/path/to/r2r-jev/target/debug/r2r-mcp"
    }
  }
}
```

Hosts that support a working-directory field may alternatively launch Cargo directly:

```json
{
  "mcpServers": {
    "r2r": {
      "command": "cargo",
      "args": ["run", "--quiet", "--bin", "r2r-mcp"],
      "cwd": "/absolute/path/to/r2r-jev"
    }
  }
}
```

The exact configuration file/location is host-specific.

## Tools

### `r2r_observe`

Submit an untrusted probabilistic observation. The MCP caller supplies the observation, but does **not** supply authoritative admission metadata.

Example arguments:

```json
{
  "provider": "fixture:jev-style",
  "subject": "agent:coder-1",
  "scope": "repo:alpha",
  "task": "fix login redirect",
  "action": "github.merge_pull_request",
  "intent": "merge unrelated changes",
  "beyond_scope_ppm": 940000,
  "destructive_ppm": 720000
}
```

The server binds the trusted Admission context internally, runs Evidence Admission, and lets only accepted evidence reach the R2R relation transitions.

A state version advances only when the relation graph for that `(subject, scope)` domain changes.

### `r2r_decide`

Evaluate a proposed action against the persistent relation state for the requested `(subject, scope)` domain.

Example arguments:

```json
{
  "subject": "agent:coder-1",
  "scope": "repo:alpha",
  "action": "github.merge_pull_request",
  "resource": "pr:42",
  "task": "fix login redirect"
}
```

The tool is read-only with respect to relation state. A suspended authorization returns a deterministic response shaped like:

```json
{
  "decision": "DENY",
  "reason_code": "AUTHORIZATION_SUSPENDED",
  "decision_id": "decision-000001",
  "state_version": "state-000001"
}
```

### `r2r_explain`

Explain a recorded decision:

```json
{
  "decision_id": "decision-000001"
}
```

The response includes the governing authorization relation, the domain-scoped state version used for the decision, and the recorded causal/provenance chain.

## End-to-end behavior

The reference test demonstrates this sequence over a real stdio MCP connection:

```text
MCP initialize
  -> tools/list
  -> r2r_observe(risky judgment)
  -> Evidence Admission accepts BeyondScope
  -> Authorization Active -> Suspended
  -> state-000001
  -> r2r_decide(merge_pull_request)
  -> DENY / AUTHORIZATION_SUSPENDED
  -> r2r_explain(decision-000001)
  -> provenance includes authorization-0001
```

Run it with:

```bash
cargo test --test mcp_stdio
```

The normal CI command also includes it:

```bash
cargo test --all-targets
```

## Trust boundary

The prototype deliberately does not let MCP JSON self-assert:

```text
source reliability
corroborator identity/count
policy version
virtual-time authority
operator identity
relation state
```

Those values belong to the trusted runtime boundary.

For v0.1 the server binds a deterministic demo Admission context internally. This is a reference mechanism, not a measured claim about Jev accuracy.

## Domain isolation

One `r2r-mcp` process can now host multiple isolated governance domains keyed by:

```text
(subject, scope)
```

Each domain owns an independent `Governance` state machine, virtual tick, projected decision, governing authorization relation, and state-version sequence. A suspension in `agent:coder-1 / repo:alpha` therefore does not change the decision or state version for `agent:coder-2 / repo:beta`.

Decision identifiers remain store-wide so `r2r_explain(decision_id)` can address a decision directly.

## EventStore boundary

The runtime depends on the `EventStore` trait rather than directly on `MemoryEventStore`.

The boundary currently covers:

```text
current_state_version(domain)
advance_state_version(domain)
record_event(event)
next_decision_id()
record_decision(decision)
decision(decision_id)
event(event_id)
```

`MemoryEventStore` is the reference implementation. A persistent implementation can replace it through `R2rMcpServer::with_store(...)` without changing MCP tool handlers.

## Current limitations

This prototype is intentionally local and minimal:

- the default store is in-memory; restart loses events and decisions;
- no `r2r_record_outcome` yet;
- no privileged `r2r_override` yet;
- no deterministic `r2r_replay` endpoint yet;
- no MCP resources yet;
- no remote Streamable HTTP/authentication mode yet;
- no execution adapter: R2R decides and explains, but does not execute downstream tools.

These boundaries preserve the product invariant:

```text
Judgment -> Evidence -> Admission -> R2R -> Enforcement
```

not:

```text
MCP caller -> set relation state directly
```
