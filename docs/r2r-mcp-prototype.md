# r2r-mcp prototype

This executable is the first MCP-facing reference implementation of the R2R governance control plane.

The currently executable control loop is:

```text
r2r_observe
  -> persistent R2R state
  -> r2r_decide
  -> r2r_explain
  -> r2r_record_outcome
  -> r2r_replay
```

The server uses the official Rust MCP SDK (`rmcp`), stdio transport, an `EventStore` persistence boundary, and both in-memory and durable local-store profiles.

## Run

Default in-memory mode:

```bash
cargo run --bin r2r-mcp
```

Durable local mode:

```bash
R2R_STORE_PATH=/absolute/path/r2r-store.json cargo run --bin r2r-mcp
```

The process speaks MCP over stdin/stdout and is intended to be launched by an MCP host.

## Generic MCP host configuration

```bash
cargo build --bin r2r-mcp
```

Then point the host at the executable:

```json
{
  "mcpServers": {
    "r2r": {
      "command": "/absolute/path/to/r2r-jev/target/debug/r2r-mcp",
      "env": {
        "R2R_STORE_PATH": "/absolute/path/to/r2r-store.json"
      }
    }
  }
}
```

Omit `R2R_STORE_PATH` to use the in-memory reference store.

## Tools

### `r2r_observe`

Submit an untrusted probabilistic observation. The caller does not supply authoritative admission metadata.

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

The trusted runtime binds source reliability, corroboration, virtual time and expiry, runs Evidence Admission, then applies only accepted evidence to R2R.

The public event id is store-wide, for example `event-000001`. The R2R kernel keeps its deterministic domain-local id such as `ev-0001`; both are retained in provenance.

A state version advances only when the relation graph for that `(subject, scope)` domain changes.

### `r2r_decide`

Evaluate a proposed action against persistent relation state without mutating it.

```json
{
  "subject": "agent:coder-1",
  "scope": "repo:alpha",
  "action": "github.merge_pull_request",
  "resource": "pr:42",
  "task": "fix login redirect"
}
```

A suspended authorization returns a response shaped like:

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

Decision provenance is assembled from durable stored events, including the public event id, domain-local kernel event id, admitted evidence, relation transitions, and decision line.

### `r2r_record_outcome`

Record what happened after a decision:

```json
{
  "decision_id": "decision-000001",
  "outcome": "blocked",
  "detail": "enforcement adapter blocked execution"
}
```

Supported outcome values are finite and typed:

```text
executed
blocked
failed
rolled_back
user_corrected
policy_breach_confirmed
```

Outcome reports are caller supplied and therefore remain **untrusted input** in v0.1. Recording an outcome does not directly mutate relation state, even for `policy_breach_confirmed`. A later outcome-to-evidence Admission rule may turn selected outcomes into governance-active evidence without allowing MCP JSON to bypass Admission.

### `r2r_replay`

Replay all stored observation events for one governance domain:

```json
{
  "subject": "agent:coder-1",
  "scope": "repo:alpha"
}
```

For each observation event the store retains the normalized judgment plus the trusted Admission snapshot used at ingestion:

```text
source_reliability_ppm
independent_corroborators
now_vtick
expires_vtick
admission_policy_version
```

Replay rebuilds a fresh `Governance` kernel and checks, event by event:

```text
kernel event id
relation transitions
resulting domain state version
```

The response includes:

```text
replay_match
recorded_state_version
replayed_state_version
replayed_events
first_divergent_event
policy_versions
```

The first divergence is reported using the store-wide public event id.

## End-to-end behavior

The real stdio integration tests execute both the normal five-tool flow and a process restart flow:

```text
process 1
  -> r2r_observe(risky judgment)
  -> Authorization Active -> Suspended
  -> durable state-000001
  -> exit

process 2
  -> open same R2R_STORE_PATH
  -> startup recovery/replay
  -> r2r_decide
  -> DENY at state-000001
  -> r2r_replay
  -> replay_match=true
```

Run them with:

```bash
cargo test --test mcp_stdio
```

The normal CI command includes them:

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

One `r2r-mcp` process can host multiple isolated governance domains keyed by:

```text
(subject, scope)
```

Each domain owns an independent `Governance` state machine, virtual tick, projected decision, governing authorization relation, and state-version sequence.

Store-wide public ids prevent collisions between domain-local kernel ids. For example, two domains may both produce `ev-0001`, while the EventStore exposes them as `event-000001` and `event-000002`.

## EventStore boundary

The runtime depends on the `EventStore` trait rather than directly on a storage implementation.

The boundary covers store health, domain state versions, durable events, decisions, and outcomes:

```text
health()
current_state_version(domain)
advance_state_version(domain)

next_event_id()
record_event(event)
event(event_id)
events_for_domain(domain)
all_events()

next_decision_id()
record_decision(decision)
decision(decision_id)

next_outcome_id()
record_outcome(outcome)
outcome(outcome_id)
```

### `MemoryEventStore`

The default reference implementation. It is deterministic but volatile.

### `JsonFileEventStore`

A local durable profile intended to prove persistence/recovery semantics before introducing a database backend.

It serializes the complete store snapshot and replaces the target through a temporary file plus rename. A failed durable write marks the store unhealthy. MCP calls check store health before and after execution; after a persistence failure the runtime fails closed instead of silently continuing on volatile state.

## Startup recovery

Opening a durable store does not simply trust the serialized projected state.

Before the MCP server starts accepting calls it:

1. reads all stored observation events;
2. rebuilds each `(subject, scope)` `Governance` kernel from scratch;
3. uses the trusted Admission snapshot captured when each event was ingested;
4. verifies the stored kernel event id;
5. verifies every relation transition;
6. verifies each resulting domain `state_version`;
7. verifies the final stored domain state version.

If the required Admission policy version is unavailable or any event diverges, startup fails rather than serving decisions from unverifiable state.

This is the same core property used by `r2r_replay`, now applied to runtime recovery.

## Current limitations

This prototype is still intentionally local and minimal:

- durable mode is a single local JSON snapshot, not a transactional database and not safe for multiple concurrent writer processes;
- outcome events are auditable but do not yet enter an outcome-specific Evidence Admission rule;
- replay supports the current observation event type and current policy implementation, not historical loading of arbitrary old rule-pack binaries;
- no privileged `r2r_override` MCP tool yet;
- no MCP resources yet;
- no remote Streamable HTTP/authentication mode yet;
- no execution adapter: R2R decides and records outcomes, but does not execute downstream tools;
- decision/state-version binding is exposed but not yet enforced atomically at execution time.

The architectural invariant remains:

```text
Judgment -> Evidence -> Admission -> R2R -> Enforcement
```

not:

```text
MCP caller -> set relation state directly
```
