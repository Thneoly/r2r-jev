# Experiment: Stateless Gate vs Stateful Governance

This experiment holds the probabilistic judgments constant and changes only what
the system does **after** receiving them.

It is intentionally **not** a Jev accuracy benchmark.

## Research question

> What changes when a typed probabilistic judgment is used only for the current
> call versus admitted into persistent governance state?

Two arms consume the same integer PPM judgments:

### Arm A — Stateless gate

```text
current judgment
    ↓
threshold
    ↓
ALLOW / DENY current call
```

No authorization state survives the call.

### Arm B — Stateful governance

```text
judgment / governance event
    ↓
Evidence
    ↓
persistent Authorization state
    ↓
future ALLOW / DENY
```

For this minimal experiment, the state machine is deliberately tiny:

```text
Active
  ├─ high-risk judgment ────────> Suspended
  ├─ review_failure ────────────> Suspended
Suspended
  └─ human_override ────────────> Active
```

The real R2R project uses typed relations and R→R transitions; this executable is
only a small public experiment.

## Why there are two scenarios

### `persistent-policy.csv`

The policy says that after a serious governance event, authorization remains
suspended until an explicit repair/override.

A stateless gate cannot represent that temporal policy without adding state.

Expected qualitative result:

```text
Stateless: misses later calls whose current score is low
Stateful:  preserves suspension until repair
```

### `false-positive.csv`

Persistence has a cost.

If a high-risk judgment is wrong and the system immediately admits it into
persistent authorization state, later benign calls may also be denied.

Expected qualitative result:

```text
Stateless: false positive affects one call
Stateful:  false positive can have a larger temporal blast radius
```

This scenario is important because the desired R2R rule is **not**:

```text
Jev score -> permission
```

It is:

```text
Jev score -> evidence -> admission semantics -> relation transition
```

A production design therefore needs mechanisms such as corroboration,
confidence thresholds, evidence classes, review, decay/expiry, repair, and human
override.

## Run

From repository root:

```bash
cargo run --bin compare -- experiments/stateless-vs-stateful/persistent-policy.csv
cargo run --bin compare -- experiments/stateless-vs-stateful/false-positive.csv
```

The program prints each tool-call decision and summary counts.

## What this experiment can establish

It can demonstrate that:

1. persistent governance semantics create history-sensitive behavior;
2. a stateless current-call threshold does not have that behavior unless state is
   added elsewhere;
3. persistence can reduce missed policy enforcement under a persistent policy;
4. persistence can also amplify a bad judgment.

It **cannot** establish that R2R is generally safer or better than a stateless
gate. That requires realistic traces, independently labeled outcomes, multiple
policies, cost measures, and statistical evaluation.

## Next experiment

The next useful step is a larger event stream with independently labeled
governance outcomes and these metrics:

- unsafe-action duration;
- repeated unsafe attempts;
- unnecessary blocks;
- repair latency;
- governance-state churn;
- temporal blast radius of a false positive;
- explanation/provenance depth;
- replay determinism.
