#!/usr/bin/env python3
"""Check actual Bevy Remote values after reflection mutation; no performance ranking."""
import json, pathlib, socket, subprocess, time, urllib.request
root=pathlib.Path(__file__).resolve().parents[1]
bin=root/'experiments/comparison/target/debug'
print(subprocess.check_output([str(bin/'interact')],text=True))
with socket.socket() as s:
    s.bind(('127.0.0.1',0));port=s.getsockname()[1]
proc=subprocess.Popen([str(bin/'bevy'),str(port)])
try:
    request=urllib.request.Request(f'http://127.0.0.1:{port}', data=json.dumps({'jsonrpc':'2.0','id':1,'method':'world.query','params':{'data':{'components':['bevy::Player']},'strict':True}}).encode(),headers={'Content-Type':'application/json'})
    for attempt in range(50):
        try:
            with urllib.request.urlopen(request,timeout=1) as response: result=json.load(response)
            break
        except OSError: time.sleep(.05)
    else: raise RuntimeError('Bevy Remote did not start')
    assert 'result' in result,result
    assert result['result'][0]['components']['bevy::Player']=={'id':42,'health':20},result
    print(json.dumps({'bevy_remote':result}))
finally:
    proc.terminate();proc.wait()
