# Evidence Admission Semantics v0.1

Status: **experimental normative draft**

## 1. Problem

A probabilistic judgment is not authority.

The unsafe shortcut is:

```text
Jev score >= threshold
        ↓
Authorization = Suspended
```

That makes a model output behave like a permission mutation. R2R needs an explicit boundary between **observation** and **governance state**.

Evidence Admission Semantics v0.1 introduces that boundary:

```text
Judgment
   ↓
EvidenceObserved
   ↓
Admission
   ├── Reject
   ├── Hold
   └── Accept
          ↓
   EvidenceAdmitted
          ↓
   typed R→R rules
          ↓
Relation state transition
```

The central rule is:

> **A judgment may produce evidence. Only admitted evidence may become an input to deterministic relation transitions.**

Even `Accept` does not mutate Trust, Delegation, Authorization, or any other relation directly. It emits a typed `EvidenceAdmitted` event. Relation transitions remain the responsibility of the R2R runtime.

---

## 2. Evidence envelope

An observed judgment is wrapped in an evidence envelope:

\[
e = \langle id, kind, subject, scope, source, c, r, k, t_o, t_x \rangle
\]

where:

- `id` — stable evidence identifier;
- `kind` — typed claim such as `BeyondScope` or `DestructiveAction`;
- `subject` — governed Agent or actor;
- `scope` — repository/resource/task scope;
- `source` — judgment provider identity;
- `c` — confidence, represented as integer parts-per-million (`0..1_000_000`);
- `r` — source reliability assigned by trusted configuration, also integer ppm;
- `k` — independent corroboration count;
- `t_o` — observation virtual tick;
- `t_x` — expiry virtual tick.

Important trust rule:

> `source_reliability_ppm`, corroboration identity/count, observation tick, and expiry are **not self-asserted by the probabilistic model**. They are assigned or verified by the adapter/runtime boundary.

The deterministic kernel never needs floating point.

---

## 3. Admission result

Admission produces one of three typed outcomes:

```text
Reject(reason)
Hold(reason)
Accept(class)
```

### Reject

The evidence is retained for audit if desired, but it cannot participate in governance transitions.

Typical reasons:

- expired evidence;
- unsupported evidence kind;
- source reliability below the minimum trust floor;
- confidence below the review floor.

### Hold

The evidence is plausible but insufficient to become governance-active.

`Hold` is intentionally non-authoritative. It may:

- wait for another independent source;
- wait for human review;
- wait for later evidence;
- expire without affecting Relation state.

### Accept

The evidence is admitted into the deterministic governance plane and produces:

```text
EvidenceAdmitted {
    evidence_id,
    kind,
    subject,
    scope,
    class
}
```

`class` in v0.1 is:

```text
Strong
Corroborated
```

`EvidenceAdmitted` is an **input event**, not a direct state mutation.

---

## 4. Deterministic v0.1 policy

The initial policy is intentionally small and falsifiable. It is not claimed to be a universal trust model.

Constants:

```text
MIN_SOURCE_RELIABILITY = 600_000 ppm
MIN_REVIEW_CONFIDENCE   = 600_000 ppm
STRONG_CONFIDENCE       = 900_000 ppm
STRONG_RELIABILITY      = 800_000 ppm
CORROBORATED_CONFIDENCE = 700_000 ppm
CORROBORATED_RELIABILITY= 700_000 ppm
MIN_CORROBORATORS       = 2
```

Admission is evaluated in this order:

```text
1. if kind is unsupported
      → Reject(UnsupportedKind)

2. if now_vtick > expires_vtick
      → Reject(Expired)

3. if source_reliability < 600_000
      → Reject(SourceBelowTrustFloor)

4. if confidence >= 900_000
      and source_reliability >= 800_000
      → Accept(Strong)

5. if confidence >= 700_000
      and source_reliability >= 700_000
      and independent_corroborators >= 2
      → Accept(Corroborated)

6. if confidence >= 600_000
      → Hold(NeedsCorroborationOrReview)

7. otherwise
      → Reject(InsufficientSupport)
```

The ordering is normative for v0.1.

There is deliberately no multiplication of `confidence × reliability` and no claim that the inputs form a Bayesian posterior. They are separate policy dimensions.

---

## 5. Relation context is downstream, not hidden inside admission

Admission decides whether evidence is eligible to enter governance. It does **not** decide the final Relation effect.

For example:

```text
EvidenceAdmitted(kind=BeyondScope, class=Strong)
        ↓
R2R rule pack
        ↓
EvidenceRelation supports RiskRelation
        ↓
RiskRelation weakens TrustRelation
        ↓
TrustRelation constrains DelegationRelation
        ↓
DelegationRelation suspends AuthorizationRelation
```

Another rule pack could legitimately react differently:

```text
EvidenceAdmitted(BeyondScope, Strong)
        ↓
create Supervision
        ↓
keep Authorization Active but constrained
```

Therefore:

\[
Admission(e) \neq AuthorizationDecision(e)
\]

and:

\[
EvidenceAdmitted(e) \rightarrow R2R(e, G_t) \rightarrow G_{t+1}
\]

The graph context `G_t` remains part of the R2R transition semantics.

---

## 6. Required invariants

### A1 — No direct probabilistic mutation

A Jev/model result MUST NOT directly set Relation phase or fields.

```text
Judgment → RelationMutation    // forbidden
```

### A2 — Hold is non-authoritative

`Hold` MUST NOT cause a governance Relation transition by itself.

### A3 — Expired evidence is inert

Evidence with `now_vtick > expires_vtick` MUST NOT be admitted.

### A4 — Reliability is externally bound

The model MUST NOT choose its own `source_reliability_ppm`.

### A5 — Corroboration means independent provenance

`independent_corroborators` MUST count distinct trusted provenance identities, not repeated samples from the same source.

### A6 — Replay determinism

Given the same evidence envelope, admission policy version, and virtual tick, the admission result MUST be identical.

### A7 — Admission is versioned evidence processing

Every `EvidenceAdmitted` / `EvidenceHeld` / `EvidenceRejected` record SHOULD include `admission_policy_version = "0.1"` so replay can preserve historical semantics after policy evolution.

---

## 7. Why `Hold` matters

Without `Hold`, the system is forced into a binary choice:

```text
ignore evidence
or
change governance state
```

That makes false positives expensive.

`Hold` creates a third state:

```text
Judgment
   ↓
plausible but insufficient
   ↓
Hold
   ├── corroborated later → Accept
   ├── human confirms     → Accept
   ├── contradicted       → Reject
   └── expires            → Reject
```

This is the main v0.1 mechanism for limiting the temporal blast radius of a single uncertain judgment.

---

## 8. Example cases

| Case | confidence | reliability | corroborators | expiry | Result |
|---|---:|---:|---:|---|---|
| strong trusted signal | 940k | 850k | 0 | valid | `Accept(Strong)` |
| strong but untrusted source | 970k | 400k | 0 | valid | `Reject(SourceBelowTrustFloor)` |
| medium + two independent sources | 760k | 800k | 2 | valid | `Accept(Corroborated)` |
| medium, no corroboration | 760k | 800k | 0 | valid | `Hold(NeedsCorroborationOrReview)` |
| stale evidence | 990k | 990k | 3 | expired | `Reject(Expired)` |
| low support | 420k | 900k | 3 | valid | `Reject(InsufficientSupport)` |

The second and fifth rows are intentional counterexamples to `confidence alone ⇒ authority`.

---

## 9. Non-goals of v0.1

v0.1 does not claim to solve:

- calibration of Jev probabilities;
- learning source reliability automatically;
- Bayesian evidence fusion;
- adversarial collusion between corroborators;
- evidence decay functions beyond explicit expiry;
- organization-level reconciliation;
- optimal thresholds;
- automatic human-override policy.

These remain experimentable extensions rather than hidden assumptions.

---

## 10. Falsifiable hypotheses

### EA-H1 — Admission reduces false-positive persistence

Compared with `score >= threshold → persistent suspension`, an admission layer with `Hold` should reduce the number/duration of persistent false-deny states under uncertain single-source judgments.

### EA-H2 — Admission preserves persistent-governance benefit

Under independently confirmed governance violations, `Accept(Corroborated)` should preserve the history-sensitive enforcement behavior of Stateful R2R.

### EA-H3 — Explicit provenance improves replay/explanation

An execution trace containing:

```text
JudgmentObserved
→ EvidenceHeld/Admitted/Rejected
→ RelationTransition
→ AuthorizationDecision
```

should mechanically explain more governance decisions than a direct threshold gate with equivalent judgments.

None of these are considered established by this specification alone.

---

## 11. Minimal execution path

The public executable for v0.1 should demonstrate at least these cases:

```text
A. high confidence + trusted source       → Accept(Strong)
B. high confidence + untrusted source     → Reject
C. medium confidence + corroboration      → Accept(Corroborated)
D. medium confidence, one source          → Hold
E. expired evidence                       → Reject
F. low confidence                         → Reject
```

Only A/C emit `EvidenceAdmitted`; only downstream R2R rules are allowed to change long-lived governance Relations.

---

## 12. Evolution rule

Future versions may introduce richer evidence combination, decay, provenance graphs, source reputation, or human review, but they must preserve the architectural separation:

\[
\boxed{
Judgment \rightarrow Evidence \rightarrow Admission \rightarrow R2R \rightarrow Enforcement
}
\]

not:

\[
\boxed{
Judgment \rightarrow Permission
}
\]
