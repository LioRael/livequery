#!/usr/bin/env bash
set -euo pipefail
engine="$1"
python3 scripts/benchmark-service.py --binary target/ci-bin/livequery-workload --only soak --engine "$engine" --repeats 1 --seconds 7200 --output target/endurance.json
python3 scripts/check-load.py target/endurance.json --min-seconds 7200 --engine "$engine"
