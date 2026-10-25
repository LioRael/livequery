#!/usr/bin/env python3
"""Exercise real CLI discovery, schema, query, watch and a Tokio TCP echo service."""
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / 'target/debug/livequery'

def cli(pid, *args):
    return subprocess.check_output([str(BIN), *args, '--pid', str(pid), '--json'], text=True)

def run_example(name, env):
    process = subprocess.Popen([str(ROOT / 'target/debug' / name)], stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, env={**os.environ, **env})
    try:
        line = process.stdout.readline().strip()
        assert line, process.stderr.read()
        print(line)
        if name == 'async-server':
            host, port = re.search(r'echo ([^:]+):(\d+)', line).groups()
            with socket.create_connection((host, int(port)), timeout=2) as stream:
                stream.sendall(b'hello-livequery')
                assert stream.recv(128) == b'hello-livequery'
            time.sleep(.05)
            data = json.loads(cli(process.pid, 'query', 'SELECT * FROM requests'))
            assert data['rows'][0] == {'bytes': 15, 'completed': 1}, data
        else:
            schemas = json.loads(cli(process.pid, 'schema'))
            assert 'players' in schemas
            assert 'token' not in str(schemas)
            discovered = subprocess.check_output([str(BIN), 'list', '--json'], text=True)
            assert any(str(process.pid) == item['pid'] for item in map(json.loads, discovered.splitlines()))
            json.loads(subprocess.check_output([str(BIN), 'attach', str(process.pid), '--json'], text=True))
            output = cli(process.pid, 'watch', 'SELECT * FROM players', '--interval-ms', '300', '--count', '4')
            frames = [json.loads(line) for line in output.splitlines()]
            assert len(frames) >= 2, frames
            assert frames[0]['rows'] != frames[-1]['rows']
            table = subprocess.check_output([str(BIN), 'query', 'SELECT id, health FROM players', '--pid', str(process.pid)], text=True)
            assert 'scanned' in table
            print(table.strip())
            failed = subprocess.run([str(BIN), 'query', 'DELETE FROM players', '--pid', str(process.pid)], capture_output=True, text=True)
            assert failed.returncode == 1 and 'invalid' in failed.stderr
        process.wait(timeout=15)
        assert process.returncode == 0, process.stderr.read()
        print(f'{name}: PASS (graceful exit, real socket and CLI)')
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=3)

run_example('concurrent', {'LIVEQUERY_DEMO_TICKS': '12'})
run_example('async-server', {'LIVEQUERY_DEMO_SECONDS': '3'})
