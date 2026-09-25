# R2R MCP Remote Profile v0.1

## Purpose

`r2r-mcp-remote` is the first networked deployment profile for the R2R governance control plane.

The remote profile keeps the same governance model as the stdio server but moves identity authority out of caller-supplied MCP arguments and into server configuration.

```text
MCP client
   |
   | HTTPS at production edge
   v
Bearer authentication
   |
   v
server-resolved principal
   |
   v
exact scope authorization
   |
   v
R2R MCP tools
   |
   v
durable EventStore
   |
   v
Decision-State Binding / enforcement
```

## Security properties

### Server-resolved identity

A remote client cannot choose its R2R `subject`.

For `r2r_observe`, `r2r_decide`, and `r2r_replay`, the wrapper overwrites any client-supplied `subject` with `R2R_REMOTE_PRINCIPAL` before the request reaches the governance runtime.

This changes the trust model from:

```text
client says: subject = agent:coder-1
```

to:

```text
authenticated deployment identity
        -> server resolves subject
        -> R2R domain
```

### Exact scope allowlist

`R2R_REMOTE_SCOPES` is a comma-separated set of exact scope identifiers. v0.1 deliberately does not implement wildcard expansion.

Requests for a scope outside the allowlist fail closed before reaching R2R state mutation or decision logic.

For tools that reference an existing `decision_id`, the server reads the durable decision record and verifies both the stored principal and stored scope before delegating the operation.

### Durable-only remote governance

`R2R_STORE_PATH` is required.

The remote profile does not run against `MemoryEventStore`. Network-accessible authorization and execution validation must survive process restart and remain replayable.

### Bearer authentication

Every HTTP request to `/mcp` must include:

```text
Authorization: Bearer <token>
```

`R2R_REMOTE_BEARER_TOKEN` must contain at least 32 bytes. The reference server compares equal-length token bytes without early exit.

The token is configuration, not governance evidence. It is never inserted into relation state or provenance.

### Rate limiting

The reference profile applies a fixed-window request limit before MCP dispatch.

`R2R_REMOTE_RATE_LIMIT_PER_MINUTE` defaults to `60`. A rejected request returns HTTP `429` with `Retry-After: 60`.

The v0.1 limiter is process-local and deployment-wide. A production multi-replica deployment needs a per-principal distributed limiter or equivalent gateway control.

### Transport security

The binary defaults to:

```text
127.0.0.1:8000
```

It refuses to bind plaintext HTTP to a non-loopback address unless the operator explicitly sets:

```text
R2R_REMOTE_ALLOW_INSECURE_HTTP=1
```

That escape hatch is for controlled development only.

Production deployment should terminate TLS at a reverse proxy, ingress, or service mesh on the same trusted host/network boundary and proxy to the loopback listener. Bearer credentials must not traverse an untrusted plaintext network.

## Configuration

Required:

```bash
export R2R_STORE_PATH=/var/lib/r2r/events.json
export R2R_REMOTE_PRINCIPAL=agent:coder-1
export R2R_REMOTE_SCOPES=repo:alpha,repo:beta
export R2R_REMOTE_BEARER_TOKEN='replace-with-at-least-32-bytes-of-random-secret'
```

Optional:

```bash
export R2R_REMOTE_BIND=127.0.0.1:8000
export R2R_REMOTE_RATE_LIMIT_PER_MINUTE=60
```

Start:

```bash
cargo run --bin r2r-mcp-remote
```

Endpoint:

```text
http://127.0.0.1:8000/mcp
```

The production-facing URL should be HTTPS through the TLS terminator.

## Authorization behavior by tool

| Tool | Remote identity/scope behavior |
| --- | --- |
| `r2r_observe` | server overwrites `subject`; requested `scope` must be allowed |
| `r2r_decide` | server overwrites `subject`; requested `scope` must be allowed |
| `r2r_replay` | server overwrites `subject`; requested `scope` must be allowed |
| `r2r_explain` | stored decision must belong to principal and allowed scope |
| `r2r_record_outcome` | stored decision must belong to principal and allowed scope |
| `r2r_validate_execution` | stored decision must belong to principal and allowed scope; normal Decision-State Binding checks still apply |

## Failure boundary

Remote authentication and domain authorization are outer control-plane checks. They do not replace R2R relation governance.

```text
AuthN
  -> deployment principal
AuthZ
  -> allowed R2R domain
R2R
  -> relationship-based governance decision
Decision-State Binding
  -> execution-time freshness check
```

A request must pass every layer.

## Tests

The repository exercises:

- missing Bearer credentials -> HTTP 401;
- incorrect Bearer credentials -> HTTP 401;
- authenticated MCP `initialize` over Streamable HTTP -> success;
- caller-supplied `subject` -> replaced by server principal;
- disallowed scope -> rejected;
- decision owned by another principal -> rejected;
- rate limit exhaustion -> rejected;
- existing durable restart/replay and stale-decision enforcement tests.

## v0.1 limitations

This is intentionally a narrow reference remote profile, not a complete multi-tenant IAM system.

Current limitations include:

- one authenticated principal per deployment;
- one static Bearer secret per deployment;
- process-local rate limiting;
- JSON file EventStore remains a single-writer reference backend;
- TLS is expected at a trusted reverse proxy/ingress rather than implemented in-process;
- no token rotation registry, OIDC/OAuth identity mapping, tenant registry, or distributed session/rate state yet.

The next remote iteration should introduce an authenticated principal registry (or OIDC/OAuth-backed identity provider), per-principal policy/scope mapping, secret rotation, and a transactional multi-writer store before claiming multi-tenant production readiness.
