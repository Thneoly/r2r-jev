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

Expected shape:

```text
JudgmentObserved
  beyond_scope = 0.940000
  destructive  = 0.720000

R2R causal chain
  Evidence       created
  Trust          Active -> Warning
  Delegation     Active -> Degraded
  Authorization  Active -> Suspended

Decision: DENY
```

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
│   ├── main.rs        # minimal CLI
│   ├── jev.rs         # Jev adapter
│   ├── model.rs       # typed boundary objects
│   └── r2r.rs         # deterministic demo transition chain
├── demo/
│   └── fixtures/      # offline judgments
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

## License

No open-source license has been selected yet. Until a license is added, normal copyright rules apply. This is intentional so the repository owner can make that legal choice explicitly.
