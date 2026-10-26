from pathlib import Path
import subprocess,time,signal,json,ctypes
p=Path(__file__).resolve().parent
class TS(ctypes.Structure): _fields_=[('s',ctypes.c_long),('n',ctypes.c_long)]
lib=ctypes.CDLL('/usr/lib/libSystem.B.dylib')
def mono():
 t=TS();assert lib.clock_gettime(6,ctypes.byref(t))==0;return t.s*10**9+t.n
target=subprocess.Popen([str(p/'target'),str(p)])
try:
 while not list(p.glob(f'{target.pid}-*.tid')):time.sleep(.001)
 s=subprocess.Popen(['/tmp/lq-native-thread-sampler',str(target.pid),'2',str(p),str(p/'final-samples.jsonl')]);time.sleep(.25)
 a=mono();target.send_signal(signal.SIGSTOP);time.sleep(.12);target.send_signal(signal.SIGCONT);b=mono();target.wait();s.wait()
finally:
 if target.poll() is None:target.kill();target.wait()
rows=[json.loads(l) for l in (p/'final-samples.jsonl').read_text().splitlines()][1:]
inside=[t for r in rows if a+10_000_000<r['monotonic_ns']<b-10_000_000 for t in r['threads']]
outside=[t for r in rows if r['monotonic_ns']<a-10_000_000 or r['monotonic_ns']>b+10_000_000 for t in r['threads'] if 'state' in t]
assert s.returncode==0 and len(inside)>=5 and all(t['state']==3 for t in inside) and len({t['cpu_ns'] for t in inside})==1 and sum(t['state']==1 for t in outside)>20
valid=[r for r in rows if r['sample_end_ns']]
result={'exit':s.returncode,'samples':len(rows),'controlled_stop_start_ns':a,'controlled_stop_end_ns':b,'stopped_samples':len(inside),'stopped_cpu_growth_ns':inside[-1]['cpu_ns']-inside[0]['cpu_ns'],'running_samples':sum(t['state']==1 for t in outside),'observer_cpu_ms':(valid[-1]['observer_cpu_ns']-valid[0]['observer_cpu_ns'])/1e6,'max_read_us':max(r['sample_end_ns']-r['monotonic_ns'] for r in valid)/1000,'note':'One-second owned computing helper, 120ms SIGSTOP. Validates distinction against exact intervention interval; not natural failure evidence.'}
(p/'final-validation.json').write_text(json.dumps(result,indent=2)+'\n');print(result)
