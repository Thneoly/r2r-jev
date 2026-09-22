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

## Results (2026-09-23)

Four deterministic traces, both arms, run with `src/bin/admission-compare.rs`
(12 tests lock these numbers; rerun with `cargo run --bin admission-compare --
<trace.csv>`):

| trace | metric | baseline | admission |
|---|---|---|---|
| uncertain-single-source | false-deny calls | **2** | **0** |
| | effective repairs | 1 | 0 |
| strong-trusted-false-positive | false-deny calls | 1 | 1 |
| corroborated-violation | violation calls allowed | 3 | **1** |
| hold-lifecycle | violation calls allowed | 4 | **2** |
| | held resolution | n/a | 1 accept + 1 expired + 1 pending |

Readings (EA-H1/H2 against these numbers):

1. **EA-H1 supported in the uncertain band.** A 870k-confidence single-source
   flag with no corroboration is Held instead of admitted: zero false-deny
   calls, zero repairs, versus two false denials plus a human repair under
   the direct threshold.
2. **EA-H1 does not extend to the strong-trusted band.** A 930k confidence
   flag from a high-reliability source is `Accept(Strong)`: both arms
   suspend and both inherit one false denial. Admission v0.1 does not fix
   this band — the mitigation there is source-reliability configuration,
   not admission structure.
3. **EA-H2 supported.** In the corroborated-violation and hold-lifecycle
   traces the baseline never suspends below its 850k threshold while
   violations accumulate (3 and 4 allowed); admission suspends at the
   second, corroborated signal (1 and 2 allowed). Corroboration made
   enforcement *earlier* than the fixed threshold, not later.
4. **Hold resolved both ways.** One held signal resolved by corroboration
   (Accept), one by expiry, one remained pending at trace end — the third
   state is not a leak, it has explicit exits.
5. **Provenance depth** per event: baseline 2 links, admission 4-5 links
   (event -> evidence -> admission -> decision [+ authorization]).

These are constructed traces with a fixed policy version; they demonstrate
mechanism behavior, not field performance. What they do establish on the
protocol's own terms: the temporal blast radius of uncertain single-source
judgments went to zero without losing (in fact improving) corroborated
enforcement in the sub-threshold band, and the honest limit — the
strong-trusted false positive — is preserved in plain sight.
