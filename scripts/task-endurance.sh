#!/usr/bin/env bash
set -euo pipefail
engine="$1"
binary=target/ci-bin/livequery-workload
python3 scripts/benchmark-service.py --binary "$binary" --only baseline --task-rows 1000 --string-bytes 128 --engine "$engine" --seconds 30 --repeats 3 --output target/task-baseline.json
for period in 10 100; do
  for observers in 1 4; do
    python3 scripts/benchmark-service.py --binary "$binary" --task-rows 1000 --string-bytes 128 --publish-ms "$period" --subscribers "$observers" --engine "$engine" --seconds 30 --repeats 3 --output "target/task-cost-${period}-${observers}.json"
  done
done
python3 scripts/benchmark-service.py --binary "$binary" --task-rows 1000 --string-bytes 128 --engine "$engine" --seconds 600 --repeats 1 --output target/task-1000.json
python3 scripts/check-load.py target/task-1000.json --min-seconds 600 --engine "$engine"
python3 scripts/benchmark-service.py --binary "$binary" --task-rows 5000 --string-bytes 96 --engine "$engine" --seconds 7200 --repeats 1 --output target/task-5000.json
python3 scripts/check-load.py target/task-5000.json --min-seconds 7200 --engine "$engine"
