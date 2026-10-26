#!/usr/bin/env python3
"""Enforce correctness/resource gates, not machine-dependent throughput claims."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("report")
parser.add_argument("--min-seconds", type=int, default=30)
parser.add_argument("--engine", choices=["sync", "async", "both"], default="both")
args = parser.parse_args()
report = json.loads(Path(args.report).read_text())
expected = {"sync", "async"} if args.engine == "both" else {args.engine}
assert report["load_seconds"] >= args.min_seconds, "requested duration too short"
assert {row["engine"] for row in report["results"]} == expected, "missing engine results"
assert len(report["results"]) == len(expected) * report["repeats"], "missing trials"
for row in report["results"]:
    assert row["mode"] in {"published", "jobs"} and row["subscribers"] == 4
    if row["mode"] == "jobs":
        assert row["task_rows"] >= 1000
        assert set(row["query_kinds"]) == {"sort", "count", "max"}
        assert all(value > 0 for value in row["query_kinds"].values())
    assert row["elapsed_seconds"] >= args.min_seconds, "trial ended early"
    assert row["requests"] > 0 and row["observations"] > 0, "no useful traffic"
    assert row["publication_attempts"] > 0, "no publication attempts"
    assert row["request_errors"] == 0, row["request_error_codes"]
    for key in ["observation_error_codes", "publication_error_codes"]:
        assert set(row[key]) <= {"busy"}, (row["engine"], key, row[key])
    # Busy is visible backpressure, but excessive diagnostic loss is not acceptable.
    assert row["observation_errors"] / max(1, row["observations"] + row["observation_errors"]) < .01
    attempted_publications = row["publication_attempts"]
    assert row["publication_errors"] / max(1, attempted_publications) < .01
    samples = [s for s in row["resource_samples"] if s["elapsed_seconds"] >= 10]
    assert len(samples) >= max(4, int((args.min_seconds - 10) / 5 * .9)), "insufficient resource samples"
    assert samples[-1]["elapsed_seconds"] >= args.min_seconds - 10, "sampling ended early"
    for previous, current in zip(samples, samples[1:]):
        assert 0 < current["elapsed_seconds"] - previous["elapsed_seconds"] <= 15, "sampling stalled"
        assert current["requests"] > previous["requests"], "business traffic stalled"
        assert current["observations"] > previous["observations"], "observation traffic stalled"
        if row["mode"] == "jobs":
            assert all(current["query_kinds"].get(k, 0) > previous["query_kinds"].get(k, 0) for k in ("sort", "count", "max")), "query kind stalled"
    for key, allowance in [("threads", 1), ("fds", 32), ("rss_bytes", 8 * 1024 * 1024)]:
        values = [s[key] for s in samples if s[key] is not None]
        assert len(values) == len(samples), f"{key} sampling unavailable"
        assert max(values) - min(values) <= allowance, (row["engine"], key, min(values), max(values))
    print(row["engine"], "load correctness/resource gates PASS")
