# Security Policy

## Scope

This document covers the reference `r2r-mcp` stdio server, the `r2r-mcp-remote` Streamable HTTP profile, Evidence Admission implementation, R2R governance runtime, local persistence profile, execution-binding reference adapter, and live Jev adapter in this repository.

Security fixes are applied to the latest revision of `main` while the project remains pre-1.0.

## Security model

The core trust boundary is:

```text
untrusted observation / model output
        -> Evidence Admission
        -> deterministic R2R relation transitions
        -> governance decision
        -> Decision-State Binding
        -> external enforcement adapter
```

An MCP caller is not allowed to directly set relation state, source reliability, policy versions, corroborator identity/count, virtual-time authority, or operator identity.

`r2r_decide` is read-only with respect to relation state, although it persists an auditable decision. `r2r_record_outcome` records caller-supplied outcome data but does not directly turn that data into governance authority.

A previously issued `ALLOW` is not an indefinitely reusable capability. Execution validation binds the decision to its original action and exact relation-state version; a later state transition invalidates the old decision.

## Transport profiles

### Local stdio

The local reference server uses stdio and is launched by an MCP host as a child process. It does not bind a public TCP/HTTP listener.

Authentication and request rate limiting are therefore delegated to the local host/process boundary for this profile. This must not be interpreted as a production remote-access security model.

### Remote Streamable HTTP v0.1

`r2r-mcp-remote` is an authenticated reference remote profile. It requires a durable event store and applies the following outer security controls before delegating to R2R governance:

- Bearer authentication on every HTTP request;
- a server-configured principal (`R2R_REMOTE_PRINCIPAL`);
- an exact server-side scope allowlist (`R2R_REMOTE_SCOPES`);
- caller-supplied `subject` values are overwritten by the authenticated principal for domain-addressed tools;
- decision-addressed tools verify the persisted decision belongs to the authenticated principal and an allowed scope;
- a process-local fixed-window request rate limit;
- loopback-only plaintext HTTP by default.

The reference Bearer token must contain at least 32 bytes and is compared without an early-exit byte comparison after length equality is established. The secret is configuration and is not intentionally written to relation state or provenance.

The remote binary refuses plaintext HTTP on non-loopback addresses unless `R2R_REMOTE_ALLOW_INSECURE_HTTP=1` is explicitly enabled. That flag is for controlled development only.

Production deployment should terminate TLS at a trusted reverse proxy, ingress, or service-mesh boundary and proxy to the loopback listener. Bearer credentials must not traverse an untrusted plaintext network.

The v0.1 remote profile is deliberately single-principal. It does not yet claim multi-tenant production readiness. A multi-tenant profile additionally needs identity/token rotation, per-principal policy mapping, distributed rate limiting, transactional multi-writer persistence, and operational secret-management integration.

See `docs/r2r-mcp-remote-v0.1.md` for the deployment contract and current limitations.

## Persistence

`MemoryEventStore` is volatile and is used only by the local prototype profile.

`JsonFileEventStore` is a local single-writer reference profile. Startup recovery replays stored observation events and verifies kernel event ids, relation transitions, and state versions before serving decisions.

The remote profile requires durable storage and does not intentionally fall back to `MemoryEventStore`.

The JSON store is not intended for concurrent multi-process writers or hostile shared filesystems. Production deployments should use a transactional backend with access control, integrity protection, and appropriate backup/retention controls.

## Fail-closed behavior

A durable-store persistence failure marks the store unhealthy. MCP calls check store health and return an error rather than silently continuing to issue governance decisions from state that was not durably committed.

Startup also fails if the durable event log cannot be reproduced with the available Admission policy or if replay diverges from recorded transitions/state versions.

Remote requests fail closed on missing/invalid authentication, unauthorized scope, decision ownership mismatch, store errors, rate-limit exhaustion, or stale Decision-State Binding.

## Credentials and network access

Fixture mode and local `r2r-mcp` operation do not require an API key.

Live Jev mode reads `TYPESAFE_API_KEY` from the caller environment. The project does not intentionally persist API keys or include them in governance state or provenance output.

`TYPESAFE_ENDPOINT` is caller-controlled, but the live adapter rejects endpoints that do not use the `https://` scheme before constructing a request with the bearer credential. Custom HTTPS endpoints must still be treated as trusted configuration and must not be sourced from untrusted input.

Remote MCP mode reads `R2R_REMOTE_BEARER_TOKEN` from the process environment. It is used only at the HTTP authentication boundary and is not intentionally persisted.

The deterministic fixture path does not require network access. Live Jev mode performs outbound access only when explicitly invoked. Remote MCP mode binds an inbound HTTP endpoint only when the dedicated `r2r-mcp-remote` binary is started.

## Governance integrity

Only evidence that returns `Accept(...)` from the versioned Evidence Admission policy can mutate the current demo relation state. `Hold(...)` and `Reject(...)` do not enter governance.

Outcome reports are treated as untrusted audit input in v0.1. Even `policy_breach_confirmed` does not directly mutate relation state without a future Admission rule.

Human override inside the kernel is modeled as an explicit governance event and produces provenance rather than silently resetting state. A privileged MCP override tool is not yet exposed.

Remote authentication answers who the caller is and which R2R domains it may address; it does not replace relation governance. An authenticated request can still be denied by R2R state or by stale execution binding.

## Determinism and recovery

The governance kernel uses deterministic virtual ticks and sequential identifiers rather than wall-clock time or randomness in the relation-transition path.

Durable startup recovery and `r2r_replay` rebuild governance from stored events and trusted Admission snapshots, then verify recorded transitions and state versions.

## Security testing

CI executes Rust unit/integration tests, the offline fixture, the Admission reference binary, real stdio MCP tool calls, deterministic replay, a cross-process durable restart test, execution-binding tests, and a process-level authenticated Streamable HTTP smoke test.

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
- stale `ALLOW` decisions remaining executable after relation-state change;
- execution action substitution or caller-supplied version substitution;
- live endpoint configuration allowing bearer credentials over non-HTTPS transport;
- remote callers spoofing `subject` identity;
- remote callers crossing their configured scope boundary;
- decision ids being used across authenticated principals;
- missing or incorrect remote Bearer credentials reaching MCP dispatch;
- accidental plaintext non-loopback remote binding.

## Reporting a vulnerability

Please do not open a public issue for a vulnerability that could expose credentials, bypass an authentication/authorization boundary, corrupt governance state, or enable unintended network/file access.

Use GitHub's private vulnerability reporting / Security Advisory flow when available. If private reporting is unavailable, contact the repository maintainer through the public contact information associated with the GitHub account and avoid including exploit details in public channels.

Please include the affected commit/version, reproduction steps, expected vs actual behavior, impact/prerequisites, and any suggested mitigation.

## Third-party verification

Third-party scanners and registries may lag the current repository state or have language-specific analysis limitations. Their scores should be interpreted together with the exact verified commit/code hash and the repository's executable tests.
