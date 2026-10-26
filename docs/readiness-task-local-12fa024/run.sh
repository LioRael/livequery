#!/bin/bash
set -euo pipefail
cd /Users/leosouthey/.codex/worktrees/investigation-experience/livequery
out=target/task-local-12fa024
/Users/leosouthey/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin/rustc -Vv > "$out/toolchain.txt"
git rev-parse HEAD >> "$out/toolchain.txt"
sw_vers >> "$out/toolchain.txt"
shasum -a 256 experiments/workload/target/release/livequery-workload >> "$out/toolchain.txt"
for engine in sync async; do
 python3 scripts/benchmark-service.py --only baseline --task-rows 1000 --string-bytes 128 --engine "$engine" --seconds 30 --repeats 3 --output "$out/$engine-baseline.json"
 for period in 10 100; do
  for observers in 1 4; do
   python3 scripts/benchmark-service.py --task-rows 1000 --string-bytes 128 --publish-ms "$period" --subscribers "$observers" --engine "$engine" --seconds 30 --repeats 3 --output "$out/$engine-cost-$period-$observers.json"
  done
 done
 python3 scripts/benchmark-service.py --task-rows 1000 --string-bytes 128 --engine "$engine" --seconds 600 --repeats 1 --output "$out/$engine-1000.json"
 python3 scripts/check-load.py "$out/$engine-1000.json" --min-seconds 600 --engine "$engine"
 python3 scripts/benchmark-service.py --task-rows 5000 --string-bytes 96 --engine "$engine" --seconds 7200 --repeats 1 --output "$out/$engine-5000.json"
 python3 scripts/check-load.py "$out/$engine-5000.json" --min-seconds 7200 --engine "$engine"
done
printf 'PASS\n' > "$out/completed.txt"
