# Standalone symbolic angle core

`tzap::symbolic_angle` represents exact, nonperiodic real scalar expressions.
It provides the data structure and its arithmetic, normalization, proof queries,
parser, and interner. Circuit gates, optimization passes, QASM handling, synthesis,
and the existing `tzap::angle::Angle` continue to use their existing representation.

## Construction and arithmetic

```rust
use tzap::symbolic_angle::{Angle, AngleContext, Proof, RotationPeriod};

let mut ctx = AngleContext::default();
let a = ctx.parse("sin(pi/7) + 0.1")?;
let b = ctx.parse("-sin(pi/7) + 0.2")?;
assert_eq!(ctx.add(&a, &b)?, ctx.rational(3, 10)?);

let two_pi = ctx.pi_fraction(2, 1)?;
assert_ne!(two_pi, Angle::ZERO);
assert_eq!(two_pi.prove_identity(RotationPeriod::TwoPi), Proof::Proven);
assert_eq!(two_pi.prove_identity(RotationPeriod::FourPi), Proof::Disproven);
# Ok::<(), tzap::symbolic_angle::Error>(())
```

`AngleContext` supplies checked `add`, `sub`, `neg`, `scale`, `half`, `mul`, `div`,
and integral `pow`, plus `sin`, `cos`, `tan`, `exp`, `ln`, and `sqrt`. Every operation
returns a complete exact value or an error. Angles own their storage and remain
valid after their context is dropped.

Use `rational` and `pi_fraction` for machine integer inputs, or `big_rational` and
`big_pi_fraction` for `num_bigint::BigInt` inputs. Arithmetic promotes coefficients
when they overflow the machine representation and demotes them when they fit again.
There is no floating-point fallback.

`decimal` interprets decimal text exactly. `parse` additionally accepts `pi`,
parentheses, unary signs, the arithmetic operators `+ - * / ^`, and the six
functions. Powers associate to the right, and exponentiation binds more tightly
than a unary sign. Exponents must evaluate to an `i64` integer. Variables and
complex values are outside this closed-real-expression API.

`Angle::from_f64` preserves the exact dyadic value supplied by the caller. It
accepts every finite binary64 value, including subnormals, and canonicalizes signed
zero. It never infers a multiple of pi. For example, decimal `"0.1"` and binary64
`0.1` have different exact meanings. NaN and infinities are rejected.

## Storage and interning

`Angle` occupies 16 bytes on the measured ARM64 target. Small normalized rational
and pi coefficients fit inline as an `i32` numerator and `u32` denominator.
Binary64 inputs also fit inline. Intermediate `Rational` values use `i64` and `u64`
until arbitrary precision is needed. Larger scalar values and expression nodes use
immutable shared storage.

Addition of binary64 inputs and scaling by signed powers of two operate on
integer significands. Exactly representable results stay inline; overflow,
underflow, and sums that require rounding promote to exact rationals. Small
decimal literals use machine arithmetic without temporary strings or big integers.
Reduced coefficients compare and hash their stored numerator and denominator
directly, avoiding repeated division during interning.

Each context has its own weak interner. Keys include the full node structure;
hash collisions never establish equality. Reconstructing a live canonical node
in the same context shares its storage. Cloning an angle shares existing nodes,
and values can be combined across contexts without importing their entire trees.
Separate contexts require no global locks.

Expired weak entries are removed during bucket lookup and when the table reaches
its entry limit. `prune` performs explicit cleanup; `clear_interner` discards the
lookup table while preserving existing angles. `stats` exposes hits, misses,
pruned entries, and currently retained entries. Clearing the table leaves the
cumulative counters intact. `shares_storage` checks node pointer identity and
returns false for inline values.

## Normalization and proof queries

Normalization reduces rationals, collects linear coefficients, sorts sums and
products, and flattens their corresponding associative nodes. Repeated identical
factors and existing powers of the same base combine by adding integral
exponents. Factors remain separate when their combined exponent exceeds `i64`.
Exact rules include quarter-turn trig values,
odd and even parity, rational-pi trig periods, perfect-square roots,
`sin(x)^2 + cos(x)^2`, and valid inverse-function compositions. Multiplication
does not expand arbitrary sums.

`Eq` and `Hash` describe literal equality of the supported normalized forms.
They never apply rotation periods. Structural inequality does not prove two
expressions have different mathematical values. `Ord` supplies a deterministic
structural ordering for canonicalization, rather than numerical real ordering.
Its fingerprints and textual term order are not a persistent storage format.

`prove_zero`, `prove_positive`, `prove_nonnegative`, `prove_identity`, and the
context's `prove_equal` return `Proven`, `Disproven`, or `Unknown`. Sign metadata
and bounded symbolic rules support these queries; no numerical tolerance enters
a proof. This is a limited symbolic core, so some true identities remain unknown.

Partial functions validate domains before simplification. Division and
nonpositive integral powers require a proved nonzero operand; `ln` requires a
proved positive argument; `sqrt` requires a proved nonnegative argument; `tan`
requires a proved nonzero cosine. An invalid domain returns an explicit error.
An unproved domain returns `DomainUnproved`, even when the expression is actually
valid. For example, `sqrt(sin(100))` needs stronger sign analysis than this core
provides, whereas `sqrt(sin(1)^2)` is accepted. Undefined terms cannot disappear
through cancellation.

`quarter_turns` and `prove_identity` take an explicit `TwoPi` or `FourPi` view.
`rotation_view` reduces pure pi coefficients into the chosen centered interval;
it leaves general expressions intact. Scalar arithmetic always retains literal
values such as `2*pi` and `4*pi`.

## Limits and exact text

`Limits` constrains coefficient and exponent bits, expanded expression node counts,
depth, terms, estimated arithmetic work, source bytes, and retained interner
entries. Every public context operation gets a fresh work budget. Parsing shares
one budget across its complete expression. Shared subexpressions count each
occurrence toward the expanded node limit. Failed operations preserve existing
values, although temporary weak entries and statistics can change.

`Display` emits exact expression text and can expand shared subexpressions.
Use `to_expression_string(max_bytes)` to bound output length. Text can be parsed
in another context, subject to that context's input and resource limits. Large
dyadic-to-rational conversions and text output can allocate, even when the
original binary64 input was inline.

Certified numerical evaluation, arbitrary-precision trig evaluation, synthesis,
and runtime circuit integration are future layers. Function values currently
remain symbolic whenever an exact simplification is unavailable.

## Tests and performance measurements

The 37 core tests compare randomized machine and large-rational arithmetic
against independent `BigRational` calculations. They also check binary64 pairs
and power-of-two scaling against an independent exact oracle, promotion and
cancellation, trig periods and signs, power grouping and exponent overflow,
partial domains, resource limits, forced hash collisions, exact text round trips,
malformed Unicode input, and interner capacity and lifetimes.

A separate allocator test verifies zero allocations for the common inline
operations, binary64 negation and halving, addition of minimum subnormals,
symbolic term cancellation, small decimal parsing, hashing large coefficients,
and repeated construction of live interned sin and cos nodes.

Run the standalone microbenchmarks with:

```sh
cargo run --release --example symbolic_angle_bench
```

These measurements used an Apple M3, ARM64, Rust 1.98.1, and the release profile.
The baseline is the standalone core at commit `163aa1b`. Both binaries use the
same expanded benchmark harness. Three pairs of complete runs alternate which
binary runs first. Each run reports the median of seven samples with 100,000
operations per sample; the larger rational case uses 10,000. The table reports
the median of those three run medians.

| Operation | Before ns/op | After ns/op | Speedup |
| --- | ---: | ---: | ---: |
| Construct pi/7 | 12.75 | 12.75 | 1.00× |
| Add pi/7 and 2pi/7 | 15.27 | 14.52 | 1.05× |
| Add pi/4 and 2pi/4 | 13.08 | 12.83 | 1.02× |
| Classify pi/4 as a quarter turn | 0.85 | 0.96 | 0.89× |
| Add binary64 0.25 and 0.5 exactly | 31.03 | 16.13 | 1.92× |
| Reconstruct live sin(pi/7) | 17.00 | 17.88 | 0.95× |
| Cancel a symbolic term with its negative | 107.41 | 14.28 | 7.52× |
| Construct and drop sin(1/3) | 79.51 | 74.76 | 1.06× |
| Add one to a 336-bit rational | 3030.87 | 159.85 | 18.96× |
| Negate binary64 0.1 | 158.69 | 11.19 | 14.18× |
| Halve binary64 0.1 | 169.52 | 15.58 | 10.88× |
| Add minimum subnormals | 963.98 | 15.05 | 64.05× |
| Parse decimal 0.125 | 255.61 | 38.28 | 6.68× |
| Parse sin(pi/7) with its node cached | 359.11 | 131.74 | 2.73× |
| Hash a 336-bit coefficient | 87.41 | 4.99 | 17.52× |

The largest gains come from retaining exact dyadic results inline, avoiding
coefficient division while hashing and comparing, and adding an integer to a
large reduced rational without reducing it again. Simple symbolic cancellation
and machine decimal parsing also avoid temporary allocations.

Small differences in unchanged operations should be interpreted alongside
machine variation: the unchanged legacy quarter-turn classification also moved from
4.35 to 4.97 ns/op between the paired binaries. The current core's cached-function
hit takes 17.88 ns/op. For comparison, the unchanged gate Angle's specialized
quarter-turn addition takes 10.00 ns/op versus 12.83 for this core; its binary64
addition takes 22.38 ns/op versus 16.13 for this core.

These timings describe scalar operations on one machine. Circuit performance
requires separate measurements when integration is implemented.
