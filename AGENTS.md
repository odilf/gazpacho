# Gazpacho Contributor Guide

Gazpacho is video editing software whose project language compiles from a
`.gzp` AST into a render graph and, ultimately, a render schedule.

## Architecture

The intended pipeline is:

```
.gzp source -> gazpacho-ast -> gazpacho-compile render graph/plan -> gazpacho-render
```

Keep crate responsibilities narrow:

- `gazpacho-datatypes`: shared primitives and media types.
- `gazpacho-ast`: lexer, parser, AST, and printing for `.gzp`.
- `gazpacho-operations`: built-in operation definitions, inputs, dependencies,
  and operation properties.
- `gazpacho-compile`: AST evaluation/name resolution into a `RenderGraph`, then
  analysis and scheduling.
- `gazpacho-media`: media metadata plus sequential and random access reading
  and writing.
- `gazpacho-render`: execution of compiled graphs using media access.
- `gazpacho-fixtures`: generated and real-world media fixtures for tests.
- `gazpacho-ui`: application UI.

Prefer dependencies in that direction. In particular, shared semantics belong
in `gazpacho-datatypes` or `gazpacho-operations`, not in the renderer.

## Compiler Model

Do not model rendering solely as `render(project, t) -> Frame`. A node must
receive a downstream demand that can include time samples, output resolution,
region of interest, and format/precision:

```
render(node, demand) -> Frame
demand = { times, resolution, roi, format }
```

Each operation transforms demand while traversing downward and transforms frames
while traversing upward. The compiler should abstractly interpret the graph to
produce a symbolic demand summary per node, then derive a schedule from those
summaries.

This model enables three optimization families:

- Hoist time-invariant work and other loop-invariant computation.
- Share duplicate or overlapping frame demands through caching/deduplication.
- Schedule media access according to demand shape, such as sequential decode,
  sliding windows, reverse GOP buffering, and time-varying liveness.

Preserve semantic information needed for valid rewrites. For example,
format/precision affects whether algebraic optimizations are sound, and time,
resolution, and ROI must all propagate through the graph.

## Development

- Prefer asking the user questions to clarify intent rather than inferring
  it, even for small things. Ask first whenever there is any reasonable doubt
  about what is wanted; do not silently guess.
- Do not make consequential design decisions unilaterally. When a decision is
  genuinely hard or the intended semantics are unclear, do not guess; instead
  ask the user. If the decision cannot be resolved immediately, leave a
  `todo!()` (or a narrowly scoped stub) in code and add a `TODO:` comment
  describing the decision that remains, so a human can resolve it.
- Run `cargo check --verbose` for the CI check.
- Run `cargo nextest run --verbose` for the test suite.
- The workspace enables strict Clippy warnings, especially around panics,
  unchecked indexing, ignored errors, and unsafe code. Fix the underlying
  issue or use a narrowly scoped `#[expect(..., reason = "...")]` when an
  invariant is intentional.

Add tests near the affected crate. Use media fixtures and property/oracle tests
when a change affects decoding, timing, or media metadata behavior.
