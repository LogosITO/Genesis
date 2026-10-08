# Research references and evidence ledger

## Established knowledge
- Inigo Quilez, [Distance Functions](https://iquilezles.org/articles/distfunctions/): common analytic primitive and CSG formulas. The code in this repository is independently written and licensed here.
- Hart, *Sphere Tracing: A Geometric Method for the Antialiased Ray Tracing of Implicit Surfaces* (1996), [DOI:10.1007/s003710050084](https://doi.org/10.1007/s003710050084): distance bounds for sphere tracing.
- Rust project, [The Cargo Book](https://doc.rust-lang.org/cargo/) and [Rust Reference](https://doc.rust-lang.org/reference/): workspace and language behavior.

## Hypotheses
Field-first state may support shared queries across rendering, simulation, and interaction with acceptable cost. This has not been measured.

## Decisions
The four ADRs record current engineering decisions, not research findings.

## Open questions
Robust conservative bounds for composed fields, scaling of graph evaluation, deterministic cross-platform rule evaluation, and data format for persistent world state.

## Experimental findings
None published yet. Local correctness checks are reported in the repository and are not performance experiments.
