# Privacy Policy

Effective date: 2026-09-25

This policy describes the reference `r2r-jev` repository and its `r2r-mcp` executable.

## Local MCP mode

`r2r-mcp` currently uses local stdio transport. In its default configuration it does not expose a public network listener and does not send MCP observations, decisions, outcomes, or replay data to an external service.

The MCP host supplies governance inputs such as subject, scope, task, action, intent, and probabilistic judgment values. The runtime processes those values locally.

## Storage

By default, `r2r-mcp` uses an in-memory event store. Data is lost when the process exits.

If `R2R_STORE_PATH` is configured, the server stores its local governance snapshot at the specified filesystem path. The snapshot may contain:

- subject and scope identifiers;
- task, action, and intent strings supplied to `r2r_observe`;
- probabilistic judgment values;
- trusted Admission metadata bound by the runtime;
- relation transitions and provenance;
- decisions and execution outcomes.

The operator of the MCP host is responsible for choosing an appropriate local storage path, filesystem permissions, retention period, backup policy, and deletion procedure.

## Live Jev mode

The separate CLI live mode is opt-in. When a user explicitly runs the live adapter with a configured `TYPESAFE_API_KEY`, inputs required for the judgment request may be sent to the configured Jev/TypeSafe service. Users should review the privacy terms of that external service before using live mode with sensitive data.

The recorded fixture path does not require a network request.

## Credentials

The project does not require API keys for fixture mode or local `r2r-mcp` operation. Live Jev credentials are read from the environment. They should not be committed to the repository or included in governance event payloads.

## Telemetry

The reference implementation does not add analytics or tracking telemetry of its own.

## Authentication and remote access

The current MCP reference server is a local stdio process launched by the MCP host. It does not provide an anonymous public HTTP endpoint. A future remote transport must define authentication, authorization, transport security, retention, and rate-limit policy before it is treated as a production deployment profile.

## Data deletion

For in-memory mode, stopping the process removes runtime data.

For `R2R_STORE_PATH` mode, delete the configured local store file and any operator-created backups to remove the persisted reference data.

## Changes

Material privacy changes should be documented in this file together with the corresponding code change.

## Contact

For questions or security/privacy reports, use the repository's GitHub issue or security-reporting channels described in `SECURITY.md`.
