# Security Policy

## Scope

`r2r-jev` is a research integration demonstrating a deterministic governance boundary around probabilistic Jev judgments.

The repository currently provides a Rust CLI/demo and supporting experiments. It does **not** currently expose a standalone MCP server endpoint or a production authorization service.

The security-sensitive boundary is:

```text
probabilistic judgment
    -> Evidence Admission
    -> persistent R2R relation state
    -> enforcement decision
```

A Jev score is treated as evidence, never as authority by itself.

## Supported versions

Security fixes are applied to the latest revision of the `main` branch while the project remains pre-1.0.

## Reporting a vulnerability

Please do not open a public issue for a vulnerability that could expose credentials, bypass an authorization boundary, corrupt persistent governance state, or enable unintended network/file access.

Instead, use GitHub's private vulnerability reporting / Security Advisory flow for this repository when available. If private reporting is unavailable, contact the repository maintainer through the public contact information associated with the GitHub account and avoid including exploit details in public channels.

Please include:

- affected commit/version;
- reproduction steps or a minimal proof of concept;
- expected vs actual security behavior;
- impact and prerequisites;
- any suggested mitigation.

## Security assumptions and boundaries

### Credentials

Live mode reads `TYPESAFE_API_KEY` from the caller environment. The fixture path does not require a network credential.

The project does not intentionally persist API keys or include them in governance state or provenance output.

`TYPESAFE_ENDPOINT` is caller-controlled, but the live adapter rejects endpoints that do not use the `https://` scheme before constructing a request with the bearer credential. Custom HTTPS endpoints must still be treated as trusted configuration and must not be sourced from untrusted input.

### Network access

The deterministic fixture path is intended to run without network access.

Live Jev mode performs an outbound request only to an HTTPS endpoint accepted by the adapter. No inbound network listener is provided by this repository.

### Filesystem access

The Rust runtime in this repository does not require arbitrary filesystem access for the governance path. Demo fixtures and experiment inputs are repository-local inputs selected by the caller.

### Governance integrity

Only evidence that returns `Accept(...)` from the versioned Evidence Admission policy is allowed to mutate persistent demo relation state. `Hold(...)` and `Reject(...)` do not enter governance.

Human override is modeled as an explicit governance event and creates provenance rather than silently resetting state.

### Determinism

The governance kernel uses deterministic virtual ticks and sequential identifiers rather than wall-clock time or randomness. Replaying the same event sequence with the same policy version and trusted admission context is expected to reproduce the same result.

## Security testing

CI executes the Rust test suite and the offline fixture path. The repository also keeps a top-level `tests/` contract test so external security/indexing tools can detect executable test coverage in addition to module-local Rust unit tests.

Important classes of regression include:

- probabilistic judgments bypassing Evidence Admission;
- rejected or held evidence mutating relation state;
- low-reliability sources gaining authority through confidence alone;
- expired evidence being admitted;
- replay becoming nondeterministic;
- human override failing to produce provenance/supervision state;
- fixture execution unexpectedly requiring credentials or network access;
- live endpoint configuration allowing bearer credentials to be sent over non-HTTPS transports.

## Third-party trust indexes

Third-party scanners and registries may index this public repository independently. Their labels, scores, classifications, and scan timestamps are external assessments and may lag the current repository state.

If an external index classifies `r2r-jev` as an MCP server, note that the current repository is an integration/governance research project and does not presently implement a standalone MCP server transport.
