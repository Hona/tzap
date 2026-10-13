#!/usr/bin/env python3
"""Alternate release CLI baselines; emit wall time, peak RSS, and output metrics.

Usage: python3 scripts/symbolic_angle_compare.py main=/path/to/main
       baseline=/path/to/baseline candidate=/path/to/candidate --runs 5
"""
import argparse
import json
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", nargs="+", help="name=/absolute/binary/path")
    parser.add_argument("--runs", type=int, default=5)
    args = parser.parse_args()
    if args.runs < 3:
        parser.error("use at least three paired runs")
    binaries = dict(item.split("=", 1) for item in args.binaries)
    root = Path(__file__).resolve().parent.parent
    cases = [
        ("hwb12", root / "benchmarks/feynman/hwb12.qasm", ["-O1"]),
        ("laplacian-filter", root / "benchmarks/cobble-rz/laplacian-filter.qasm", ["-O1"]),
        ("qft40", root / "benchmarks/qft/qft_q040_d68921.qasm", ["-O1"]),
        ("certified-native", root / "tests/fixtures/symbolic-angle-baseline.qasm",
         ["--passes", "DecomposeRotations", "--epsilon", "1e-10"]),
    ]
    records = {}
    for case, source, options in cases:
        samples = {name: [] for name in binaries}
        for run in range(args.runs):
            order = list(binaries)
            if run % 2:
                order.reverse()
            for name in order:
                command = [binaries[name], str(source), *options, "--no-parallel", "-q", "--json"]
                if sys.platform == "darwin":
                    command = ["/usr/bin/time", "-l", *command]
                start = time.perf_counter()
                result = subprocess.run(command, capture_output=True, text=True, check=True)
                elapsed = (time.perf_counter()-start)*1000
                report = json.loads(result.stdout)
                rss = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
                samples[name].append({"wall_ms": elapsed, "peak_rss_bytes": int(rss[1]) if rss else None,
                                      "metrics": report["metrics"], "numerical": report["numerical"]})
        # Approximation and optimization quality must agree as well as runtime.
        reference = samples[next(iter(binaries))][0]
        for values in samples.values():
            for value in values:
                if value["metrics"] != reference["metrics"] or value["numerical"] != reference["numerical"]:
                    raise RuntimeError(f"{case}: output metrics or guarantees differ")
        records[case] = {name: {"median_ms": statistics.median(v["wall_ms"] for v in values),
                                "min_ms": min(v["wall_ms"] for v in values),
                                "max_ms": max(v["wall_ms"] for v in values),
                                "median_peak_rss_bytes": statistics.median(v["peak_rss_bytes"] for v in values)
                                if values[0]["peak_rss_bytes"] is not None else None,
                                "samples": values} for name, values in samples.items()}
    print(json.dumps({"runs": args.runs, "worker_mode": "serial", "cases": records}, indent=2))


if __name__ == "__main__":
    main()
