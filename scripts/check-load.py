#!/usr/bin/env python3
"""Enforce correctness/resource gates, not machine-dependent throughput claims."""
import json
from pathlib import Path
import sys

report = json.loads(Path(sys.argv[1]).read_text())
for row in report["results"]:
    assert row["request_errors"] == 0, row["request_error_codes"]
    for key in ["observation_error_codes", "publication_error_codes"]:
        assert set(row[key]) <= {"busy"}, (row["engine"], key, row[key])
    # Busy is visible backpressure, but excessive diagnostic loss is not acceptable.
    assert row["observation_errors"] / max(1, row["observations"] + row["observation_errors"]) < .01
    attempted_publications = row["publication_attempts"]
    assert row["publication_errors"] / max(1, attempted_publications) < .01
    samples = [s for s in row["resource_samples"] if s["elapsed_seconds"] >= 10]
    assert len(samples) >= 4, "need at least 30 seconds for a resource trend"
    for key, allowance in [("threads", 1), ("fds", 32), ("rss_bytes", 8 * 1024 * 1024)]:
        values = [s[key] for s in samples if s[key] is not None]
        assert len(values) == len(samples), f"{key} sampling unavailable"
        assert max(values) - min(values) <= allowance, (row["engine"], key, min(values), max(values))
    print(row["engine"], "load correctness/resource gates PASS")
