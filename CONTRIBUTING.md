# Contributing

This repository is intentionally small and deterministic. Contributions that
keep it that way are welcome in four tracks.

## Ground rules

- **Determinism is the product.** No clocks, no randomness, no floating point
  in governance paths — probabilities are integer parts-per-million from the
  adapter boundary onward, time is virtual ticks.
- **Every behavior change ships with a test** that fails before and passes
  after. `cargo test --all-targets` must stay green.
- Run `cargo fmt` before committing.
- One scenario or one change per pull request.

## Track 1 — Scenario traces (easiest entry)

The experiments are driven by CSV judgment traces:

```text
experiments/stateless-vs-stateful/*.csv
experiments/evidence-admission-v0.1/traces/*.csv
```

Columns are documented in the experiment READMEs. A new trace should:

1. express a situation the existing traces do not cover;
2. come with a test in the corresponding binary's test module asserting the
   metrics you expect (see `src/bin/admission-compare.rs` for examples);
3. state its point in the note column.

**Adversarial traces are especially welcome.** A trace where admission loses
is as valuable as one where it wins — `strong-trusted-false-positive.csv`
exists precisely to keep an honest limit in plain sight. If your trace makes
the current policy look bad, that is a good trace.

## Track 2 — Admission policy variants (within the v0.1 envelope)

The constants and ordering in `src/admission.rs` are experiment variables.
Variant exploration is welcome under three constraints:

1. stay within the v0.1 envelope: the five evidence dimensions
   (kind, confidence, source reliability, corroboration, virtual-time
   expiry), deterministic evaluation, integer ppm;
2. bump `POLICY_VERSION` for any semantic change, so old records keep their
   replay semantics;
3. report variant results against the same traces — a variant is a claim
   about traces, not a taste.

Proposals **beyond** the envelope (new evidence dimensions, learned or
self-asserted reliability, correlation between sources, decay functions) are
open research questions, not PR material — open a Discussion first.

## Track 3 — Adapters and fixtures

The live Jev adapter is deliberately thin and isolated from deterministic
governance semantics. Keep it that way. New fixtures must be plain ASCII
JSON so the deterministic path stays byte-stable across platforms.

## Track 4 — Analysis and write-ups

Deterministic, offline scripts that turn binary output into tables or
figures; and experiment interpretations that quote actual output rather than
intent. `scripts/make_demo_gif.py` is the house style: reproducible from the
artifact it documents.

## Issues

Use the issue templates. For a new scenario proposal, include the CSV inline
and say which arm you expect it to hurt.
