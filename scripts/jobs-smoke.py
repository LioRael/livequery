#!/usr/bin/env python3
"""Acceptance: identify a stuck worker, repair it, and observe recovery without code edits."""
import json
import os
import pty
from pathlib import Path
import select
import subprocess
import tempfile
import time

root = Path(__file__).resolve().parents[1]
cli = root / 'target/debug/livequery'

def line(process):
    assert select.select([process.stdout], [], [], 8)[0], 'service/CLI output timed out'
    return process.stdout.readline()

with tempfile.TemporaryDirectory(prefix='lqj-') as directory:
    socket = str(Path(directory) / 'a.sock')
    host = subprocess.Popen([str(root / 'target/debug/jobs'), socket], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    watch = None
    try:
        assert json.loads(line(host))['stalled_worker'] == 2
        def command(sql):
            return json.loads(subprocess.check_output([str(cli), 'query', sql, '--socket', socket, '--json'], text=True, timeout=5))
        deadline = time.monotonic() + 8
        while True:
            result = command("SELECT id, worker_id, elapsed_ms FROM jobs WHERE state = 'running' ORDER BY elapsed_ms DESC LIMIT 20")
            if result['rows'] and all(row['worker_id'] == 2 and row['elapsed_ms'] >= 300 for row in result['rows']):
                break
            assert time.monotonic() < deadline, result
            time.sleep(.1)
        assert command("SELECT COUNT(*) FROM jobs WHERE state = 'running'")['rows'][0]['count'] == 4
        assert command("SELECT MAX(elapsed_ms) FROM jobs WHERE state = 'running'")['rows'][0]['max'] >= 300
        assert result['publication']['sequence'] == result['revision']['sequence']
        assert result['publication']['age_ms'] is not None
        host.stdin.write('submit\n'); host.stdin.flush()
        assert line(host).strip() == 'accepted', 'business ingress must remain available'
        watch = subprocess.Popen([str(cli), 'watch', "SELECT COUNT(*) FROM jobs WHERE state = 'running'", '--socket', socket, '--json', '--interval-ms', '100', '--count', '20'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        assert json.loads(line(watch))['rows'][0]['count'] > 0
        host.stdin.write('repair 2\n'); host.stdin.flush()
        assert line(host).strip() == 'worker 2 repaired'
        recovered = False
        while watch.poll() is None:
            output = line(watch)
            if output and json.loads(output)['rows'][0]['count'] == 0:
                recovered = True
                break
        assert recovered, 'WATCH must observe running count recover to zero'
        watch.communicate(timeout=5)
        assert watch.returncode == 0
        session = subprocess.run([str(cli), 'session', '--socket', socket], input='schema\nSELECT COUNT(*) FROM jobs\nSELECT nope FROM jobs\nSELECT MAX(elapsed_ms) FROM jobs\nquit\n', capture_output=True, text=True, timeout=8)
        assert session.returncode == 0, session.stderr
        assert 'unknown field' in session.stderr and 'available fields' in session.stderr
        assert 'count' in session.stdout and 'max' in session.stdout
        master, slave = pty.openpty()
        terminal = subprocess.Popen([str(cli), 'session', '--socket', socket], stdin=slave, stdout=slave, stderr=slave, env={**os.environ, 'TERM': 'xterm-256color'})
        os.close(slave)
        try:
            def until(marker, prompt_after=False):
                output = b''
                deadline = time.monotonic() + 8
                while marker not in output or (prompt_after and b'livequery>' not in output[output.find(marker) + len(marker):]):
                    assert time.monotonic() < deadline, output
                    if select.select([master], [], [], .2)[0]:
                        output += os.read(master, 65536)
                return output
            until(b'livequery>')
            os.write(master, b'SELECT CO\t FROM jobs\r')
            assert b'count' in until(b'scanned', True), 'Tab should complete COUNT(*)'
            os.write(master, b'\x1b[A\r')
            assert b'count' in until(b'scanned', True), 'Up should recall the previous query'
            os.write(master, b'quit\r')
            # A real terminal keeps draining output while termios restores the mode.
            deadline = time.monotonic() + 5
            while terminal.poll() is None:
                assert time.monotonic() < deadline, 'session shutdown timed out'
                if select.select([master], [], [], .1)[0]:
                    try: os.read(master, 65536)
                    except OSError: pass
            terminal.wait(timeout=1)
            assert terminal.returncode == 0
        finally:
            if terminal.poll() is None: terminal.kill(); terminal.wait()
            os.close(master)
        host.stdin.write('quit\n'); host.stdin.flush()
        host.communicate(timeout=5)
        assert host.returncode == 0
        assert not Path(socket).exists(), 'shutdown must remove its socket'
        print('jobs: discover stalled worker, accept requests, aggregate, WATCH repair, interactive recovery and cleanup PASS')
    finally:
        for process in [watch, host]:
            if process is not None and process.poll() is None:
                process.kill(); process.communicate(timeout=5)
