# Security Policy

## Scope

This document covers the reference `r2r-mcp` server, Evidence Admission implementation, R2R governance runtime, local persistence profile, and live Jev adapter in this repository.

Security fixes are applied to the latest revision of `main` while the project remains pre-1.0.

## Security model

The core trust boundary is:

```text
untrusted observation / model output
        -> Evidence Admission
        -> deterministic R2R relation transitions
        -> governance decision
        -> external enforcement adapter
```

An MCP caller is not allowed to directly set relation state, source reliability, policy versions, corroborator identity/count, virtual-time authority, or operator identity.

`r2r_decide` is read-only with respect to relation state. `r2r_record_outcome` records caller-supplied outcome data but does not directly turn that data into governance authority.

## Transport profiles

### Local stdio

The current reference server uses stdio and is launched by an MCP host as a child process. It does not bind a public TCP/HTTP listener.

Authentication and request rate limiting are therefore delegated to the local host/process boundary for this profile. This must not be interpreted as a production remote-access security model.

### Future remote transport

A remote Streamable HTTP deployment must add, at minimum:

- authenticated client identity;
- authorization scoped to governance domains and privileged operations;
- TLS;
- request/body limits;
- per-client and/or per-subject rate limiting;
- replay/idempotency protection where mutation is possible;
- audit logging for privileged operations;
- secret-management integration.

Remote mode should not be advertised as production-ready until those controls are implemented and tested.

## Persistence

`MemoryEventStore` is volatile.

`JsonFileEventStore` is a local single-writer reference profile. Startup recovery replays stored observation events and verifies kernel event ids, relation transitions, and state versions before serving decisions.

The JSON store is not intended for concurrent multi-process writers or hostile shared filesystems. Production deployments should use a transactional backend with access control, integrity protection, and appropriate backup/retention controls.

## Fail-closed behavior

A durable-store persistence failure marks the store unhealthy. MCP calls check store health and return an error rather than silently continuing to issue governance decisions from state that was not durably committed.

Startup also fails if the durable event log cannot be reproduced with the available Admission policy or if replay diverges from recorded transitions/state versions.

## Credentials and network access

Fixture mode and local `r2r-mcp` operation do not require an API key.

Live Jev mode reads `TYPESAFE_API_KEY` from the caller environment. The project does not intentionally persist API keys or include them in governance state or provenance output.

`TYPESAFE_ENDPOINT` is caller-controlled, but the live adapter rejects endpoints that do not use the `https://` scheme before constructing a request with the bearer credential. Custom HTTPS endpoints must still be treated as trusted configuration and must not be sourced from untrusted input.

The deterministic fixture path does not require network access. Live Jev mode performs outbound access only when explicitly invoked.

## Governance integrity

Only evidence that returns `Accept(...)` from the versioned Evidence Admission policy can mutate the current demo relation state. `Hold(...)` and `Reject(...)` do not enter governance.

Outcome reports are treated as untrusted audit input in v0.1. Even `policy_breach_confirmed` does not directly mutate relation state without a future Admission rule.

Human override inside the kernel is modeled as an explicit governance event and produces provenance rather than silently resetting state. A privileged MCP override tool is not yet exposed.

## Determinism and recovery

The governance kernel uses deterministic virtual ticks and sequential identifiers rather than wall-clock time or randomness in the relation-transition path.

Durable startup recovery and `r2r_replay` rebuild governance from stored events and trusted Admission snapshots, then verify recorded transitions and state versions.

## Security testing

CI executes Rust unit/integration tests, the offline fixture, the Admission reference binary, real stdio MCP tool calls, deterministic replay, and a cross-process durable restart test.

Important regression classes include:

- probabilistic judgments bypassing Evidence Admission;
- held/rejected evidence mutating relation state;
- low-reliability sources gaining authority through confidence alone;
- expired evidence being admitted;
- domain state leaking across `(subject, scope)` boundaries;
- duplicate public event identity across domains;
- durable restart reverting a suspended authorization to default allow;
- replay divergence being ignored;
- persistence failure silently continuing on volatile state;
- live endpoint configuration allowing bearer credentials over non-HTTPS transport.

## Reporting a vulnerability

Please do not open a public issue for a vulnerability that could expose credentials, bypass an authorization boundary, corrupt governance state, or enable unintended network/file access.

Use GitHub's private vulnerability reporting / Security Advisory flow when available. If private reporting is unavailable, contact the repository maintainer through the public contact information associated with the GitHub account and avoid including exploit details in public channels.

Please include the affected commit/version, reproduction steps, expected vs actual behavior, impact/prerequisites, and any suggested mitigation.

## Third-party verification

Third-party scanners and registries may lag the current repository state or have language-specific analysis limitations. Their scores should be interpreted together with the exact verified commit/code hash and the repository's executable tests.
