#!/usr/bin/env python3
"""Check real alpha.2 binaries in both directions."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
old_cli = ROOT / 'target/compat-install/bin/livequery'
new_cli = ROOT / 'target/debug/livequery'
old_host = ROOT / 'tests/compat-alpha2/target/debug/livequery-legacy-host'
new_host = ROOT / 'target/debug/concurrent'


def spawn(binary):
    child = subprocess.Popen([str(binary)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                             env={**os.environ, 'LIVEQUERY_DEMO_TICKS': '20'})
    assert child.stdout.readline().startswith('PID ')
    return child


def stop(child):
    child.wait(timeout=8)
    assert child.returncode == 0, child.stderr.read()


legacy = spawn(old_host)
try:
    result = subprocess.run([str(new_cli), 'schema', '--pid', str(legacy.pid)], capture_output=True, text=True)
    assert result.returncode != 0 and 'incompatible' in result.stderr, result
finally:
    stop(legacy)

current = spawn(new_host)
try:
    result = subprocess.check_output([str(old_cli), 'watch', 'SELECT * FROM players', '--pid', str(current.pid),
                                      '--count', '3', '--interval-ms', '300', '--json'], text=True)
    frames = [json.loads(line) for line in result.splitlines()]
    assert len(frames) >= 2 and frames[0]['rows'] != frames[-1]['rows']
    status = json.loads(subprocess.check_output([str(new_cli), 'status', '--pid', str(current.pid), '--json'], text=True))
    assert status['players']['age_ms'] is not None
finally:
    stop(current)
print('new CLI rejects alpha.2 Agent explicitly; alpha.2 CLI still queries/watches new Agent; status PASS')
