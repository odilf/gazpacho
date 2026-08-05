# Tricky cases

Interesting/tricky cases that are useful to keep in mind when doing design decisions.

Also meant to show cases where a "naive" approach would be much worse than a "smart" approach. The point is that gazpacho should always use the better approach.

Notation: `t` is output time, `f` is one output frame period, `c(t)` is "the
clip `c` sampled at time `t`".

## 1. Interleaving two videos

```
interleave(a, b) = \t -> if even(floor(t * fps)) { a(t) } else { b(t) }
```

**Generic.** At each `t` it pulls one side. If pulls are treated as
independent, source `a` gets asked for `t = 0, 2f, 4f, ...`, and a decoder
asked for non-consecutive times *seeks*: back to a keyframe, then decode
forward. Say, 100x slower than sequential.

**By hand.** Run both decoders sequentially at full rate and discard the frames
you do not need. Decoding a frame and throwing it away is far cheaper than a
seek.

The point here is that the *set of times each source is sampled at* increases
monotonically. Does this only matter for sequential access?

## 2. Motion blur over an animated transform

```
load("a.mp4") |> transform(pan(t)) |> motion_blur(shutter: 1/48, taps: 8)
```

**Must know.**
- Which tap times collapse onto the same source frame. Computable from the
  affine time map plus the source's frame timing.
- That `transform` is a pointwise resample, so the tap loop can be fused into a
  single kernel.
- That the per-tap parameters are CPU-computable scalars.

**Note.** This answers the open question in `gazpacho-compile/src/plan.rs:9-12`
about how many frames of uniforms to pass the GPU: **the uniform-array size is
the tap count**, and it is statically known.

**Degenerate case.** If nothing below the blur varies within the frame period,
the mean of 8 identical frames *is* the frame, so **the whole blur eliminates**.
This is common in practice: project-wide motion blur applied over locked-off
footage.


## 3. Temporal denoise: the sliding window

```
denoise(c) = \t -> mean([c(t - 2f), c(t - f), c(t), c(t + f), c(t + 2f)])
```

**Generic.** Five decodes per output frame.

**By hand.** Consecutive output frames share 4 of their 5 taps. A 5-frame ring
buffer makes it **1 decode per output frame** -- a 5x win -- and it converts
random access into sequential access, which is a second, larger win.

**Must know.** That the tap set is a *sliding window*: successive tap sets
overlap heavily and their union is monotone. Then buffer depth = window size,
known statically.

**Related.** The same machinery covers frame-rate mismatch. Rendering a 23.976
source at 30fps means some source frames are consumed by two output frames. If
everything below is time-invariant given the same source frame, you reuse the
whole processed result, not just the decode.


## 4. Motion blur over a linear op

```
load("a.mp4") |> contrast(1.5) |> mblur(taps: 8)
```

**Generic.** 8 decodes, **8 contrasts**, 1 average.

**By hand.** `mean(a*x_i + b) = a*mean(x_i) + b`. Contrast is affine, so it
commutes with the average. Hoist it out of the tap loop: 8 taps, **1 contrast**.
Loop-invariant code motion, for pixels.

**Must know.** That `contrast` is linear.

**Caveat, and it is a real one.** Our `contrast` clamps to `0..=255`
(`gazpacho-render/src/lib.rs:41`), and clamping is *not* linear. So this
rewrite is **invalid in the current 8-bit pipeline** and valid in an unclamped
float one.

That is not a footnote. It means op algebra is conditional on the working
format, so precision/format is something the compiler has to track -- not
something the renderer picks locally.


## 5. Logo, title card, colour matte

```
overlay(main, load("logo.png") |> opacity(0.8) |> transform(corner))
```

**Generic.** Re-evaluates the logo subtree every frame, forever.

**By hand.** The subtree contains no `t`. Compute it **once**, hold the texture,
reuse it for the whole duration.

**Must know.** Which subtrees are time-invariant.

**Note.** Trivial analysis, enormous payoff, and it covers most of what is
actually in a real project file: logos, lower-thirds, backgrounds, mattes,
static grades. Any design that cannot hoist these is losing on the single most
common case.


## 6. Small picture-in-picture, and crop

```
overlay(main_1920x1080, load("cam.mp4") |> scale_to(320x180) |> transform(corner))
```

**Generic.** Decodes `cam.mp4` at its native 1920x1080 and then downscales.
That is ~36x more decode and bandwidth than needed (2073600 px vs 57600 px).

**By hand.** Ask the decoder for 320x180 directly. Hardware scalers do this
nearly free, and `ResolutionRequest` (`gazpacho-media/src/read/mod.rs:197`)
already has the hook.

**Must know.** The *required resolution*, which propagates **downward** through
the graph, inverted by each op along the way.

**Same shape, different axis.** `crop(transform(c, m), rect)` should inverse-map
`rect` through `m` and decode/sample only that region.

**Note.** Resolution and region-of-interest flow down the graph with exactly the
same structure as time does. Any design with a place for time to flow down but
no place for these is incomplete.


## 7. Reverse, and a speed ramp through zero

```
c |> speed(lerp(2.0, -1.0))
```

**Generic.** Source time is non-monotone. Either seek per frame
(catastrophic), or buffer without bound.

**By hand.**
- Source time is the integral of the speed curve. Precompute it as a piecewise
  table at compile time, so per frame it is a lookup, not an integration.
- Check the **sign**. Where it is positive: sequential decode. Where negative:
  decode a GOP forward into a ring buffer and emit backward.
- Ring buffer depth = max GOP length, readable from `VideoMetadata::keyframes`
  (`gazpacho-media/src/metadata.rs:108`).

**Must know.** Monotonicity and sign of the time map, per interval, plus GOP
structure. All statically available.

**Note.** A generic renderer has to assume the worst everywhere, so it takes the
slow path even on the 95% of the timeline that is forward-monotone.


## 8. An opaque overlay that covers the frame

```
overlay(expensive_pipeline, fullscreen_title |> opacity(alpha(t)))
```

**Generic.** Decodes and processes `expensive_pipeline` throughout.

**By hand.** While `alpha == 1` and the overlay covers the output rect, the
pipeline underneath is **dead** -- do not decode it at all. But it is dead only
in *part* of the timeline. Occlusion culling, with time-varying liveness.

**Must know.** Geometry and opacity as functions of `t`, and the times at which
alpha saturates.

**Note.** Do not just skip the work -- skip *warming the decoder* too, and know
when to warm it back up before the reveal. Liveness has a lead time.


## 9. Smaller cases

Worth having on the list, without full treatment:

- **Nested transitions.** A fade between (a fade between a and b) and c has
  three concurrent decoders. Any "size the buffer for two" assumption breaks.
  The peak must be computed statically, not guessed.

- **Audio-driven auto-cut** (see the README TODO list). Structure derived from
  media *content*. Forces analysis to happen at compile time, but the output is
  ordinary constant breakpoints, so nothing downstream needs to change.

- **Crossfade between two ranges of the same file.** Two read positions in one
  file. Probe metadata once; know that peak concurrent decoders on that file is
  2, and preallocate.

- **Same frame used twice with a shift.** `blend(x, shift(x, 1f))` is not caught
  by CSE, because the two paths have different time maps. It is caught by a
  frame cache with a one-frame window -- so the plan should tell the renderer
  the window size it needs.


## What the cases have in common

Two observations, and they are what the design should be built around.

### The optimisations are three families, not nine special cases

- **Hoist out of the frame loop.** Time-invariant subtrees (5), linear ops out
  of tap loops (4), precomputed integrals (7), collapsed blurs (2).
- **Share.** Taps landing on one source frame (2), sliding windows (3), frame
  reuse across fps mismatch (3), ordinary CSE.
- **Schedule.** Monotone -> sequential; negative -> GOP ring buffer (7);
  periodic -> sequential-with-drop (1); dead -> do not even warm (8); and
  static peak sizing for every pool.

None of them are about *which op is which*. They are all about the *shape of
what gets asked for*.

### `render(project, t) -> Frame` is the wrong signature

In cases 2 and 3 the thing being asked for is a *set* of times, not one time.
In case 6 it carries a resolution and a rectangle. In case 4 it carries a
precision. The honest signature is:

```
render(node, demand) -> Frame

demand = { times, resolution, roi, format }
```

Under it, every op does exactly one kind of thing: transform the demand on the
way down, transform the frame on the way up.

| op                 | what it does to the demand      |
|--------------------|---------------------------------|
| `concat`           | routes it to one child          |
| `speed` / `trim`   | scales/shifts its times         |
| `mblur`            | *replicates* it at N times      |
| `crop`             | shrinks its roi                 |
| `scale`            | shrinks its resolution          |
| `contrast`         | passes it through untouched     |

This is where the asymmetry that made earlier designs feel wrong goes away.
`concat` is not special: it is the routing case, sitting beside the scaling case
and the replicating case. And motion blur -- which broke every point-sampled
model we tried -- is simply the op whose demand transform is not one-to-one.

The compiler's job then states in one line:

> Abstractly interpret the graph over demands, computing for each node a
> symbolic description of the demand it will receive as a function of output
> time.

Every optimisation above is reading a property off that description: is it
constant (hoist), monotone (sequential), periodic (interleave), overlapping
frame to frame (ring buffer), empty (cull), collapsing onto one source frame
(dedupe taps).

It also dissolves the "flatten the graph or walk it" question, which was
premature. What you precompute is the demand summary per node; the plan is the
graph plus those summaries plus a schedule derived from them.


## Open questions

- **Audio.** Only glanced at so far. Audio demands *intervals* natively rather
  than points, which either fits the demand model cleanly or shows that `times`
  needs to be richer than a set of points. Worth settling before committing,
  because the answer changes the core type.

- **How far does periodicity need to go?** Case 1 needs it. Does anything else?
  If it is only interleaving, it may be cheaper to special-case than to build a
  general periodic-demand representation.

- **Where does format/precision get decided?** Case 4 shows it gates a rewrite,
  so it cannot be purely a renderer concern. Unclear yet whether it is inferred,
  declared per project, or negotiated per subtree.

- **Cost model.** Several of these are only optimisations *given* that decode is
  expensive relative to GPU work. That should be measured, not assumed.

