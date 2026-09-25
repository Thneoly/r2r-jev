# R2R MCP Product Surface v0.1

Status: design proposal

## 1. Product definition

R2R MCP is the MCP-facing governance surface for Relation-to-Relation (R2R).

It is **not** a generic MCP gateway and does not execute arbitrary downstream tools itself. Its role is to expose a small, typed, auditable interface through which an MCP host or agent runtime can:

1. submit observations and proposed actions;
2. query persistent relation state;
3. obtain a deterministic governance decision before execution;
4. record execution outcomes as governance events;
5. inspect provenance and replay state.

The architectural invariant remains:

```text
Judgment -> Evidence -> Admission -> R2R -> Enforcement
```

MCP is an integration surface around this invariant, not an authority that bypasses it.

## 2. Product positioning

```text
MCP Host / Agent Runtime
        │
        │ MCP tools + resources
        ▼
┌──────────────────────────────┐
│          R2R MCP             │
│ governance control surface   │
└──────────────┬───────────────┘
               │
      ┌────────┼─────────┐
      ▼        ▼         ▼
 Admission   R2R      Provenance
            Runtime     / Replay
      │        │
      └────┬───┘
           ▼
      Decision State
           │
           ▼
    Enforcement Adapter
           │
           ▼
       Real Tool/API
```

R2R MCP should be understood as an **agent governance control plane**.

## 3. Design principles

### 3.1 No direct relation mutation

Do not expose tools such as:

```text
set_authorization("active")
set_trust("warning")
```

Relations may change only through typed governance events processed by Admission + R2R rules.

### 3.2 Evidence is not authority

A caller may submit observations, judgments, tool outcomes, and review events, but cannot directly assert authoritative relation state.

### 3.3 Decide is side-effect free

Asking whether an action is allowed must not mutate relation state.

### 3.4 Explicit provenance

Every state-changing result must expose:

- input event id;
- admission result;
- policy/rule-pack versions;
- relation transition ids;
- resulting state version.

Every decision must expose the governing relation(s) that caused it.

### 3.5 Deterministic replay

The same ordered events + policy versions + trusted admission context must reproduce the same resulting relation state.

### 3.6 Least authority

R2R MCP should not accept arbitrary shell commands, filesystem paths, downstream bearer tokens, or unbounded code as governance inputs.

## 4. MCP capability groups

v0.1 uses two core MCP surfaces:

```text
Tools      -> governance operations
Resources  -> inspect durable state/provenance
```

Prompts are optional operator UX and are not part of the enforcement contract.

## 5. Tool surface v0.1

### 5.1 `r2r_observe`

Submit typed evidence candidates or a normalized judgment event.

**Purpose:** ingestion boundary.

**Must not:** directly grant/revoke authority.

Input shape:

```json
{
  "subject": "agent:coder-1",
  "scope": "repo:alpha",
  "action": "github.merge_pull_request",
  "resource": "pr:42",
  "intent": "merge PR 42",
  "observations": [
    {
      "kind": "BeyondScope",
      "confidence_ppm": 940000,
      "source": "jev"
    }
  ],
  "idempotency_key": "client-event-123"
}
```

Trusted admission metadata such as source reliability, corroboration identity/count, and expiry belongs to the trusted runtime/adapter boundary and must not be self-asserted by an untrusted model.

Output includes:

```text
event_id
admission results
relation transitions
state_version
```

### 5.2 `r2r_decide`

Evaluate whether a proposed action is currently allowed by persistent relation state.

This is the primary **pre-execution** tool.

Input:

```json
{
  "subject": "agent:coder-1",
  "scope": "repo:alpha",
  "action": "github.merge_pull_request",
  "resource": "pr:42",
  "task": "fix login redirect bug"
}
```

Output:

```json
{
  "decision": "DENY",
  "reason_code": "AUTHORIZATION_SUSPENDED",
  "decision_id": "decision-0068",
  "governing_relations": [
    {
      "relation_id": "authorization-0017",
      "type": "Authorization",
      "state": "Suspended"
    }
  ],
  "required_next_step": {
    "type": "human_review"
  },
  "state_version": "state-000017"
}
```

`r2r_decide` is pure with respect to persistent relation state.

### 5.3 `r2r_record_outcome`

Record what actually happened after an action decision.

Examples:

```text
executed
blocked
failed
rolled_back
user_corrected
policy_breach_confirmed
```

This closes the control loop:

```text
propose -> decide -> execute/block -> record outcome -> update relations
```

### 5.4 `r2r_override`

Submit a privileged human/operator governance event.

This is **not** `set_state`. The rule pack decides how an override changes relations.

Example operations:

```text
restore_under_supervision
narrow_scope
approve_once
expire_suspension
revoke_delegation
```

This tool requires stronger authorization than ordinary agent-facing tools.

### 5.5 `r2r_explain`

Explain why a decision or relation state exists without changing state.

Accept one of:

```text
decision_id
relation_id
event_id
state_version
```

Return both:

1. a structured causal chain (normative);
2. a natural-language summary (convenience).

Example:

```text
ev-0042
  -> evidence-0091[BeyondScope:Accept(Strong)]
  -> authorization-0017[Active -> Suspended]
  -> decision-0068[DENY]
```

### 5.6 `r2r_replay`

Replay a bounded event range using explicit policy/rule-pack versions and verify the resulting state hash/version.

Output should identify:

```text
replay_match
recorded_state_version
replayed_state_version
first_divergent_event (if any)
policy_versions
```

### 5.7 `r2r_get_relation`

Read a typed relation or relation set.

Typical filters:

```text
subject
scope
resource
relation_type
state_version
```

This is read-only.

## 6. What should NOT be a public MCP tool

The following interfaces are explicitly excluded:

```text
r2r_set_relation
r2r_set_authorization
r2r_delete_history
r2r_edit_provenance
r2r_execute_tool
r2r_eval_arbitrary_policy_code
r2r_set_source_reliability_from_model
```

Reason: these bypass the R2R causality model or enlarge the authority boundary unnecessarily.

## 7. Resource surface v0.1

Resources expose durable governance state without turning reads into imperative tool calls.

Suggested URI families:

```text
r2r://subjects/{subject}/relations
r2r://scopes/{scope}/relations
r2r://relations/{relation_id}
r2r://events/{event_id}
r2r://decisions/{decision_id}
r2r://states/{state_version}
r2r://provenance/{event_id}
r2r://policies/admission/{version}
r2r://policies/rules/{version}
```

Resources should be immutable by URI wherever possible. Historical state versions must remain readable for replay/audit.

## 8. Decision contract

The key R2R MCP contract is not merely `ALLOW/DENY`.

A decision is:

```text
Decision = {
  verdict,
  reason_code,
  governing_relations,
  state_version,
  policy_versions,
  provenance_ref,
  required_next_step?
}
```

Initial verdict set:

```text
ALLOW
DENY
REQUIRE_REVIEW
ALLOW_UNDER_SUPERVISION
```

The verdict taxonomy must remain finite and versioned.

## 9. Identity and relation model

v0.1 should normalize identifiers instead of accepting free-form entity semantics.

Suggested namespaces:

```text
agent:<id>
human:<id>
team:<id>
service:<id>
repo:<id>
resource:<type>:<id>
tool:<provider>:<name>
task:<id>
```

Core relations:

```text
Trust
Delegation
Authorization
Supervision
Ownership
Scope
Responsibility
```

The MCP layer serializes these relations; the R2R runtime owns their semantics.

## 10. Trusted vs untrusted input boundary

This boundary is critical.

Untrusted/model-supplied:

```text
intent
proposed action
task text
probabilistic judgment
observed result
```

Trusted runtime/operator supplied:

```text
authenticated subject identity
source reliability
corroborator identity
policy version
virtual time / expiry authority
operator identity
state handle
```

An MCP client must not be able to promote untrusted data into trusted admission metadata by placing it in JSON.

## 11. Transport and deployment profiles

### Profile A — Local stdio

Best initial developer experience:

```text
Claude/Codex/IDE
     │ stdio
     ▼
   r2r-mcp
     │
 local R2R store
```

Properties:

- no inbound network listener;
- easy local installation;
- single-user trust boundary;
- suitable for v0.1 reference implementation.

### Profile B — Remote Streamable HTTP

For teams and shared governance state:

```text
Agent Hosts
    │ HTTPS + auth
    ▼
 R2R MCP Service
    │
 shared relation store
```

Requires:

- authenticated identity;
- per-tool authorization;
- tenant/scope isolation;
- audit logging;
- TLS-only endpoints;
- replayable persistent storage.

## 12. MCP-native security rules

1. `r2r_override` must never be available to an unauthenticated model identity.
2. trusted admission metadata must be server-derived or adapter-bound.
3. `r2r_decide` must not produce side effects.
4. historical provenance must be append-only from the public API perspective.
5. repeated state-changing requests require idempotency keys.
6. decisions should support optimistic state version checks to avoid TOCTOU races.
7. remote mode should fail closed when identity or state version cannot be established.
8. external credentials for downstream tools are outside the R2R MCP tool schema.

## 13. TOCTOU / enforcement binding

A governance decision can become stale between decision and execution.

Therefore enforcement adapters should bind execution to the decision state:

```text
r2r_decide
  -> decision_id + state_version
  -> enforcement adapter
  -> verify decision/state binding
  -> execute
  -> r2r_record_outcome
```

For sensitive actions, execution should fail if the relevant relation state has changed since the decision was issued.

## 14. Storage abstraction

The MCP product surface must not assume one storage backend.

Reference runtime interface:

```text
EventStore
RelationStore
DecisionStore
SnapshotStore
PolicyRegistry
```

v0.1 can begin with an in-memory or local append-only store, but tool schemas must already be compatible with a persistent backend.

## 15. Suggested Rust package layout

```text
src/
├── admission.rs
├── model.rs
├── r2r.rs
├── jev.rs
├── mcp/
│   ├── mod.rs
│   ├── server.rs
│   ├── tools.rs
│   ├── resources.rs
│   ├── schema.rs
│   ├── auth.rs
│   └── error.rs
├── store/
│   ├── mod.rs
│   ├── memory.rs
│   └── event_log.rs
└── bin/
    ├── r2r-mcp.rs
    └── ...
```

Reference implementation should use the official Rust MCP SDK (`rmcp`) rather than implementing the protocol by hand.

## 16. MVP cut

The first real MCP release should implement only:

```text
r2r_observe
r2r_decide
r2r_record_outcome
r2r_explain
r2r_get_relation
```

plus resources for relations, events, decisions, and policy versions.

`r2r_override` and `r2r_replay` should be implemented immediately after the base persistence model is stable because they carry stronger privilege/audit requirements.

## 17. End-to-end example

```text
1. Agent proposes github.merge_pull_request

2. Host calls r2r_decide
   -> ALLOW
   -> decision-100 / state-42

3. Enforcement adapter executes merge

4. Adapter calls r2r_record_outcome
   -> succeeded
   -> event-101

5. Later Jev observes suspicious BeyondScope behavior

6. Adapter calls r2r_observe
   -> evidence accepted
   -> Trust Active -> Warning
   -> Delegation Active -> Degraded
   -> Authorization Active -> Suspended
   -> state-43

7. Agent proposes another merge

8. r2r_decide
   -> DENY
   -> governed by authorization-17

9. Operator inspects r2r_explain

10. Operator may submit privileged r2r_override
    -> Authorization restored under Supervision
```

## 18. Product boundary statement

The canonical public description should be:

> **R2R MCP is a governance control plane for AI agents and tool ecosystems. It turns admitted evidence and execution history into persistent, replayable relationship state, then exposes deterministic pre-execution decisions with provenance.**

Short form:

> **Persistent relationship governance for agent actions.**

## 19. v0.1 success criteria

The product surface is successful when a third-party MCP host can demonstrate all of the following without understanding R2R internals:

1. ask whether an agent may perform an action;
2. see which persistent relation governed the answer;
3. submit new evidence without directly mutating authority;
4. observe a later decision change because relation state changed;
5. inspect the causal chain;
6. replay the same events and reproduce the same state.

That is the minimum externally legible proof that R2R is more than a stateless policy check.