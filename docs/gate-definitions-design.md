# OpenQASM 2 gate definitions in tzap

Status: implemented and integrated with `main` at `76588fe`, 2026-10-13
(tzap 0.8.0). Original design baseline: `ec8a3bd` (0.7.0).
Work branch: `codex/gate-definitions`.

Accept custom `gate` definitions and inline every executed call before returning
a `Circuit`. Keep `Circuit`, `Gate`, register storage, and the optimizer pass
interfaces unchanged. Definitions and parameter expressions belong only to the
QASM frontend. Optimization runs on the fully expanded circuit, and output
contains ordinary operations rather than definitions or custom calls.

## Scope and observable behavior

The initial release supports parameterized and unparameterized definitions,
empty bodies, calls to earlier definitions, and mixed indexed/register operands.
Bodies may use tzap's existing unitary gates plus the QASM primitives `U` and
`CX`. All parameters must become finite constants during parsing. This is a
supported QASM 2 subset, not full language conformance.

Inlining preserves operation order and operand roles. It introduces no qubits,
classical bits, measurements, or resets. Unused definitions emit nothing. An
empty gate invocation still counts as an executed statement for the existing
rule prohibiting register declarations after execution begins. A definition
itself does not start execution.

Keep the existing top-level measurement/reset and register behavior. Reject
`if`, `opaque`, and arbitrary external includes. Reject barriers inside custom
bodies, including unused bodies; preserving barriers requires a separate
representation. Existing top-level barrier skipping remains a documented
limitation, not a precedent for silently discarding barriers in new syntax.

The semantic contract is the same quantum behavior up to a whole-circuit global
phase, under the frontend's documented parameter evaluation policy. Do not
optimize or export reusable gate bodies separately. Fully inline first, then run
the existing passes. A future facility for controlling or exporting a custom
unitary needs a separate exact-phase contract.

## Example

```qasm
OPENQASM 2.0;
include "qelib1.inc";

gate pair(theta) a,b {
    rz(theta/2) a;
    cx a,b;
    rz(-theta/2) b;
}

gate twice(theta) a,b {
    pair(theta) a,b;
    pair(theta) a,b;
}

qreg q[2];
twice(pi/4) q[0],q[1];
```

Parsing produces two repetitions of `rz(pi/8) q[0]; cx q[0],q[1];
rz(-pi/8) q[1];`, represented using exact symbolic angle values. The
optimizer may subsequently combine operations across the original call boundary.

## Frontend structure

Use a streaming top-level parser and store only reusable definition bodies.
Avoid constructing an AST for every operation in a large flat input. Suggested
private modules:

```text
src/qasm/scan.rs       borrowed statement text, lines, statement boundaries
src/qasm/expr.rs       expression parsing, resolution, evaluation
src/qasm/definitions.rs definition registry and body validation
src/qasm/inline.rs     argument binding, broadcasting, bounded expansion
src/qasm/gate_tests.rs regression and semantic tests
```

`src/qasm.rs` remains the public entry point and serializer. Keep
`parse(&str) -> Result<Circuit, String>` and `Circuit::from_qasm` compatible.
Errors use the existing string boundary with statement line numbers and bounded
call traces. The iterative expansion stack lives in `definitions.rs`;
`inline.rs` handles top-level operands, broadcasting, and resource budgets.

Logical frontend types, with details finalized during implementation:

```rust
struct GateDefinition {
    name: String,
    parameters: Vec<String>,
    qubits: Vec<String>,
    body: Vec<BodyCall>,
    line: usize,
}

struct BodyCall {
    callee: CallableId,
    parameters: Vec<Expr>,
    qubits: Vec<FormalQubitId>,
    line: usize,
}

enum CallableId {
    Native(NativeGateId),
    Custom(DefinitionId),
}
```

Expression references similarly use formal parameter IDs after resolution.
Store names for diagnostics, not repeated lookup during expansion. Custom arity
is not limited to three: frontend operand storage may spill beyond the current
`SmallVec` capacity, while emitted native operations retain their existing arity.

## Scanning and syntax

Replace physical-line/semicolon splitting with a scanner that respects comments,
quoted include names, parentheses, and braces. Track statement line numbers
while scanning source bytes. Recognize complete identifiers before dispatch; prefixes
such as `gatekeeper` must not be treated as keywords.

Accept definitions spanning lines or written on one line, multiple body calls
per line, and top-level calls immediately after a closing brace. A definition
ends at `}`, without requiring a trailing semicolon. Ordinary statements retain
their semicolon terminator inside bodies. Retain the legacy allowance for a
final top-level statement without a semicolon. Detect missing body terminators, mismatched delimiters,
unexpected nested definitions, and unterminated comments/strings explicitly.
Keep block comments as the existing tzap extension and treat comments as token
separators. Read tokens lazily or borrow source slices to avoid a token vector
proportional to every gate in the input.

Accept `gate g a { ... }`, `gate g() a { ... }`, and parameter lists such as
`gate g(theta,phi) a,b { ... }`; apply the corresponding optional empty
parentheses rule to calls. New custom identifiers use `[a-z][A-Za-z0-9_]*`.

## Names and definition validation

Use a signature table for native operations and a map from custom names to
stable definition IDs. Resolve each definition as it appears, then publish it
only after its body is valid. This makes all custom dependencies point backward.

The language requires declaration before use, local formal symbols, unindexed
body operands, and at least one qubit formal. It allows empty bodies and excludes
recursive gate calls. [OpenQASM 2 specification, section 2][qasm2]

Apply these additional tzap validation policies:

- Reject repeated parameter names, repeated qubit names, and overlap between the
  two formal lists. Reject reserved keywords/constants as formal names.
- Resolve expression identifiers only as parameters and body operands only as
  qubit formals. Global registers are inaccessible inside definitions.
- Reject unknown callees, direct recursion, and forward references. Reject
  nonunitary statements, declarations, includes, and barriers in bodies.
- Check parameter and qubit arity at definition time, including unused bodies.
  Reject repeated formal operands in an individual call.
- Reject duplicate custom names and custom names colliding with predefined
  callables or global register names. Check custom/register collisions in either
  declaration order. Preserve existing register-only compatibility.

Keep predefined names available without requiring an include, matching current
tzap behavior. `include "qelib1.inc"` remains a recognized marker and performs no
filesystem access. Reserve only supported callable names, not every name that
could appear in an external library. Add `U`, `CX`, `u1`, `u2`, `u3`, and `id` to the
signature table for the primitive lowering described below.

This collision policy is intentionally restrictive: a pasted library redefining
`h`, `cx`, or other predefined names is rejected. Do not silently substitute the
native implementation for a user-written definition. Supporting arbitrary
library definitions or shadowing is later work.

## Parameter evaluation

Parameter templates use resolved postfix operations and formal parameter IDs.
Closed expressions share the current exact scalar parser and cache. Template
binding uses the same `AngleContext` arithmetic, functions, domains, integer
powers, and resource checks as closed scalar expressions. QASM decimals remain
exact rationals and `pi` remains mathematical pi; no binary64 evaluation is
introduced by inlining. Unary signs and powers keep the scalar parser's
precedence and right associativity.

Evaluate actual parameters in the caller's environment and bind exact `Angle`
values by formal ID. Evaluate body expressions in the callee's environment.
Do not substitute source text or reduce literal parameters modulo a gate period.
Per-operation scalar limits and context interner rotation remain active.
`parse_numeric_sdk` keeps finite binary64 input literals, rejects top-level
nonliteral parameters, and permits structural arithmetic on those bound values
inside definitions. Ordinary QASM uses exact mathematical source semantics.

Never reduce a bound parameter modulo an angle period before body evaluation.
For example, `theta = 2*pi` must yield `theta/2 = pi`; a body implementing
a controlled rotation must retain its control-relative phase. Do not infer an exact pi
fraction from a float or introduce new tolerances during binding.

Validate expression syntax and formal references in every definition. Evaluate
constant subexpressions during validation so invalid constants fail even in
unused definitions; parameter-dependent domain errors fail when instantiated.

## Primitive lowering and phase

Keep native gates native during expansion. A body containing `ccx`, `cz`, or
`ccz` emits that existing variant rather than secretly invoking a decomposition
pass. Requested decomposition and optimization retain their normal ordering.

Lower `CX a,b` to `Gate::cnot`. The QASM 2 paper defines
`U(theta,phi,lambda) = Rz(phi) Ry(theta) Rz(lambda)`.
[OpenQASM 2 specification, equation 2][qasm2]

Lower `U` in execution order to `rz(lambda); ry(theta); rz(phi)` using
current native variants. Support `u3` through the same lowering, `u2(phi,lambda)`
through `U(pi/2,phi,lambda)`, and `id` through an empty expansion. Keep `u1` as
native P and `cu1` as native CP, preserving the current serializer and library
conventions. The complete-circuit phase contract covers alternative U conventions
that differ only by a scalar.

Every current native unitary is available inside definitions, including phase,
axis/controlled rotations, Y/SX, SWAP/CSWAP, and controlled Y/H. No gate variants
or generic quantum controls are added. A custom controlled operation must
express its control using actual gates in the body; retain all control-relative
phase terms and literal angles.

## Operand binding and broadcasting

At top level resolve each operand into an indexed qubit or a register descriptor
containing offset and width. Avoid allocating a vector for every whole register.
For a body call, map formal operand IDs through the current qubit binding.

When registers are supplied, all register operands must have equal widths;
indexed operands are reused for each lane. Expand lane zero completely, then
lane one, and so on, preserving body order within each lane. This matches the
specified custom-gate broadcast rule. [OpenQASM 2 specification, section 2][qasm2]

Check the custom signature before broadcasting and reject aliased actual qubits
in any instantiated call, including empty bodies. Validate descriptors once:
duplicate indexed qubits, equal register offsets, or an indexed qubit inside a
register range indicate an alias at some lane. Sorted descriptors permit this
check without iterating lanes or repeating a quadratic comparison. Skip alias
checking when there are no instances. Overlap between different
lanes is permitted; lane order defines execution. A bare register of width one
is still a register, not an indexed scalar that broadcasts against a wider one.
Retain tzap's zero-size-register extension: equal zero-width register operands
produce no instances, but names, arities, parameter values, and width agreement
must still be validated. Unequal widths fail before any expansion.

Use the same operand expansion helper for native unitary calls so `CX`, `U`, and
custom gates have consistent broadcasting. Measurement keeps its separate
quantum/classical width checks.

## Expansion limits and errors

Keep definition bodies as a dependency graph rather than eagerly copying nested
bodies. Compute saturating expansion costs after validation. Include both native
operations emitted and work units: each call visit, bound qubit, bound parameter,
and executed expression opcode costs one unit. Cached constant expressions cost
one unit. Include nested binding and evaluation in the cached cost. Exponential
chains and high-arity broadcasts of empty gates must consume work even without
output.

Use an explicit expansion stack and bounded expression nesting. Introduce
frontend-only `ParseLimits` and `parse_with_limits`; the existing `parse` uses
documented defaults. Limits cover custom call depth, expression nesting, emitted
operations attributable to custom calls or native register broadcasts, and
expansion work. Both kinds of implicit expansion share output/work budgets;
identity broadcasts consume work even though they emit no operations. Defaults
are call/expression depth 256, ten million expanded native operations, and twenty
million work units. Expression depth can be lowered but cannot exceed
256, bounding recursive expression parsing even with custom limits. Explicit
indexed native statements are not subject to the output/work budgets, including
primitive aliases that lower to several operations.

Check broadcast multiplication, operation counts, IDs, and allocations for
overflow. Check budgets and reserve output fallibly before iterating either a
custom call or a native register broadcast. Report limit exhaustion as a parse
error, never truncate an expansion.
Collect output privately and return a `Circuit` only after the complete input
succeeds. No caller receives a partially parsed circuit.

Errors identify the failing statement line. Signature errors include the gate
name and expected/actual counts. Runtime parameter failures include definition
locations and a bounded call trace. A runtime
binding failure for a body expression such as `1/theta` might read:

```text
line 18: while expanding twice, called pair at line 10:
line 4: division by zero in pair with theta = 0
```

## Output and integration

The existing serializer writes expanded native operations using its current
flattened register layout. Preserve the existing quantum/classical bit counts;
source register names are already lost on this baseline. Register preservation
is separate work and must not require changing `Circuit` here. Definition names, comments, and
call boundaries do not survive output. No option to preserve or reconstruct
definitions is included.

The CLI and Python QASM string interface receive support through the shared
Rust parser. Gate counts and detected gate sets describe the expanded circuit;
custom calls and unused definitions do not contribute separately. No new pass,
optimization flag, or changes to Qiskit/PennyLane object adapters are required.
Keep framework object support distinct from acceptance of QASM source strings.

## Implementation sequence and acceptance checks

1. **Structural parsing.** Add line diagnostics and streaming statement parsing;
   retain flat-circuit behavior. Check existing parser tests,
   malformed delimiters, comments, multiline statements, and exact keyword
   boundaries. Compare release parse time and peak memory on large repository
   benchmarks before proceeding.
2. **Definitions and inlining.** Add signatures, definition storage, local
   resolution, nonparameterized nested calls, and expansion limits. Verify
   handwritten expansions, arbitrary custom arity, empty and unused definitions,
   declaration ordering, collisions, recursion/forward-reference errors, and
   exponential expansion including empty-body chains.
3. **Parameters and primitive support.** Add expression binding, `U`/`CX` and
   `u1`/`u2`/`u3`/`id` lowering, plus register broadcasting. Check nested parameter
   scopes, negative/scientific values, `2*pi` followed by division, controlled
   rotation phase, mismatched widths, register offsets, aliasing, and zero widths.
4. **Integration and documentation.** Exercise CLI and Python source-string
   entry points, flat serialization/reparse, and optimization across call
   boundaries. Update README/API/module support descriptions and explain the
   collision, expression, barrier, and output policies. Run the relevant Rust
   suite and Python bindings tests in the supported build configuration.

Use independent small-circuit matrix calculations for primitive lowering and
parameterized definitions. Use the exact bounded Clifford+T/channel oracle where
its fragment applies. Compare both parsed expansion and optimized behavior to
handwritten flat circuits, including surrounding measurements and resets.
Numerical matrix checks use declared tolerances and do not establish exact real
arithmetic. Keep semantic tests separate from comparisons of emitted gate lists.

Completion requires all accepted custom calls to disappear before optimization,
unchanged circuit/pass interfaces, valid flat output, actionable errors, and no
unresolved regression in parsing ordinary large circuits.

## Validation measurements

The release parser was compared to an independently compiled parser from the
original 0.7.0 baseline, before integration with symbolic angles. These are
historical measurements of the initial frontend, not current 0.8.0 performance
claims. Nine parses per input were timed in one process; figures below
are median wall time. macOS maximum resident set size covers the benchmark
process, including its input and allocator, rather than just the circuit.

| Input | Baseline parse | Inlining frontend parse | Baseline peak resident size | Frontend peak resident size |
| --- | --- | --- | --- | --- |
| Synthetic 10 MB circuit with 999999 native gates | 65.7 ms | 51.3 ms | 315 MB | 170 MB |
| 14.6 MB gf2^256_mult benchmark | 81.2 ms | 64.6 ms | 340 MB | 179 MB |

Measurements are local observations, not a guarantee across machines or input
families. The frontend borrows ordinary statement text, uses a native indexed
operand path, and transfers the emitted vector directly into the unchanged
`Circuit`. It avoids a token tree for the whole input and a second circuit gate
vector.

[qasm2]: https://arxiv.org/html/1707.03429
