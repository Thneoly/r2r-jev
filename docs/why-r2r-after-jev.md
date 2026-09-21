# Why R2R after Jev?

Jev is useful when software needs a typed, probabilistic judgment over messy state:

```text
state -> judgment + probability
```

That solves an important problem, but not the whole governance problem.

## Stateless gating

A common pattern is:

```text
Tool call
   ↓
Jev
   ↓
score >= threshold ?
   ↓
Allow / Block
```

This is effective for immediate screening.

But after the call is handled, what remembers what happened?

## Persistent governance

R2R treats the judgment as evidence that may change long-lived relations:

```text
Jev judgment
   ↓
Evidence
   ↓
Trust
   ↓
Delegation
   ↓
Authorization
   ↓
future decisions
```

The point is not that every judgment must mutate state. The point is that persistent changes are admitted through explicit relation-transition semantics rather than being hidden inside application glue.

## Judgment is not authority

This repository follows one rule:

> **A probabilistic judgment is evidence, not permission.**

That prevents the model from becoming an implicit superuser.

Jev may estimate:

```text
P(action is beyond scope) = 0.94
```

R2R still decides whether that evidence is sufficient to change governance state.

## Why this matters over time

Consider repeated agent activity:

```text
normal
normal
minor anomaly
review failure
out-of-scope action
normal
repeat violation
human override
```

A stateless gate sees eight isolated calls.

A stateful governance runtime can remember that these events changed relationships between:

- agent and task;
- agent and resource;
- reviewer and delegation;
- supervisor and authorization;
- organization and risk.

This lets later decisions depend on history without forcing the probabilistic judge itself to maintain authority state.

## Research question

The interesting comparison is therefore not:

```text
Is R2R more accurate than Jev?
```

It is:

> **What value does persistent governance state add when both systems receive the exact same judgments?**

A useful controlled experiment should keep the Jev outputs fixed and compare:

```text
A. Judgment -> threshold -> immediate allow/deny
B. Judgment -> evidence -> persistent relation state -> future governance
```

Possible metrics:

- repeat unsafe attempts;
- violation duration;
- unnecessary blocking;
- recovery latency;
- history sensitivity;
- causal explanation depth;
- replayability;
- governance churn.

## Positioning

Jev and R2R solve different layers:

```text
LLM  -> Reason
Jev  -> Judge
R2R  -> Govern
MCP  -> Act
```

The goal of this project is to make that boundary concrete and runnable.
