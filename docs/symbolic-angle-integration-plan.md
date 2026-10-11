# Symbolic angle integration plan

Use the standalone symbolic `Angle` as the exact scalar representation throughout
tzap. Keep arithmetic and optimization exact; evaluate expressions numerically
only at explicit synthesis, numeric export, or diagnostic boundaries. Preserve
the speed of the common Clifford+T and finite numeric input paths.

## Implementation status

The scalar storage switch and pass migration are implemented on
`codex/symbolic-angle`, based on main at `f29e728`. Circuit fields alias the
symbolic type; native parsing is exact; SDK imports preserve binary64 values;
per-pipeline and per-worker contexts enforce resource limits. Cancellation and
phase folding use gate-aware proof queries, and `CnotMin` uses sparse general
accumulation with rollback. Certified synthesis evaluates full expressions with
outward integer intervals and independently verifies ordinary and controlled
operators. The existing per-input binary64 epsilon contract remains in place.

Independent formal phase tests, arbitrary-precision rational references, exact
controlled-gate matrices, allocation checks and Python SDK equivalence tests
cover this migration. [The benchmark record](symbolic-angle-integration-baseline.md)
separates the preparatory measurements from the integrated implementation.

The detailed design below remains the roadmap. Whole-circuit epsilon allocation,
an exact epsilon API, broader symbolic proof rules, and a fallible whole-circuit
serialization budget are follow-up extensions. General random/Pauli folds use
bounded canonical merging; sparse accumulation is currently used by `CnotMin`.
Source syntax and per-expression limits remain enforced, and callers can bound
individual text output with `Angle::to_expression_string`.

## Scalar and gate semantics

`Angle::Eq` and `Hash` must agree on literal normalized scalar expressions.
`0`, `2*pi`, and `4*pi` remain distinct values. Structural inequality is not a
proof of mathematical inequality. Only `Proof::Proven` authorizes an exact
rewrite; `Unknown` preserves the original gates.

Centralize gate identity, inverse, quarter-turn classification, and emission in
a gate-aware adapter. Every caller must supply the gate family and whether the
output is an executed circuit considered up to global phase or a literal reusable
operator. The existing `RotationPeriod` selects an angle period; it does not by
itself track phases introduced by lowering to named gates.

| Use | Safe period | Required behavior |
| --- | --- | --- |
| Scalar arithmetic and function arguments | None | Retain literal values before division, halving, and function application. |
| Executed RX, RY, RZ up to whole-circuit global phase | `2*pi` | Projective reductions are allowed only in this scope. |
| Literal RX, RY, RZ operators and bodies that may later be controlled | `4*pi` | Preserve the sign at `2*pi` and phases introduced by named-gate lowering. |
| P and CP | `2*pi` | Their literal operator period is already `2*pi`. |
| CRX, CRY, CRZ | `4*pi` | Reduce and expand while preserving relative control phases. |

For example, CRZ(`2*pi`) is Z on the control, whereas CRZ(`4*pi`) and
CP(`2*pi`) are identities. Never normalize an uncontrolled angle projectively
before adding a control. If reusable controlled bodies are introduced later,
store their global phase explicitly or reject projective optimization of them.

## Context and failure handling

Introduce an internal run state with an `AngleContext`, limits, arithmetic
diagnostics, and any certified evaluation cache. Pass implementations receive
mutable access explicitly through internal methods such as `run_with_state`.
Existing public `Pass::run` entry points create a local state and delegate; the
optimization driver reuses one state across a serial pipeline and its fixpoint
iterations. A rayon chunk owns its own state. No global interner or mutex enters
exact arithmetic, and circuits contain only owned angles, never context IDs.

Thread the state through `pass.rs`, `optimize.rs`, and each arithmetic consumer.
Direct parser and pass APIs must use the same checks as the driver. Context reuse
across calls can be exposed later without making it a circuit ownership rule.
Cross-context values remain usable without recursively importing their DAGs.

Keep per-operation limits and add run or chunk budgets for total normalization
work, retained memory, output expression bytes, and evaluation work. A million
individually valid operations must still have a bounded aggregate cost. Aggregate
worker counters at stage boundaries rather than adding an atomic operation to
every successful scalar operation.

Construct each replacement before mutating deletion marks, Pauli frames, phase
maps, or circuit output. Arithmetic and budget failures retain the affected input
region. Required synthesis and output conversion return an indexed error.
Preserve the existing distinction between fallible `try_run` and the public
infallible pass adapter, which returns the original circuit on failure.

## Implementation sequence

### 1 Capture the baseline and complete core adapters

Record end-to-end release measurements for the merged branch and main at
`f29e728`, then establish regression fixtures for all angle-bearing gates. Include
the new certified synthesis and SDK export paths in the baseline.

Add the gate-aware proof and emission adapter without changing circuit storage.
Test literal equality against explicit rotation equivalence, including both
periods and phase-sensitive controlled operators. Add an internal read-only DAG
visitor for evaluator and serializer use; keep interner keys and fingerprints
private. Add an exact conversion from every legacy affine value, converting its
binary64 residual as a dyadic rational and retaining its literal pi coefficient.

Build a bounded phase accumulator with a modulo-eight fast path for named
Clifford+T gates and a general symbolic path. Benchmark incremental sorted merging
against a sparse coefficient map and balanced batching. Choose the measured
winner: repeatedly rebuilding a sorted sum of N distinct terms must not require
quadratic work. The accumulator must finalize through the core's canonical rules,
support sign changes, and report limits before committing a rewrite.

**Exit checks:** legacy affine conversion preserves exact accepted values; forced
hash collisions cannot establish equality; common named-gate operations allocate
nothing; large unique-term folds have bounded growth; tiny limits leave inputs
unchanged. No circuit storage migration is required for this step.

### 2 Add certified evaluation for symbolic expressions

Extract reusable enclosing arithmetic from `synthesis/interval.rs` into an
internal numerical module while preserving the existing verifier. Add arbitrary
rational division and enclosing evaluation for every core node: rationals, pi,
sums, products, quotients, integer powers, sin, cos, tan, exp, ln, and sqrt.
Extend the current integer interval implementation incrementally; do not replace
the verified boundary with a general floating-point calculator.

Expose an internal interface conceptually equivalent to
`enclose(angle, precision_bits, limits) -> Result<Enclosure, EvaluationError>`.
Use outward integer bounds, cache each shared node once per requested precision,
and refine precision only when a requested result or domain proof remains
unresolved. Bounded range reduction must account for operand magnitude and pi
uncertainty, including huge rational arguments. An interval containing zero is
not proof of zero. Refinement exhaustion returns an error or `Unknown`.

Domain checks remain prior to cancellation. The current core rejects valid
partial-function expressions when its cheap sign rules cannot prove their
domains. Keep that conservative behavior initially. Then offer an explicit
construction or parsing path that can consult the certified evaluator for a
nonzero or positive operand before creating the partial node. Keep numerical
refinement out of ordinary fast arithmetic.

**Exit checks:** independent rational and high precision references fall inside
enclosures; tests cover subnormals, huge arguments, near-zero denominators,
logarithm and square-root boundaries, and tangent poles. Tests must check
enclosure containment and failure behavior, not just agreement of midpoints.
Existing synthesis certificate tests continue to pass unchanged.

### 3 Replace circuit storage and migrate existing consumers

Make `tzap::angle::Angle` expose the symbolic scalar type, with gate helpers
living in the adapter. Change all angle-bearing fields in `circuit.rs` together:
P, RX, RY, RZ, CP, CRX, CRY, and CRZ. Retain checked finite-float convenience
constructors. Replace affine component inspection with exact classification,
conversion, or enclosing evaluation according to the consumer's purpose.

Treat the equality and hashing change as a public API migration. Audit gate
equality, circuit comparisons, fixpoint detection, map keys, inverse matching,
and synthesis caches in the same change. Remove assumptions that every angle is
affine or that a failure to classify proves the gate non-Clifford mathematically.
Preserve the existing input grammar in this step by converting its accepted
legacy values exactly; enable the new source semantics in step 5.

| Consumer | Migration |
| --- | --- |
| `cancel.rs` | Use explicit gate periods and proved inverse or identity checks. Unknown relationships remain unchanged. |
| `phase_fold_pauli.rs` | Accumulate signed exact expressions on proved matching axes; finalize before deletion and frame changes. Preserve RX and RY reconstruction. |
| `phase_fold_rand.rs` | Replace angle merging and emission with the accumulator. Preserve its separate randomized matching contract; exact angles do not eliminate sketch collisions. |
| `cnot_min.rs` | Carry exact angles in parity maps. Preserve complete chunk rollback on failed arithmetic or output budgets. |
| `decompose.rs` | Use exact half and negation for controlled expansion; preserve CP control phases and verification of the complete original gate. |
| `synthesis.rs` | Enclose the full symbolic target and scaled targets; remove dependence on affine `components()` and residual binary64 exponent estimates. |
| `angle_export.rs` | Use exact representability tests and gate-aware lowering. Reject numeric export of arbitrary expressions unless approximation is explicitly requested. |
| `super_opt` | Lower only proved supported discrete rotations. General expressions remain barriers to exact matrix windows. |
| `pbc/convert.rs` | Accept only the proved supported discrete fragment; retain the existing compact `PauliAngle` representation. |
| `unitary.rs` and test utilities | Make approximate evaluation explicit and fallible for unsupported expressions. Keep numerical comparisons supplemental to exact and interval checks. |

This is one atomic storage switch, split into preparatory refactors if needed.
Every consumer must compile and preserve its guarantee when it lands; do not
temporarily make an unsupported expression disappear or route it through f64.

**Exit checks:** complete Rust and Python suites pass; circuits outlive their
contexts; repeated and parallel optimization agree on guarantees; all native
gate families preserve literal control phases; existing certification and safe
SDK export regressions pass. `Angle` and `Gate` sizes do not grow on supported
64-bit targets without a separately justified performance decision.

### 4 Extend the decomposition boundary and budgets

Keep `DecomposeRotations::try_run` as the fallible public boundary and
`DecomposeRz` as its compatibility alias. Its sequence becomes:

1. Classify exact supported gates and identities with gate-specific semantics.
2. Expand controlled gates with exact half-angles and negation.
3. Enclose the original symbolic target and any scaled targets under evaluation
   limits, then produce a high precision candidate input.
4. Let gridsynth propose a bounded word; independently verify it against the
   original expression, including conversion and range-reduction uncertainty.
5. Verify the assembled controlled operator, then commit the output and report
   its bound. Failure preserves the original circuit or returns an indexed error.

Keep the gridsynth batch lock: its dependency still has global precision state.
Replace the verified-word cache key with literal symbolic equality plus scale,
budget, and certification settings. Never use projective scalar equality or
interner pointer identity as a persistent or cross-context key. Keep the cache
bounded and avoid holding its lock during evaluation or synthesis.

The current epsilon contract is per original input gate; its reported outward
sum covers synthesis error across those gates. Preserve that default. Add an
explicit whole-circuit mode that allocates a total budget before parallel
chunking, using stable original-gate accounting. Internal lowered terms must
share their parent gate's allocation rather than each claiming the full bound.

Add exact rational or decimal epsilon input for precision beyond binary64 and
replace the fixed `1e-80` floor with explicit supported evaluation and synthesis
limits. Make this an API extension, retaining the f64 convenience route. Exact
passes spend zero numerical error. Reports distinguish synthesis, numeric
export, and any combined budget; they also retain the separate randomized
matching guarantee.

**Exit checks:** symbolic rational, mixed, and function targets are certified;
near-identity deletion occurs only under explicit approximation; failure and
precision limits are transactional; parallel allocations sum within the total
budget; controlled `2*pi` and `4*pi` cases survive cold and warm cache paths.

### 5 Migrate parsing and exact serialization

Replace `angle_expr.rs` evaluation with the core parser plus source diagnostics
and a declared input policy. In the new exact source policy, decimal `0.1` is
`1/10`, `0.1+0.2` is `3/10`, and `pi*pi/4` remains symbolic. Support the core's
six functions and integer powers only where the selected source grammar permits
them. Never silently restore numeric fallback after a limit or domain failure.

Provide an explicit compatibility policy for the former binary64 source
evaluation during migration. Preserve finite values supplied directly by Rust,
Qiskit, and PennyLane as exact dyadic inputs. If QASM 3 support is added, honor its
typed floating-point and angle semantics separately; the scalar parser must not
override source-language types. Unresolved variables and trainable parameters
remain outside the current closed-expression model.

Preserved legacy source expressions need a separate opaque instruction path
until reparsing under a declared policy succeeds. Do not put unvalidated source
text into the exact scalar DAG or allow it to participate in folding.

Add fallible bounded serialization for exact angles in all eight native gate
families. Write integer rational coefficients, mathematical pi, and supported
expression syntax. A source dialect that cannot express a value must return an
error or require explicit synthesis or approximate export. Bounds apply across
the whole output, since text can expand a compact shared DAG many times.

If structured angle persistence is needed, define a versioned DAG encoding with
explicit node tags, rational coefficients, and topological child references.
Validate domains, lengths, and resource limits on read. Node fingerprints,
pointer addresses, and display order are never the format. Existing SuperOpt
MURM files contain discrete matrices rather than symbolic angles; audit their
encoding before deciding whether a cache version bump is necessary.

**Exit checks:** exact parse/serialize/parse round trips preserve all gate kinds
and literal parameters; source profiles produce their documented meanings;
malformed Unicode, huge inputs, domain failures, and output expansion fail
cleanly with diagnostics. No source expression becomes a float implicitly.

### 6 Complete SDKs reports and release migration

Thread source policies and limits through CLI options, Python bindings, and
reports. Update `_native.pyi`, `_core.py`, Qiskit and PennyLane adapters, `API.md`,
and the angle migration notes together. Add an explicit exact-expression route
for Python users while retaining numeric input provenance and rejecting
unresolved SDK parameters.

Safe SDK export may emit named gates or exactly representable finite binary64
parameters. General dyadic results are not necessarily representable by one
binary64 value. Reuse known exact decompositions when applicable; otherwise
return an actionable error. Keep the existing explicitly lossy export clearly
reported, and add a certified export route with a declared tolerance. Its error
bound includes both expression evaluation and final binary64 rounding. Neither
route changes the circuit's stored symbolic angle.

Report unknown proof skips, normalization and evaluation limits, source policy,
certified synthesis bounds, and export bounds without presenting all exact input
as numerical fallback. Retire obsolete rounded-fold counters through a
documented schema transition. Document that expression normalization is bounded
and mathematical equivalence remains undecidable by this interface in general.

**Exit checks:** Rust, CLI, and Python expose the same policies and errors;
SDK round trips preserve exact supported cases and reject silent conversion;
reports retain certification scope; wheel builds and CI pass on supported
platforms. Publish the public equality, parsing, and serialization changes in the
next appropriately versioned API release.

## Performance acceptance

Measure every implementation stage against the merged baseline with identical
release settings, inputs, seeds, cache state, and worker count. Alternate baseline
and candidate runs and compare medians and tail latency over repeated runs.
Separate parsing, pass execution, synthesis, and serialization costs. Record
peak RSS, allocations, retained and live nodes, output bytes, gate counts, and
T counts; faster execution obtained by skipping optimization is not a win.

Use the existing `feynman`, `cobble-t`, `cobble-rz`, and `qft` corpora. Add
synthetic numeric-only, rational-pi, repeated-function, mostly-unique-expression,
heavy-cancellation, huge-coefficient, deep-expression, and controlled-rotation
workloads. Include cold and warm interning and synthesis caches, serial and
parallel pipelines, and round trips through both SDKs. Track the standalone
microbenchmarks alongside circuit measurements rather than extrapolating them.

The proposed gate for common Clifford+T workloads is no repeatable throughput
regression above 3%, no new per-named-gate allocations or big-integer evaluation,
and no increase in circuit storage stride. Investigate RSS and tail-latency
regressions even when average runtime passes. General symbolic workloads must
demonstrate bounded memory and avoid quadratic accumulation. Collect build time,
binary size, and wheel size as production dependencies grow. These are acceptance
targets, not measurements of integration that has already happened.

## Correctness coverage and completion

Expand the independent exact phase and channel oracles to exercise symbolic
construction, each migrated pass, and complete pipelines. Cover negative Pauli
axes, entangled spectators, measurements and reset, literal controlled periods,
and phase-sensitive reusable operators. Randomized tests need recorded seeds and
an independent rational oracle; numerical unitary comparisons alone cannot
establish exactness or a certificate.

Force hash collisions, context expiration, cross-context combination, resource
failures at each mutation boundary, cache eviction, and parallel budget
allocation. Check that normalization and serialization limits leave stored
values valid and that an unsupported proof never becomes an identity rewrite.

Integration is complete when every stored circuit angle uses the symbolic type;
all arithmetic consumers use exact checked operations; all period-sensitive
operations state their semantics; source and output policies are explicit; and
synthesis and certified numeric export verify the full original expression.
Existing discrete SuperOpt and PBC representations remain specialized.

The next implementation change is step 2: reusable certified evaluation of the
full scalar expression. Complete it before the atomic circuit storage switch so
the migration preserves main's synthesis and SDK guarantees from its first
usable commit.
