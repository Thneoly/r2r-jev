# Architecture

`r2r-jev` separates **probabilistic judgment**, **evidence admission**, **deterministic governance**, and **enforcement**.

## Core flow

```text
Agent / environment state
        │
        ▼
       Jev
        │
        │ typed answers + probabilities
        ▼
JudgmentObserved
        │
        ▼
Typed evidence candidates
        │
        ▼
┌───────────────────────────────┐
│ Evidence Admission v0.1       │
│                               │
│ kind                          │
│ confidence                    │
│ source reliability            │
│ corroboration                 │
│ virtual-time expiry           │
│                               │
│ Reject / Hold / Accept        │
└───────────────┬───────────────┘
                │ Accept only
                ▼
EvidenceAdmitted
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

The architectural invariant is:

```text
Judgment -> Evidence -> Admission -> R2R -> Enforcement
```

not:

```text
Judgment -> Permission
```

## Why Admission is a separate layer

A Jev answer is an observation with uncertainty. It is not authoritative governance state.

For example:

```text
beyond_scope = 0.94
```

must not directly become:

```text
Authorization = Suspended
```

Evidence Admission v0.1 first asks whether the observation is eligible to enter governance at all.

```text
beyond_scope = 0.94
        │
        ▼
EvidenceObserved
        │
        ├── kind = BeyondScope
        ├── confidence = 940000 ppm
        ├── source reliability = trusted runtime metadata
        ├── corroborators = trusted provenance metadata
        └── expiry = virtual-time metadata
        │
        ▼
Admission
        ├── Reject
        ├── Hold
        └── Accept
```

Only `Accept` emits governance-active evidence.

`Hold` is deliberately non-authoritative: it can wait for corroboration, review, contradiction, or expiry without mutating persistent relations.

## Evidence Admission v0.1

The executable policy is defined in [`src/admission.rs`](../src/admission.rs) and specified in [`docs/evidence-admission-semantics-v0.1.md`](evidence-admission-semantics-v0.1.md).

The v0.1 decision order is:

```text
unsupported kind
    -> Reject

expired
    -> Reject

source reliability below floor
    -> Reject

strong confidence + strong source reliability
    -> Accept(Strong)

medium confidence + reliable source + >=2 independent corroborators
    -> Accept(Corroborated)

review-level confidence but insufficient support
    -> Hold

otherwise
    -> Reject
```

The policy intentionally does **not** multiply probability and source reliability or claim a Bayesian interpretation. They are separate deterministic policy dimensions.

Source reliability, corroboration identity/count, and expiry are bound by the trusted adapter/runtime. The probabilistic model cannot self-assert them.

## Admission does not decide the Relation effect

`Accept` emits an `EvidenceAdmitted` input. It still does not directly mutate authorization.

The current public demo rule pack maps admitted `BeyondScope` or `DestructiveAction` evidence to this protective chain:

```text
EvidenceAdmitted
        ↓
Trust: Active -> Warning
        ↓
Delegation: Active -> Degraded
        ↓
Authorization: Active -> Suspended
```

That mapping belongs to R2R semantics, not to Jev and not to Admission.

A different rule pack could react to the same admitted evidence by creating supervision, narrowing scope, or requesting review instead of suspending authorization.

Formally:

```text
Admission(e) != AuthorizationDecision(e)
```

and:

```text
EvidenceAdmitted(e) + RelationGraph(t)
    -> R2R rules
    -> RelationGraph(t+1)
```

## Deterministic boundary

The live Jev API returns floating-point probabilities. The adapter converts them to integer parts-per-million (ppm):

```text
0.94 -> 940000
```

From that boundary onward, the demo uses integer values, explicit virtual ticks, versioned admission semantics, deterministic ids, and explicit relation transitions.

The same evidence envelope + admission policy version + virtual tick produces the same admission result.

## Current three-act demo

The fixture demo now runs through Admission v0.1.

```text
Act 1
  Jev-style judgment:
    BeyondScope = 0.94
    Destructive = 0.72

  Admission:
    BeyondScope       -> Accept(Strong)
    DestructiveAction -> Hold(NeedsCorroborationOrReview)

  Only the accepted evidence reaches R2R:
    Trust         Active -> Warning
    Delegation    Active -> Degraded
    Authorization Active -> Suspended

  -> DENY current call

Act 2
  A later benign judgment produces only rejected evidence.
  No new Relation transition occurs.
  The earlier suspended Authorization still governs the call.

  -> DENY

Act 3
  human_override
      -> repairs Authorization
      -> creates Supervision

  -> ALLOW under supervision
```

This distinguishes two forms of memory:

1. **evidence history** — what was observed and how Admission classified it;
2. **governance state** — what accepted evidence caused the Relation Graph to become.

## Provenance

The runtime records both admission and relation causality:

```text
ev-0001
  -> evidence-0001[BeyondScope:Accept(Strong)]
  -> trust-0001
  -> delegation-0001
  -> authorization-0001

ev-0001
  -> evidence-0002[DestructiveAction:Hold(...)]
ev-0001 [via authorization-0001]
  -> DENY(merge_pull_request)
```

A held or rejected evidence record remains explainable without becoming authoritative.

## Falsification boundary

The earlier direct-threshold model remains useful as a baseline:

```text
score >= threshold
    -> persistent suspension
```

Admission v0.1 is a hypothesis about reducing false-positive persistence while preserving history-sensitive governance when evidence is sufficiently supported.

The repository therefore keeps the stateless/stateful experiments and adds an Admission experiment rather than claiming that v0.1 is universally superior.

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

The integration target is therefore:

```text
probabilistic judgment
    -> evidence admission
    -> persistent relation governance
    -> arbitrary enforcement
```
