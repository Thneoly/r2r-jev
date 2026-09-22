# Evidence Admission v0.1 experiment

This experiment exercises the executable reference implementation in:

```text
src/bin/admission.rs
```

It tests the boundary:

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
R2R rules
```

The purpose is **not** to benchmark Jev accuracy. The purpose is to verify that probabilistic confidence alone cannot directly mutate governance state.

## Run

```bash
cargo run --bin admission
cargo test --bin admission
```

`cargo test --all-targets` should also include this binary automatically.

## Cases

| Case | Input shape | Expected admission | Why it matters |
|---|---|---|---|
| A | confidence=940k, reliability=850k | `Accept(Strong)` | strong trusted evidence can enter governance |
| B | confidence=970k, reliability=400k | `Reject(SourceBelowTrustFloor)` | confidence cannot manufacture source authority |
| C | confidence=760k, reliability=800k, 2 corroborators | `Accept(Corroborated)` | independent corroboration can admit a medium signal |
| D | confidence=760k, reliability=800k, 0 corroborators | `Hold(NeedsCorroborationOrReview)` | uncertain evidence is neither ignored nor made authoritative |
| E | confidence=990k but expired | `Reject(Expired)` | stale evidence is governance-inert |
| F | confidence=420k | `Reject(InsufficientSupport)` | weak evidence does not enter governance |
| G | unsupported kind, confidence=999k | `Reject(UnsupportedKind)` | unknown semantics cannot be smuggled in through a high score |

## Required properties

The executable tests these properties directly:

1. high confidence cannot override an untrusted source;
2. corroboration can admit a medium-strength signal;
3. medium single-source evidence is held, not authorized;
4. expired evidence remains inert even at very high confidence;
5. unknown evidence kinds are rejected before score evaluation;
6. the same evidence + policy + virtual tick produces the same decision;
7. only `Accept` produces an `EvidenceAdmitted` record.

## What happens after `Accept`

Nothing in this experiment directly mutates `Trust`, `Delegation`, `Authorization`, or `Supervision`.

The output is only:

```text
EvidenceAdmitted {
  evidence_id,
  kind,
  subject,
  scope,
  class,
  admission_policy_version: "0.1"
}
```

A downstream R2R rule pack decides how the current Relation Graph should respond.

That separation is intentional:

```text
Admission(e) != AuthorizationDecision(e)
```

## Falsification direction

The most important next comparison is against the earlier direct threshold persistence baseline:

```text
Baseline:
score >= threshold
    ↓
Authorization Suspended
```

versus:

```text
Admission v0.1:
score
  ↓
Evidence
  ↓
Accept / Hold / Reject
  ↓
only accepted evidence may enter R2R
```

Use identical judgment traces and compare:

- persistent false-deny duration;
- violation duration after independently corroborated true violations;
- manual repair count;
- held-evidence resolution rate;
- explanation/provenance depth;
- replay determinism.

The expected result is not assumed. A policy that overuses `Hold` may delay necessary enforcement; a policy that overuses `Accept` may retain the false-positive blast radius of the direct-threshold baseline.
