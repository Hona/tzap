# Symbolic angle integration baseline

The first integration stage adds adapters and accumulation without changing
circuit storage or pass behavior. These measurements establish the circuit
baseline for later integration and select a general accumulation strategy.

Measurements used an Apple M3, ARM64, Rust 1.98.1, and the existing release
profile. Main is `f29e728`; the standalone baseline is `7248129`. The candidate
includes the stage 1 gate adapter, borrowed expression view, accumulator, and
tests. Dependencies match across builds, including num-bigint 0.4.8,
num-rational 0.4.2, rayon 1.12.0, rustc-hash 2.1.3, and smallvec 1.16.3.

## Circuit measurements

Seven runs alternate binary order, with no concurrent builds or test runs.
The ordinary circuits use `-O1 --no-parallel -q --json`; the native fixture uses
`--passes DecomposeRotations --epsilon 1e-10 --no-parallel -q --json`. Each
sample is a separate process. Times include process startup, parsing, execution,
metrics, and JSON output. Peak RSS comes from macOS `/usr/bin/time -l`.

| Workload | Main ms | Standalone ms | Stage 1 ms | Stage 1 versus standalone |
| --- | ---: | ---: | ---: | ---: |
| Feynman hwb12 | 139.62 | 135.66 | 132.93 | -2.0% |
| Cobble Rz laplacian-filter | 4.07 | 4.11 | 4.06 | -1.3% |
| QFT qft40 | 184.05 | 184.18 | 183.40 | -0.4% |
| Certified native fixture | 146.59 | 147.54 | 147.09 | -0.3% |

| Workload | Main peak RSS MiB | Standalone peak RSS MiB | Stage 1 peak RSS MiB |
| --- | ---: | ---: | ---: |
| Feynman hwb12 | 131.73 | 131.94 | 131.20 |
| Cobble Rz laplacian-filter | 3.09 | 3.05 | 3.05 |
| QFT qft40 | 233.05 | 233.02 | 233.03 |
| Certified native fixture | 4.59 | 4.48 | 4.48 |

All samples match on output metrics and numerical guarantees. Output gate and T
counts are respectively 346,646 and 85,611 for hwb12; 1,290 and 172 for
laplacian-filter; 657,332 and 310,907 for qft40; and 3,780 and 1,489 for the native
fixture. The fixture reports nine certified input gates and a summed error bound
of `9.000000000000005e-10`. No sampled circuit has a median slowdown above the
proposed 3% threshold. Small improvements are within machine variation and do
not show a faster integrated optimizer, since passes have not been migrated.

Observed stage 1 wall-time ranges are 131.22–138.79 ms for hwb12,
3.86–4.93 ms for laplacian-filter, 181.19–186.46 ms for qft40, and
146.56–174.72 ms for the certified fixture. These are local baselines, not a
cross-platform performance guarantee.

## Parsing SDK export and synthesis

The native fixture exercises all eight angle-bearing gate families, mixed pi
and binary64 inputs, controlled `2*pi` and `4*pi` cases, a subnormal input, and a
spectator wire. A common harness builds against all three revisions. Three pairs
of process runs alternate order; each process reports the median of seven
samples. Parsing and SDK export use 1,000 operations per sample, and synthesis
uses ten after warming the verified-word cache.

| Operation | Main us | Standalone us | Stage 1 us |
| --- | ---: | ---: | ---: |
| Parse native fixture | 4.64 | 4.56 | 4.83 |
| Exact SDK export | 5.39 | 5.41 | 5.50 |
| Certified synthesis with warm cache | 4,278.91 | 4,262.30 | 4,288.12 |

The small parser fixture increased by 0.27 microseconds versus the standalone
baseline; keep it in future comparisons alongside end-to-end circuit timing.
Warm synthesis still verifies assembled controlled operators, so its time is
not merely cache lookup. Cold certification appears in the separate-process CLI
fixture above.

## Accumulation strategy

The accumulator benchmark compares sequential canonical merging, recursively
balanced batching, and a sparse coefficient map. Each fold includes constructing
its accumulator and finalizing the canonical result. Inputs are held alive;
sin of integer radians remains symbolic. General arithmetic does not reduce
these terms by an angle period. Contexts persist across folds and have identical
limits. Each measurement reports the median of seven samples, ten complete
folds per sample, after one warmup.

| Distinct terms | Sequential us | Balanced us | Sparse us |
| ---: | ---: | ---: | ---: |
| 16 | 5.22 | 3.12 | 1.51 |
| 64 | 81.34 | 27.38 | 8.71 |
| 256 | 1,123.07 | 83.22 | 22.85 |
| 1,024 | 20,573.06 | 436.10 | 102.41 |

| Repeated identical terms | Sequential us | Balanced us | Sparse us |
| ---: | ---: | ---: | ---: |
| 16 | 2.45 | 2.45 | 1.11 |
| 64 | 10.45 | 10.15 | 4.32 |
| 256 | 42.65 | 41.79 | 15.99 |
| 1,024 | 175.14 | 167.52 | 66.03 |

Use the sparse strategy for the general accumulator. It is about 4.3 times faster
than balanced batching and 201 times faster than sequential merging for 1,024
distinct terms on this machine. Its map merges coefficients incrementally and
sorts once at finalization; it does not rebuild every growing sum. These timings
exclude source parsing and expression construction and do not predict whole-pass
speedups. Named quarter accumulation remains a separate inline path with zero
allocations, verified in both debug and release allocation tests.

## Reproduction and validation

Run the strategy and boundary harnesses with:

```sh
cargo run --release --example symbolic_angle_accumulate_bench
cargo run --release --example symbolic_angle_baseline_bench
```

Build each revision in a separate checkout or archived source directory with
the same resolved dependencies. Copy the boundary harness and its fixture into
older revisions to compare identical work. Keep each binary before switching
revisions, then run:

```sh
python3 scripts/symbolic_angle_compare.py \
  main=/absolute/path/to/main-binary \
  baseline=/absolute/path/to/standalone-binary \
  candidate=/absolute/path/to/stage1-binary --runs 7
```

The comparison script validates output metrics and numerical reports before
printing samples, medians, ranges, and RSS. RSS is available on macOS; other
platforms still report wall time and metrics.

Validation includes 50 scalar and adapter unit tests, two integration tests with
an independent exact matrix oracle, the allocation test, and the complete
default Rust suite. The matrix oracle operates in `Q[zeta_16]` and compares
literal operators including the adapter's global phase. Existing synthesis
tests continue to independently verify proposed words against accepted targets.
Clippy's all-target check requires exempting two pre-existing approximate
constant diagnostics in `tests/numerical_fuzz.rs`; no symbolic adapter warnings
remain.

## Integrated circuits and passes: 2026-10-11

The integrated candidate stores symbolic angles in every circuit gate and uses
exact arithmetic throughout the passes. Main is pinned to `f29e728` (`origin/main`
at the start of integration); the candidate is `codex/symbolic-angle`, following
`e02a6a6`. This compares the implementation above with the complete migration.

The paper manifest in `~/git/tzap-superopt/evaluation/rq3/rq2-benchmarks.txt`
contains 135 circuits and 110,600,280 input gates. Its SHA-256 is
`4e301ecf3d6b22eb62d00467177ef3ce84ba44082af24961655be7a24df0c363`. All files came from the local corpus,
including locally added suites. Three repetitions of each version alternate
binary order, giving 810 successful runs, with no concurrent builds or tests.
The machine, compiler and dependency versions match the preceding baseline.

The release harness runs serial O3 with the Base MURM basis, three-qubit windows,
25 window gates and 200,000 MURM entries. Toffoli gates are decomposed before
optimization, matching paper input preparation. Both versions share the same
cache directory and harness source. Timings exclude observed MURM loading/building
and output serialization/hashing. Pipeline time includes reading, parsing and
input Toffoli decomposition. Totals sum per-circuit medians.

| Scope | Main seconds | Symbolic seconds | Change |
| --- | ---: | ---: | ---: |
| Optimization, all 135 circuits | 90.465 | 90.672 | +0.23% |
| Read/parse/decompose/optimize pipeline | 99.975 | 100.295 | +0.32% |
| Optimization, 76 circuits with no input Rz | 18.393 | 18.618 | +1.22% |

The three complete repetition sums change by +0.94%, +0.85% and -1.14%.
The aggregate difference is small relative to run variation; these local numbers
are not a universal slowdown bound. Angle and Gate remain 16 and 32 bytes on
this ARM64 target, and inline scalar/quarter-turn allocation checks pass.

| Suite | Cases | Main optimization seconds | Symbolic seconds | Change |
| --- | ---: | ---: | ---: | ---: |
| cobble-rz | 31 | 71.947 | 71.928 | -0.03% |
| feynman | 18 | 4.516 | 4.604 | +1.95% |
| large-circuits | 72 | 7.370 | 7.398 | +0.38% |
| qft | 4 | 6.595 | 6.705 | +1.67% |
| quasar-large | 10 | 0.038 | 0.038 | -0.93% |

All 76 circuits with no input Rz have identical output text and metrics.
Across all circuits, 98 match output metrics and 84 match serialized output.
Native decimal inputs now mean exact decimal rationals, and formerly refused
rounded/overflowing folds can succeed, so numerical circuits need not have the
same output as main. Total output gates fall from 9,939,079 to 9,936,046, and Rz
counts fall from 35,524 to 32,554. Total T counts change from 3,720,954 to
3,720,958; two-qubit counts change from 3,769,454 to 3,769,385. These are measured
quality differences, not a claim that every metric improves on every circuit.
All output hashes, metrics, rounds and numerical reports are deterministic
within each version across the three repetitions.

Per-circuit costs vary. `spectral-thresholding-n13` takes five O3 rounds on main
and seven after exact folding, and is 14.92% slower in the full run. Seven fresh
alternating samples confirm an 8.46% slowdown (326.18 vs 353.79 ms). Two other
apparent outliers mostly disappear in the seven-sample check: `urf1_278` changes
by +0.93%, and `qft_q020_d32421` by -0.10%. The largest case, `ols-ridge-d20`, is
3.36% slower in the full run, and had been 0.83% slower in the quiet pilot.
The complete dataset retains these differences rather than hiding outliers.

A separate one-process-per-version peak-memory measurement of `ols-ridge-d20`
uses macOS `/usr/bin/time -l`: main peaks at 3,507.0 MiB and symbolic at
3,038.7 MiB. This is one sample and includes process/library/allocator memory;
it does not establish a general memory improvement.

The new coverage includes independent formal phase signatures for general
functions across cancellation, both phase folders and CnotMin, 64 seeded
expression streams, forced resource failures and rollback, exact gate periods,
independent rational series enclosures for all six functions, certified complete
controlled operators, exact multi-parameter SDK export and 692 Python tests.
Release stress checks use `NUMERICAL_FUZZ_CASES=128` and `PBC_FUZZ_CASES=4` and
include the 5,120-case native configuration matrix and QCEC equivalence checks.

The local reproduction bundle is `/tmp/tzap-symbolic-angle/integrated-superopt-run`:
`run.py full`, `analyze.py`, `full.jsonl`, `summary.json`, per-version stderr,
`outliers.jsonl` and memory records. The unchanged server source and Cargo
manifests are in `/tmp/tzap-symbolic-angle/superopt-run/{main,candidate}`.
Build the candidate with its release manifest, copy `angle-benchmark` to the
reproduction bundle's `candidate-benchmark`, then run `python3 run.py full`
without competing builds/tests. `summary.json` records binary checksums and the
method; the main source snapshot is `/tmp/tzap-symbolic-angle/stage1-main`.
