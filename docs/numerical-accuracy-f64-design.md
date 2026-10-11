# Numerical accuracy without further angle representation changes

Status: core implemented on this branch. Baseline: `main` at
`06d62413a73882806ab560b70237f62d1d7e76b0`. Date: 2026-10-10.

## Decision

Numerical optimization must preserve the exact real value represented by each
accepted binary64 angle. If a rewrite cannot represent its result exactly, the
pass must keep the original operations. Approximate rotation synthesis remains
an explicit operation, with conversion and synthesis errors included in its
accuracy contract.

Keep the existing circuit representation, public angle constructors, qubit and
classical-bit counts, and gate semantics. Use checked arithmetic in optimization
passes and temporary higher precision at approximation boundaries. No arbitrary-precision
angle objects, expression DAGs, or accumulated error fields are added to gates.

The original proposal concerned a checkout where rotation parameters were raw
`f64`. Latest `main` has already introduced `Angle`, with finite numeric values,
exact pi coefficients, and preserved expressions. It also implements much of
the conservative folding policy. This design retains that implementation rather
than reverting it. The numeric component remains binary64. Section
[Applying the policy to a raw f64 model](#applying-the-policy-to-a-raw-f64-model)
describes how the same policy works without the existing `Angle` representation.

## Baseline and remaining work

The [angle migration notes](angle-soundness-migration.md) describe the current
accepted-angle policy. The following table distinguishes existing safeguards
from changes this design requires.

| Area | Existing behavior on the baseline | Required work |
| --- | --- | --- |
| Numeric addition | `exact_float_add` uses TwoSum and rejects a nonzero correction | Maintain the contract; verify arithmetic assumptions on supported targets |
| Named phases | Exact quarter-turn arithmetic, separate from numeric residuals | Keep the integer fast path and provenance checks |
| Zero and Clifford recognition | Numeric zero is exact; floating pi is not inferred to be mathematical pi | Extend regression coverage across all native rotations and pipeline boundaries |
| `PhaseFoldRand` | Checked merges; failed merges start a new group | Preserve transaction ordering and report randomized matching separately |
| `PhaseFoldPauli` | Checked signed merges and exact axis checks | Cover X, Y, Z emission, rejected merges, and frame updates |
| `CnotMin` | Checked phase terms; failed arithmetic restores the input chunk | Test failure after partial CZ expansion and after earlier successful merges |
| Parser | Pi-free expressions use binary64 evaluation; bounded affine pi recognition is available | State the accepted-input contract; any strict source evaluator is a separate opt-in feature |
| Controlled lowering | Literal checked half-angles; no projective normalization before expansion | Preserve this rule; improve failure diagnostics for subnormal half-angles |
| Synthesis | Lossy angle conversion, binary64 argument reduction, decimal target conversion; errors are marked uncertified | Build and verify a target from the accepted angle, account for every error, and fail when certification is unavailable |
| Export | Symbolic QASM and numeric round trips; explicit lossy export | Keep exact and approximate export distinct; do not label lossy output certified |

The failures discussed during investigation came from the older checkout. They
are regression inputs, not assertions that all those failures still occur on
this baseline. In particular, `main` already rejects rounded numeric folding.

## Semantic contract

### Accepted numeric angles

A finite `f64` denotes its exact binary value as a real number. A normal value is
an integer significand multiplied by a power of two; subnormals have the same
dyadic interpretation. A decimal input such as `0.1` is rounded when accepted.
After acceptance, an optimizer must preserve that binary value, rather than
silently rounding it again during a fold.

Named `t`, `s`, and `z` gates denote mathematical pi fractions. A numeric angle
whose bits equal Rust's `PI / 4.0` does not acquire that provenance. Existing
explicit pi fractions retain their mathematical meaning. Preserved expressions
retain their spelling and remain outside numerical folding and synthesis until
an allowed evaluator can interpret them.

Consequently, `rz(0.1); rz(0.2)` cannot generally become one binary64 rotation
without changing its accepted angle. In contrast, `rz(0.125); rz(0.25)` can
become `rz(0.375)` exactly. Exactness concerns representability, not magnitude.

### Circuit equivalence

For a unitary circuit, exact optimization preserves its operator up to one
whole-circuit global phase. For circuits with measurement or reset, it preserves
the quantum channel and classical outcomes under the pass's established input
conditions. Arithmetic checks do not authorize moving a rotation through an
incompatible gate, measurement, or reset.

An unconditional `Rz(theta)` has a projective period of mathematical `2*pi`.
A controlled `Rz(theta)` has literal period `4*pi`: a phase on its active
control subspace is observable. `P(theta)` and `Rz(theta)` differ by global
phase as unconditional operations, but `CP(theta)` and `CRz(theta)` differ by
a phase on the control. Every helper that normalizes an angle must therefore
have an explicit gate and phase context.

### Numerical guarantees and randomized matching

Exact arithmetic does not make `PhaseFoldRand` deterministic. Its Boolean
function fingerprints can collide. Reports must distinguish arithmetic
exactness from that matching risk. `PhaseFoldPauli` uses fingerprints to locate
candidates and confirms axes and commutation exactly. A pipeline requiring
deterministic rewrite validity must omit unchecked randomized matching.

## Shared arithmetic rules

### Checked exact addition

Retain [the existing helper](../src/angle.rs), with the following contract:

```text
exact_add(a, b) -> result or refusal

require finite(a) and finite(b)
s  = a + b
bb = s - a
aa = s - bb
br = b - bb
ar = a - aa
e  = ar + br
require every intermediate to be finite
require e == 0
return s, canonicalizing signed zero
```

TwoSum represents the real sum as `s + e` under its arithmetic assumptions.
Accepting only `e == 0` therefore proves that `s` is the exact sum. This is an
error-free transformation, not an approximate tolerance test. See
[Shewchuk's treatment of exact floating-point transformations](https://www.cs.cmu.edu/~quake/robust.html).

The helper requires binary64 operations rounded to nearest, gradual underflow,
and the specified evaluation order. Fast-math, reassociation, flushing subnormals
to zero, or an externally changed rounding mode invalidates the proof. Supported
builds must document these assumptions. Foreign-language entry points must not
promise exact folding in an unsupported floating-point environment: detect and
refuse that environment where practical, or use an integer-bit implementation
of the representability check. A few startup examples are diagnostics, not a
proof that every operation obeys the required environment.

Signed addition accepts only signs `+1` and `-1`. Negating a finite binary64
value is exact. Overflow in the sum or any required intermediate is a refusal,
even when another algorithm might recover a representable answer.

Kahan summation alone does not satisfy this contract. Keeping a correction
temporarily and then discarding it when emitting one `f64` still changes the
angle. No residual may disappear between accumulation and emission.

### Zero, pi fractions, and normalization

Numeric rotations are removable as angle identities only when their numeric
angle is exactly zero. A state-based proof, such as a diagonal rotation on a
wire known to be reset to zero, remains a distinct valid optimization.

Numeric rotations must never be classified as Clifford gates by a distance to
rounded pi, even if that distance is zero in binary64 arithmetic. Named gates
and explicit certified pi coefficients can use integer quarter-turn arithmetic.

Do not reduce numeric residuals using `rem_euclid(2.0 * PI)`. The divisor is a
binary approximation, and the remainder operation introduces another rounding.
Keep numeric residuals unchanged. Existing exact pi coefficients may normalize
with integer arithmetic in a context allowing projective equivalence.

### Refusal and commit ordering

A refused rewrite is an optimization opportunity that was not taken. It is not
invalid input and normally does not stop optimization. Before accepting a fold,
the pass must establish all of the following:

1. The gate movement and signed axis relation are valid.
2. The arithmetic result is representable under the accepted-angle policy.
3. Emission is defined and its real gate count satisfies the pass's cost rule.
4. All pending changes can be committed together.

Only then may it mark gates deleted, replace a live group, change a Clifford
frame, or retire history entries. A failure must not leave a partially applied
rewrite. Approximate synthesis has a different failure policy: a requested
decomposition must return an error when it cannot satisfy its stated contract.

## Changes by pass

### PhaseFoldRand

Keep `LivePhase` accumulation through `Angle::checked_add_signed`. The named
quarter-turn path must continue to avoid floating pi conversion. Numeric
residuals use checked exact addition. Mixed values keep their exact pi and
numeric components distinct through emission.

When a merge fails, leave the earlier live group for finalization, add the
current rotation as a new group, and update the lookup to the new group. The
earlier gate must not have its deletion flag changed. If subsequent rotations
cancel within the new group, the earlier group's value still survives.

At finalization, compensate for a complemented Boolean function with exact
sign handling. Emit named contributions and numeric residuals separately when
necessary. Never reconstruct a mixed group as `k * (PI / 4) + residual`.

Preserve the existing Boolean transfer rules and degree limits. Unsupported
native gates must retain their established conservative fragment boundaries;
this numerical work does not authorize new axis propagation. Measurement/reset
tests must check channels rather than only output gate counts.

Acceptance tests must include a failed merge after an earlier successful merge,
a complemented final site, and a failed group followed by an independent group
that still folds. The randomized-matching report flag remains set even if no
arithmetic refusal occurs.

### PhaseFoldPauli

For same-axis candidates, compute `sign * prior + current` through the checked
angle API after proving the axes and intervening commutation relations. Keep
the existing order in which a successful merge and its emission cost precede
history deletion and frame updates.

The policy applies uniformly to `rx`, `ry`, `rz`, and unconditional `p`, with
their existing physical-axis conventions. A failed X or Y merge must leave
both rotations live and must not convert their rounded sum into a Clifford
frame operation. Numeric zero may be eliminated; a numeric value near a
quarter-turn remains a numeric rotation.

Retain `Angle` for mixed X/Y replacements on this baseline. It can hold the
separate pi coefficient and residual. When lowering that replacement to basic
operations or QASM, use exact basis changes and commuting components, or retain
the replacement unchanged. Do not use `to_f64_lossy` to satisfy an emission
cost estimate. Count all basis changes and components if emission expands it.

Add cases with axes related by H and S, opposite signed axes, and commuting
two-qubit Clifford operations. Add blocking cases with anticommuting rotations,
measurement, reset, and supported opaque native gates. Arithmetic correctness
does not replace these structural soundness tests.

### CnotMin

Retain checked `Angle` phase terms keyed by exact parities. Named phases and
the three phase terms generated by CZ use exact quarter-turn coefficients.
Do not introduce float conversion in Gray-code synthesis or cost estimation.

Any arithmetic failure invalidates the whole chunk. Keep recording the input
operations needed for rollback, skip candidate synthesis, copy the saved input
in order, and reset the chunk state. Successful updates earlier in the chunk
must not leak into the output after a later failure.

A CZ expansion can fail after one or two term updates. The chunk failure flag
must take precedence over those partial updates. The same rule applies when
coefficient arithmetic fails, a preserved expression is encountered, or a
candidate exceeds the output budget. Numeric failure and a nonprofitable
candidate have distinct diagnostics but both preserve a valid input fragment.

Keep the existing comparison of two-qubit count and total emitted gate count.
Mixed-angle emission may use multiple operations; a phase-map entry is not
necessarily one output gate. Budget exhaustion may shorten computation, never
change the selected circuit's semantics.

### CancelGates

Use only named gates or certified exact quarter-turns for Clifford identities
around Hadamards. A numeric nonzero rotation is a barrier to such identities.
For example, `h; rz(5e-10); h` must retain the rotation's X-axis effect. It
must not become identity through a numerical zero classifier.

Keep exact inverse cancellation and existing native identities whose operands
and literal angles satisfy their full gate semantics. In particular, checking
controlled inverse angles must use literal comparison rather than projective
`2*pi` equivalence. No tolerant classifier from another pass may be imported.

### DecomposeToffoli and DecomposeCz

Their fixed gate identities require no numerical arithmetic. Copy unrelated
angle payloads and circuit metadata unchanged. Regression tests should embed
tiny and huge rotations next to the gates being decomposed, ensuring they are
neither normalized nor reclassified as a side effect.

### SuperOpt, MURM, and PBC

Keep exact finite-gate matrices, cache keys, and MURM encodings in their existing
discrete domain. Lower certified quarter-turns with the exact lowering helper.
General numeric rotations and preserved expressions remain outside that domain.
Do not approximate them to obtain a cache key or a PBC operation.

Native gates with existing exact support retain it. Numeric unitary helpers
used for diagnostics are not proof of equality and do not authorize a
replacement in the exact matrix search. No tolerance-based matrix matching is
added by this design.

### Optimization driver and parallel execution

Apply the same arithmetic policy in direct pass calls, serial pipelines,
parallel chunks, reverse passes, and fixpoint iterations. Keep counters scoped
to one optimization run and explicitly propagate their scope to workers.

Rotation synthesis continues to use its fallible `try_run` path in the driver.
The infallible `Pass` adapter may return the original circuit on failure, as it
does today, but callers requesting completed synthesis must use the fallible
interface. A returned unchanged circuit is not a successful certification.

Preserve `DecomposeRz` and `--decompose-rz` as aliases for decomposition of all
supported rotations. Do not add a separate permissive numeric-folding flag;
the exact accepted-angle policy is the default optimization behavior.

## Parsing and output boundaries

### Preserve the accepted-input contract

The default parser remains compatible: pi-free expressions follow binary64
evaluation order; recognized affine pi expressions retain exact provenance;
unsupported bounded affine arithmetic can preserve the expression. Invalid
division, nonfinite intermediate values, and invalid syntax remain errors.

This contract does not promise that every source expression equals its
mathematical interpretation. For example, pi-free `(1e17 + 1) - 1e17` can be
accepted as zero before optimization begins. Reports and examples must separate
that input evaluation from an optimizer deleting an accepted nonzero angle.

Do not silently replace the default evaluator with a higher-precision one:
that would change accepted inputs. A future strict source mode can reject
inexact intermediate operations, or evaluate an expression with certified
intervals and round once. It needs a separately specified decimal/pi policy,
resource limits, and failures for undecidable rounding. Merely increasing the
working precision is not a certificate. This mode is outside the initial work.

### Exact and approximate export

For numeric-only angles, shortest decimal spelling remains valid for tzap's
binary64 reparse: it reproduces the accepted bits. It does not assert that an
external tool interpreting the decimal as an exact real uses the same angle.

Keep symbolic QASM emission and separate mixed components. Keep the existing
numeric-only lossy exporter explicitly approximate. Converting a pi fraction
to one `f64`, or combining it with a numeric residual, must not silently enter
the ordinary serialization path. Round-trip tests check angle provenance,
literal controlled angles, gate order where relevant, and qubit/classical-bit
counts. The baseline parser and serializer flatten register names. Preserving
those names is separate work; this design does not claim it is implemented.

## Certified rotation synthesis

### Internal interfaces

Keep the public `Angle` payload unchanged. Introduce private synthesis modules
for target preparation, candidate generation, and verification. Their boundaries
must distinguish exact inputs from rounded candidates and certified bounds:

```text
prepare_target(angle, phase_context, budget, limits)
    -> exact accepted target + enclosing reduced target, or failure
generate_candidate(prepared_target, budget, limits)
    -> Clifford+T word, or failure
verify_candidate(exact_target, word, budget, limits)
    -> certified operator upper bound, or failure
```

The prepared target owns temporary dyadic values, exact pi coefficients, and
intervals; it never replaces a circuit angle. `phase_context` identifies an
unconditional rotation or a complete controlled operator. `limits` bounds
precision, preparation retries, candidate attempts, and output size. Certified
upper bounds form a separate internal type whose construction is restricted to
the verifier and outward-rounded accumulation.

The driver collects a certificate per original input gate, including the term
certificates of its expansion. Preserve the existing public `try_run` return
type; add an internal result carrying the circuit and certificates, which it
and the reporting driver share. Keep the public string-based `SynthesisError`
compatible while using a private reason enum to produce consistent diagnostics.
The backend's unchecked error estimate is never a certificate constructor.

### Accuracy metric and scope

Use projective operator distance for unitary synthesis:

```text
d(U, V) = min over real phi of ||U - exp(i*phi) V||_2
```

Here `||.||_2` is the spectral norm. The phase is one scalar for the whole
operator. For a controlled gate, it cannot be chosen independently on active
and inactive control subspaces. Certification may prove an upper bound using
any particular phase; it need not solve the minimization exactly.

Retain `epsilon` as a per-input-gate bound, including that gate's target
conversion, argument reduction, and synthesis. It is not a whole-circuit bound.
For exact surrounding operations, the triangle inequality gives a circuit
bound at most the sum of all per-gate certified bounds. Report that sum only
when every approximate operation contributing to the output is accounted for.
For measured circuits, this can bound outcome discrepancies through the
corresponding unitary replacements; it is not a general channel-distance report.

Probability error is a useful regression display, but cannot certify a unitary:
one state and one measurement can miss an error on other inputs. Production
certification checks the complete local operator.

### Build the target from accepted values

Replace `to_f64_lossy -> bounded_synthesis_angle -> decimal f64 spelling` as
the source of the synthesis target. Decode the numeric component's sign,
significand, and exponent directly into an exact dyadic temporary value. For
normal values, use the implicit leading significand bit; for subnormals, use
the fraction bits with exponent `-1074`. Handle zero separately.

For existing affine angles, evaluate `pi_coefficient * mathematical_pi +
numeric_residual` with directed-rounding bounds. This handles cancellation
between components without first converting the pi coefficient to binary64.
Preserved expressions still fail synthesis rather than guessing a value.

The dependency's current `config_from_theta_epsilon` uses the floating target's
decimal spelling. A decimal that round-trips to the same float can still be a
different exact real. Supply a target through the dependency's arbitrary-
precision configuration fields, with conversion error bounded, or adapt the
dependency if its configuration cannot retain the needed precision. Avoid
assuming that exposing a big-float field certifies its internal arithmetic.

### Certified argument reduction

Remove native binary64 `sin_cos`/`atan2` reduction from the certified path. Its
agreement with the existing floating reference matrix is not evidence that it
meets an arbitrary epsilon for the accepted mathematical angle.

Use directed-rounding intervals for pi and arithmetic to obtain a bounded
representative of the unconditional target. Increase precision until the
interval is narrow enough for the allocated budget. Large binary64 arguments
need precision that accounts for their exponent, not just epsilon. Arguments
near a reduction boundary must retain a valid circular interval or retry at
higher precision; rounding a quotient is not sufficient proof of the branch.

Certified temporary arithmetic can use an MPFR-based backend, or a pure Rust
backend with explicit interval operations and validated transcendental bounds.
[MPFR documents directed rounding for arithmetic and mathematical functions](https://www.mpfr.org/mpfr-current/mpfr.html#Rounding).
Backend selection is an implementation decision with build, portability, and
distribution consequences. The first implementation must have a fixed
precision/work limit and return an error if certification exceeds it. A
hardcoded binary64 epsilon floor is not a substitute for a measured bound.

### Allocate and verify the error

For an unconditional input rotation with budget `epsilon`, initially reserve
at most `epsilon / 4` for target preparation and `epsilon / 2` for candidate
synthesis. The remaining margin covers conversion into the synthesizer and
verification bounds. Budget arithmetic itself uses exact dyadic values or
directed rounding; a rounded-up split must not exceed the original budget.

If the target is prepared within angle error `delta`, the operator error of
`Rz` is at most `abs(delta) / 2`, using the sine bound. `P` can be checked up to
global phase through its unconditional Rz equivalent. Use the appropriate
full-operator bound when checking the original controlled gate.

Generate a candidate with gridsynth, then independently verify it against the
accepted target. Evaluate the Clifford+T word in its exact algebraic domain or
with enclosing intervals. Evaluate the target with enclosing intervals. Choose
a global phase, enclose the resulting matrix difference, and prove its spectral
norm is at most the budget. A certified Frobenius norm upper bound is a valid,
potentially conservative upper bound on the spectral norm. Any phase used in
this bound must itself be included in the interval calculation.

The independent verifier must not reuse a rounded target as its reference or
accept the dependency's requested epsilon as evidence. If the intervals cannot
establish the bound, increase verification precision; if the candidate is too
far away, tighten candidate synthesis and retry. Limit both loops and return
an actionable error on exhaustion. Unknown gridsynth output instructions are
errors. Track or explicitly account for its global-phase instructions during
verification rather than silently ignoring them in the reference operator.

Final verification against the accepted angle is authoritative. Intermediate
budgets control work and explain failures; they cannot override a failed final
certificate. Preserve the lock around the dependency's process-global precision
state, including target conversion and candidate generation that depend on it.

### Controlled rotations and basis changes

Expand `crx`, `cry`, `crz`, and `cp` before projective synthesis. Preserve the
literal input angle through expansion. Use the existing checked half-angle
operation; the smallest positive subnormal cannot be halved into one binary64
value and must fail rather than become zero.

The initial implementation retains this refusal. A later implementation can
build temporary synthesis terms directly from the exact dyadic input, without
storing the half-angle in a gate. That extension needs no public angle change,
but requires a separate internal plan type and tests of the whole expansion.

CRz expansion has two unconditional rotation terms. CP includes the additional
phase term on the control, giving three. Divide the input gate's budget among
its approximate terms without exceeding it; H, S, and CNOT basis operations
consume zero approximation budget. Synthesize Rx with H basis changes and Ry
with its existing S/H basis changes. The same guarantees apply to every axis.

Verify the reconstructed two-qubit gate as well as its terms. A collection of
unconditional approximation phases becomes a whole-circuit scalar because
each synthesized term acts unconditionally in the expanded circuit. Dropping
phase from a controlled block before expansion would instead change a relative
phase and is forbidden. Include exact mathematical `2*pi` and `4*pi` controlled
cases so the distinction cannot regress.

### Failure and reports

Retain `try_run -> Result<Circuit, SynthesisError>` and transactional output
construction. Add structured internal failure reasons for unsupported targets,
half-angle representability, precision limits, candidate limits, and verification
failure. Include the input gate index and requested bound in CLI/Python errors.

Extend numerical reports with certified synthesis count, failed certification
count, bound scope, and an optional certified sum. Keep `uncertified_syntheses`
for legacy results. Store/report upper bounds with outward rounding, or retain
a decimal bound representation if a finite binary64 upper bound is unavailable.
Do not encode an unavailable bound as zero. Keep these fields separate from
randomized matching and accepted-input fallback counters.

During rollout, the existing uncertified synthesis path can remain available
under its explicitly documented scope. Once the certified backend is ready,
switch the default synthesis path to certification. If compatibility requires
retaining legacy synthesis, give it an explicit option and mark its reports
uncertified. Exact optimization does not need that option.

## Validation

### Arithmetic oracle

Use test-only big integers and rationals to decode binary64 values directly
from their bits. For every accepted addition, assert exact rational equality.
Check that every rejection preserves the input circuit or chunk. Supplemental
floating matrix checks must not replace these assertions.

Test positive and negative zero, every subnormal boundary, adjacent normal
values, widely separated exponents, exact cancellation, intermediate overflow,
and seeded random finite bit patterns. Include both successful and refused
cases, so an implementation refusing every merge cannot pass unnoticed.

### Regression circuits

Each row applies to the accepted values, unless it explicitly concerns parser
evaluation. The output need not match a specific gate sequence when several
exact representations are possible.

| Input | Required behavior |
| --- | --- |
| `rz(1e16); rz(1); rz(-1e16)` | Preserve an exact total of one radian; never emit identity |
| `t; rz(1e17); rz(-1e17)` | Preserve T's mathematical pi contribution |
| `rz(0.125); rz(0.25)` | Allow the exact numeric merge to `rz(0.375)` |
| `rz(0.1); rz(0.2)` | Refuse emission as one rounded binary64 sum |
| `rz(5e-10)` and `rz(5e-7)` | Preserve the nonzero angles in exact passes |
| Numeric `PI/2 + 5e-10` | Preserve a numeric angle; never infer S |
| `h; rz(5e-10); h` | Preserve the nonzero X rotation |
| `rz(1e16)` | Keep the numeric value in exact passes; certify or reject synthesis |
| `rz(28296951008113760)` | Do not erase it using a rounded numeric period |
| `rz(1e308); rz(1e308)` | Preserve finite input gates when their sum overflows |
| `rz(0.0001)` repeated 10,000 times | Preserve the exact rational sum of accepted binary64 values |
| `[rz(d); y; rz(-d); y]` repeated, `d=9e-7` | Preserve the accumulated rotation, including axis signs |
| `crz` with mathematical `2*pi` | Preserve its observable control phase |
| `cp` with mathematical `2*pi` | Preserve its exact identity behavior |
| Controlled rotation at minimum positive subnormal | Return a half-angle error in the initial synthesis implementation |
| Pi-free `(1e17 + 1) - 1e17` | Test the documented parser result separately from optimizer behavior |

Repeat numeric folding cases for Rx, Ry, Rz, and P where the pass supports
those rotations. For unsupported gate families, test preservation and fragment
boundaries. Include combinations of named phases and numeric rotations, mixed
pi/residual replacements on each physical axis, and literal controlled inverses.

Use the existing [angle soundness tests](../tests/angle_soundness.rs) and
[numerical stress fuzzer](../tests/numerical_fuzz.rs) as foundations. Extend the
independent oracle's gate domain before using it to claim coverage of native
Rx/Ry or controlled rotations. Exact phase-polynomial checks alone do not cover
general non-diagonal circuits.

### Synthesis oracle

Use an independent directed-rounding implementation for accepted angles and
the generated word. Increasing precision and observing agreement is useful
debugging evidence, but the acceptance decision must use an enclosing bound.

Cover ordinary, tiny, huge, mixed, and exact pi inputs. Include `rz(1e16)` at
`epsilon=1e-20`, where the former reduction path exceeded the requested bound
in investigation. Run all rotation families over several epsilons. Verify a
complete controlled operator, including inactive-control amplitudes. Exercise
budget underflow, verification retry, resource exhaustion, malformed backend
output, concurrent synthesis calls, and absence of partial output on failure.

For demonstration output, report the change in the probability of X outcome
`+1` on an initial `|+>` state, in percentage points. For a single Rz, the ideal
probability is `(1 + cos(theta)) / 2`. Compute it from the accepted angle at
certified precision. Use additional states and measurements for native and
controlled gate tests; this illustrative probability is not the certificate.

### Integration and performance

Run direct passes and the O1/O2/O3 pipelines, with and without synthesis.
Exercise serial and parallel execution, fixpoint rounds, reversed fragments,
QASM round trips, measurements/reset, and inputs with multiple named registers.
Verify the baseline's documented flattening behavior, qubit/classical-bit counts,
and classical outputs as well as unitary fragments. If register preservation is
merged later, verify those names survive too. Counters may vary
with chunking; correctness and certification scope must not.

Benchmark named Clifford+T circuits, numeric rotations with exactly mergeable
dyadic angles, numeric workloads with many refusals, and synthesis at large
angles and tight epsilons. Measure time, peak memory, output gate counts,
refusals, precision, and retries. Ordinary exact optimization should retain its
current gate layout and allocation behavior. Certified synthesis can cost more;
its cost must be bounded and reported separately from folding performance.

## Implementation sequence

1. Preserve and document the existing folding invariants. Extend adversarial
   native-gate, rollback, and floating-environment tests. Keep current numerical
   diagnostics and all public angle constructors.
2. Implement exact temporary decoding and certified target preparation behind
   an internal synthesis interface. Choose the interval backend and establish
   precision/resource limits. Keep uncertified and certified paths identifiable.
3. Add independent word verification and bounded retries. Include target
   preparation and conversion in the per-input-gate certificate. Retain the
   existing half-angle refusal for unrepresentable controlled terms.
4. Integrate structured failures and numerical reports in Rust, CLI, and Python;
   make certified synthesis the default once its coverage meets the contract.
5. Add full local-operator and pipeline stress tests, then measure performance
   and optimization quality. Document refusal costs rather than recovering gates
   through undocumented tolerances.

The implementation is complete when exact passes add no angle rounding,
refused rewrites preserve input semantics, and every synthesis advertised as
certified has a verified bound against the accepted angle. Parser mathematical
source semantics and a whole-circuit budgeting option remain separate features.

## Applying the policy to a raw f64 model

If implemented on the earlier raw-`f64` branch, the public gate representation
can remain unchanged. Add only a pass-local accumulator:

```rust
enum AccumulatedPhase {
    NamedQuarterTurns(u8),
    Numeric(f64),
}
```

Named/named merges use integer arithmetic modulo eight in an unconditional
phase context. Numeric/numeric merges use checked exact addition. Initially
decline named/numeric merges; do not turn the named contribution into floating
pi. Keep the two original operations. This sacrifices some folds while retaining
their exact meaning. More elaborate separate-component emission is possible
later, but requires signed-axis and output-cost checks before mutation.

For `CnotMin`, either use the same tagged phase-map entries and roll back on an
incompatible merge, or make numeric rotations chunk boundaries in an initial
named-only implementation. The latter is simpler and gives up opportunities
without changing circuit semantics.

`CancelGates` must treat every nonzero numeric rotation as non-Clifford. The
parser can retain its existing binary64 input interpretation while reporting
that boundary. Certified synthesis can decode raw floats directly into temporary
exact dyadics, so it also requires no persistent angle change.

This adaptation cannot recover mathematical pi provenance already lost during
parsing. It can prevent optimization from adding further numerical error.
Latest `main` already preserves more provenance with `Angle`; no part of this
design requires removing that support.

## Implementation on this branch

The certified path is the default. It uses a pure Rust backend: integer
fixed-point intervals with outward-rounded division, Machin's formula for pi,
and Taylor polynomials with explicit remainder bounds. Accepted floats are
decoded from their bits. The candidate word is evaluated exactly in
`Z[omega]/sqrt(2)^k` with temporary big integers, then converted to intervals
once. Complete controlled operators receive an additional matrix certificate.
This avoids interval widening through every intermediate gate multiplication.

The verifier bounds the Frobenius norm, which upper-bounds the requested
projective spectral norm. Controlled terms initially receive `epsilon/(4*n)`
with a downward-rounded split; the final complete operator must still pass
against the input gate's epsilon. Named-only lowering skips that numerical
budget. Repeated unconditional targets reuse a bounded cache of 64 previously
verified words, keyed by literal accepted angle, exact temporary scale, and
identical epsilon.

Limits are 2,048 interval bits, 8,192 instructions per candidate word, and three
candidate attempts. Terms sent to the synthesis backend require epsilon at least
`1e-80`; tighter controlled-term budgets can hit this limit before the input
epsilon does. Directly certified controlled identity candidates do not invoke the
backend and can succeed at smaller epsilon within the interval precision limit. A
limit or failed verification returns a typed internal failure, displayed through
the existing public synthesis error with the input gate index and epsilon.
There is no fallback to uncertified synthesis.

Existing folding safeguards remain in place. TwoSum now has an additional
allocation-free integer check of the exact binary sum. Numeric zero recognition
and half-angle representability also use bits, and signed arithmetic avoids
floating multiplication by a sign. Public angle payloads and gate layout are
unchanged.

Rust, JSON, and Python reports expose certified input-gate counts, failed
certification counts, and the outward-rounded sum of synthesized input-gate
bounds. The sum covers synthesis only; parser input interpretation and unchecked
randomized matching remain separate.

Regression inputs include the investigation's large-angle, tiny-angle,
near-Clifford, overflow, mixed named/numeric, controlled-phase, and rational
coefficient examples. The full repeated circuits contain 400,000 gates for the
Rz/Y example and 600,000 gates for the P/X example. They run as explicit release
stress tests; smaller versions also run in the normal suite. The independent
rational phase interpreter is shared with the numerical stress fuzzer.

The boundary follow-up implements temporary exact half-angle plans for controlled
synthesis without changing the public Angle representation. A scale-aware cache
keeps whole, positive-half, and negative-half targets distinct. Direct identity
candidates are verified against the complete original controlled operator; a
nonidentity input approximated by identity is counted as certified synthesis even
when its angle is a symbolic quarter turn.

The same follow-up adds shared exact SDK export, complete explicitly lossy export,
and independent 420-digit dense reference checks. SDK mixed-angle regressions
cover all axes, large positive/negative values, maximum binary64 values, default
serial/parallel pipelines, and controlled relative phases. The rational phase
interpreter also recognizes diagonalized Rx/Ry basis sandwiches. The floating
matrix helper now multiplies symbolic and numeric phase factors separately; it
remains a floating helper, rather than an accuracy certificate.

Strict mathematical source evaluation and verification of randomized Boolean
matches remain separate future extensions.

### Validation results

The full Rust suite and Python suite pass after the boundary follow-up. Python
reports 427 passed and one optional test skipped. The release stress run also
passes all 2,048 seeded numerical-fuzz
circuits and both full repeated-circuit regressions. The arithmetic guard tests
exercise flushed and directed-rounded candidate results without changing the
floating-point environment of other test threads. Python lint, formatting, and
native stub checks pass. Strict Clippy still reports existing warnings in older
modules; the new synthesis modules introduce none.

The follow-up replayed all 158 QASM inputs in `tzap-superopt/benchmarks` through
the release serial O3 pipeline with warm MURM caches. Every input and output
metric matched the saved `origin/main` baseline. This one-run regression replay
took 98.30 seconds including process startup; it is not a repeated performance
comparison. Dedicated CLI checks also passed 24 synthesis inputs, including
large angles, subnormals, unrepresentable rational halves, and controlled full
turns. Two requests below the backend precision floor were refused without
writing an output circuit.

A local release CLI smoke benchmark used one run per input, with parsing and
process startup included. These figures are observations on the development
machine, not a comparison against `main` or a performance guarantee. Exact
folding used only `PhaseFoldPauli`; synthesis used only `DecomposeRotations`.

| Input | Output gates | Refused rounded folds | Wall time | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| 50,000 pairs of T and Tdg | 0 | 0 | 0.029 s | 19.0 MiB |
| 100,000 repetitions of `rz(0.125)` | 1 | 0 | 0.043 s | 30.3 MiB |
| 50,000 pairs of `rz(0.1); rz(0.2)` | 100,000 | 99,999 | 0.037 s | 25.4 MiB |
| `rz(1e16)`, epsilon `1e-20` | 532 | 0 | 0.048 s | 4.1 MiB |

The final row has one certified input gate and an outward-rounded synthesis
bound of `1.0000000000000001e-20`. Named and numeric exact-folding cases have
zero synthesis error budget. Precision and retry counts are currently internal
resource controls; they are not exposed as public report fields.
