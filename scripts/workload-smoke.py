#!/usr/bin/env python3
"""Validate diverse queries against unchanged sync/Tokio hosts and coherent publication."""
import json, pathlib, socket, subprocess, time
root=pathlib.Path(__file__).resolve().parents[1]
binary=root/'experiments/workload/target/release/livequery-workload'
def request(path, command):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(2);sock.connect(path);sock.sendall(json.dumps(command).encode()+b'\n')
        with sock.makefile('rb') as stream: reply=json.loads(stream.readline())
    assert reply['status']!='error',reply
    return reply
for engine in ['sync','async']:
    proc=subprocess.Popen([str(binary),engine,'published','3'],stdout=subprocess.PIPE,text=True)
    try:
        meta=json.loads(proc.stdout.readline())
        with socket.create_connection(('127.0.0.1',meta['port']),timeout=2) as stream:
            for _ in range(120):
                stream.sendall(b'x'*64)
                data=b''
                while len(data)<64: data+=stream.recv(64-len(data))
                assert data==b'x'*64
        time.sleep(.1)
        schema=request(meta['socket'],{'command':'schema'})['data']
        assert schema['counters'][0]['schema']
        for sql,expected in [('SELECT completed FROM counters',[{'completed':120}]),('SELECT bytes FROM counters WHERE completed > 100 AND bytes > 6400 LIMIT 1',[{'bytes':7680}])]:
            assert request(meta['socket'],{'command':'query','sql':sql})['data']['rows']==expected
        observed=request(meta['socket'],{'command':'observe','sql':'SELECT * FROM counters','since':None})
        row=observed['data']['rows'][0];assert row['bytes']==row['completed']*64
        print(engine+': schema, projection, compound predicate, limit and coherent observation PASS')
    finally:
        proc.terminate();proc.wait()
        # Forced termination cannot run Server::drop; remove only this fixture's directory.
        if 'meta' in locals():
            path=pathlib.Path(meta['socket']);path.unlink(missing_ok=True);path.parent.rmdir()
