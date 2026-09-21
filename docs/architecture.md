# Architecture

`r2r-jev` deliberately separates **probabilistic judgment** from **deterministic governance**.

## Core flow

```text
Agent / environment state
        │
        ▼
       Jev
        │
        │ typed answer + probability
        ▼
JudgmentObserved
        │
        │ adapter boundary
        ▼
Evidence Relation
        │
        │ typed R -> R transitions
        ▼
Trust / Delegation / Authorization / Supervision
        │
        ▼
Enforcement adapter (for example MCP)
        │
        ▼
Real tool execution
```

## Why the boundary matters

A Jev answer is not authoritative state. It is an observation with uncertainty.

For example:

```text
beyond_scope = 0.94
```

should not directly become:

```text
Authorization = Suspended
```

Instead it becomes evidence:

```text
Jev judgment
   ↓
JudgmentObserved
   ↓
EvidenceRelation
   ↓
R2R admission / transition semantics
   ↓
Trust
   ↓
Delegation
   ↓
Authorization
```

This preserves a clear separation between:

- **Judge** — what seems true now;
- **Govern** — what persistent state changes are admitted;
- **Act** — what the system is allowed to execute.

## Deterministic boundary

The live Jev API returns floating-point probabilities. The adapter converts them to integer parts-per-million (ppm):

```text
0.94 -> 940000
```

The demo governance kernel then uses only integer state and explicit thresholds.

This is a small demonstration of a broader R2R principle:

> Probabilistic systems may provide evidence and proposals, while governance state transitions remain explicit, replayable, and inspectable.

## Current demo semantics

The public demo intentionally uses a small transition chain, played out in
three acts over a persistent state:

```text
Act 1  BeyondScopeEvidence
          ↓ weakens
       Trust: Active -> Warning
          ↓ degrades
       Delegation: Active -> Degraded
          ↓ constrains
       Authorization: Active -> Suspended      -> DENY current call

Act 2  A later, below-threshold judgment is still admitted as evidence,
       but the decision inherits the suspended authorization
       -> DENY a call a stateless gate would ALLOW

Act 3  human_override (itself a governance event)
          ↓ repairs
       Authorization: Suspended -> Active
          ↓ creates
       Supervision relation (supervisor = the human)
       -> ALLOW under supervision
```

Every admitted event, evidence record, and relation transition receives a
deterministic sequential id (`ev-0001`, `evidence-0001`, `trust-0001`, ...),
and the demo prints the causal provenance chain that connects them. There are
no clocks and no randomness: replaying the same event sequence reproduces the
same ids and the same output byte for byte.

The full R2R project explores richer typed relations, conflicts, propagation boundaries, reconciliation, replay, and enforcement.

## Enforcement is replaceable

`r2r-jev` does not bind governance to MCP. The same resulting authorization state could feed:

```text
MCP gateway
HTTP gateway
A2A gateway
agent framework hook
Kubernetes admission
CI/CD control point
```

The integration target is therefore not "Jev + MCP". It is:

```text
probabilistic judgment -> persistent governance state -> arbitrary enforcement
```
