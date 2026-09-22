---
name: Bug report
about: Determinism, admission, or output mismatch
title: "[bug] "
labels: bug
---

**What did you run?** (command + file)

**What did you expect?**

**What happened instead?** (paste actual output)

**Determinism check** — did two runs of the same input produce different output?

```bash
cargo run -- <mode> > run1.txt && cargo run -- <mode> > run2.txt && diff run1.txt run2.txt
```
