# R2R + Jev

> **Jev judges. R2R remembers, governs, and reconciles.**

`r2r-jev` is a small public integration that demonstrates one idea:

> **A probabilistic judgment should be evidence, not authority.**

Jev turns unstructured state into typed probabilistic decisions. R2R turns accepted evidence and events into persistent, replayable relation state.

```text
Reason                  Judge                     Govern                         Act

LLM / Agent  ───────▶   Jev   ───────────────▶   R2R   ─────────────────────▶   MCP / Tools
                        typed judgment            persistent relation state
                              │                         │
                              │ JudgmentObserved      │ Evidence
                              │                         ▼
                              └──────────────────▶  Trust
                                                    │
                                                    ▼
                                                 Delegation
                                                    │
                                                    ▼
                                               Authorization
```

The important boundary is:

```text
Jev result != permission
```

A Jev score becomes a `JudgmentObserved` event. R2R then applies deterministic transition semantics to decide whether that evidence is strong enough to change Trust, Delegation, Authorization, Supervision, or other relations.

## Why this exists

Most agent guardrails answer a local question:

```text
Is this tool call risky right now?
```

R2R asks a different question:

```text
Given what has happened over time, what relationship now exists between
this agent, this task, this resource, and the governing organization?
```

A single judgment can therefore affect future actions without making the probabilistic model itself the authority source.

## Quick start — no API key

The repository ships with a recorded Jev-style fixture so the governance path is runnable without network access:

```bash
git clone https://github.com/Thneoly/r2r-jev.git
cd r2r-jev
cargo run -- fixture
```

Expected shape (three acts):

```text
Act 1: a judgment becomes evidence, not permission
  JudgmentObserved  event=ev-0001 provider=fixture:jev-style
  judgment          beyond_scope=0.940000 destructive=0.720000 tool=merge_pull_request
  Evidence          evidence-0001 created
  Trust             Active -> Warning             trust-0001 (caused by evidence-0001)
  Delegation        Active -> Degraded            delegation-0001 (caused by evidence-0001)
  Authorization     Active -> Suspended           authorization-0001 (caused by evidence-0001)
  Decision          DENY merge_pull_request (threshold-crossing judgment admitted as evidence)

Act 2: the next call inherits history
  JudgmentObserved  event=ev-0002 provider=fixture:jev-style
  judgment          beyond_scope=0.180000 destructive=0.120000 tool=read_file
  Evidence          evidence-0002 created
  Relations         unchanged (a stateless gate would ALLOW this call)
  Decision          DENY read_file (authorization remains suspended by earlier evidence)

Act 3: human override repairs the relation, under supervision
  GovernanceEvent   event=ev-0003 kind=human_override supervisor=human-1
  Authorization     Suspended -> Active           authorization-0002 (caused by ev-0003)
  Supervision       created                       supervision-0001 (caused by ev-0003)
  Decision          ALLOW merge_pull_request (human override restores authorization under supervision)

Provenance
  ev-0001 -> evidence-0001 -> trust-0001 -> delegation-0001 -> authorization-0001 -> DENY(merge_pull_request)
  ev-0002 -> evidence-0002 [via authorization-0001] -> DENY(read_file)
  ev-0003 -> authorization-0002 -> supervision-0001 -> ALLOW(merge_pull_request)
```

Ids are sequential and deterministic: no clocks, no randomness. The same
event sequence always produces the same output, byte for byte.

## Live Jev

Set a TypeSafe API key:

```bash
export TYPESAFE_API_KEY=...
```

Then run:

```bash
cargo run -- live \
  --task "Fix the login redirect bug" \
  --tool "merge_pull_request" \
  --scope "repo-alpha" \
  --intent "Merge a pull request containing unrelated repository changes"
```

The live adapter sends typed `noul` questions to Jev and converts returned floating-point probabilities into integer parts-per-million **at the adapter boundary**. The deterministic R2R demo kernel itself does not use floating point.

Live mode covers a single judgment (Act 1). The full three-act story, including
history inheritance and human override, is the fixture path above.

## Core design rule

```text
Jev proposes evidence.
R2R admits state transitions.
Enforcement executes the resulting decision.
```

Or, more compactly:

> **Judge -> Govern -> Act.**

## Architecture

```text
                 Probabilistic judgment plane

 Agent state ───────────────▶ Jev
                               │
                               │ typed answer + probability
                               ▼
                        JudgmentObserved

──────────────── deterministic boundary ────────────────

                               │
                               ▼
                       Evidence Relation
                               │
                          typed R -> R
                               │
                 ┌─────────────┼─────────────┐
                 ▼             ▼             ▼
               Trust       Delegation   Authorization
                                                │
                                                ▼
                                         Enforcement
```

See [`docs/architecture.md`](docs/architecture.md).

## What this repository is — and is not

This repository is intentionally small. It demonstrates the integration boundary between a fast probabilistic judge and a deterministic relation-governance runtime.

It is **not**:

- a claim that Jev itself should mutate authorization state;
- a replacement for the full R2R runtime and calculus;
- another generic MCP gateway;
- a benchmark claiming R2R makes Jev more accurate.

The research question is instead:

> **Does persistent governance state add value beyond stateless per-call gating?**

A planned comparison will hold Jev judgments constant and compare:

```text
A. Judgment -> threshold -> allow / deny
B. Judgment -> evidence -> relation state -> future governance
```

## Repository layout

```text
.
├── src/
│   ├── main.rs        # three-act demo CLI (fixture + live)
│   ├── jev.rs         # Jev adapter
│   ├── model.rs       # typed boundary objects
│   ├── r2r.rs         # deterministic demo governance kernel
│   └── bin/
│       └── compare.rs # stateless vs stateful experiment
├── demo/
│   └── fixtures/      # offline judgments
├── experiments/
│   └── stateless-vs-stateful/
├── docs/
│   ├── architecture.md
│   └── why-r2r-after-jev.md
└── .github/workflows/
    └── ci.yml
```

## Relationship to R2R

The full R2R project explores relations as first-class runtime resources with typed Relation-to-Relation transitions, conflict semantics, propagation boundaries, reconciliation, deterministic replay, and agent enforcement.

This repository focuses on one public integration surface:

```text
probabilistic judgment -> evidence -> persistent relation state
```

## Status

Early public demo. The fixture path is the reproducible reference path; the live Jev adapter is deliberately thin and isolated from deterministic governance semantics.

## Experiment: stateless gate vs stateful governance

The repository includes a deliberately small comparison that feeds the same
judgment stream to two arms:

```text
A. judgment -> threshold -> current-call decision
B. judgment -> evidence -> persistent governance state -> future decisions
```

Run:

```bash
cargo run --bin compare -- experiments/stateless-vs-stateful/persistent-policy.csv
cargo run --bin compare -- experiments/stateless-vs-stateful/false-positive.csv
```

The second scenario is intentionally adversarial to persistent state: it shows
that a false positive can have a larger temporal blast radius when evidence is
persisted. This is why the intended design is **judgment -> evidence -> admission
semantics -> relation transition**, not `score -> permission`.

See [`experiments/stateless-vs-stateful/README.md`](experiments/stateless-vs-stateful/README.md)
for what this experiment can and cannot establish.

## Non-affiliation

This is an independent project. It is not affiliated with, endorsed by, or
sponsored by TypeSafe AI. "Jev", "TypeSafe", and "System One" are used only
to describe the public API this integration targets, and remain the property
of their respective owners.

## License

No open-source license has been selected yet. Until a license is added, normal copyright rules apply. This is intentional so the repository owner can make that legal choice explicitly.
