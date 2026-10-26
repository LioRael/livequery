import pathlib,subprocess,os,time,json,hashlib,difflib
base=pathlib.Path('target/offcpu-diagnostic');root=base/'system-trace-buffered';(root/'events').mkdir(parents=True,exist_ok=True)
src=base/'source';patch=[]
for rel in ['crates/livequery-core/Cargo.toml','crates/livequery-core/src/lib.rs','crates/livequery-core/src/diagnostic.rs','crates/livequery-core/src/state.rs','crates/livequery-query/src/lib.rs','experiments/workload/Cargo.lock']:
 p=pathlib.Path(rel);old=p.read_text() if p.exists() else '';patch.extend(difflib.unified_diff(old.splitlines(True),(src/rel).read_text().splitlines(True),fromfile='a/'+rel,tofile='b/'+rel))
(root/'instrumentation.patch').write_text(''.join(patch));binary=src/'experiments/workload/target/release/livequery-workload'
(root/'provenance.json').write_text(json.dumps({'base_source':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'diagnostic_only':True,'injected_pause':False,'seconds':120,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'patch_sha256':hashlib.sha256((root/'instrumentation.patch').read_bytes()).hexdigest(),'note':'Modified source defined by patch; benchmark source_ref alone is not provenance. Sampling and stage checkpoints add overhead. Not release acceptance.'},indent=2)+'\n')
env=os.environ.copy();env.pop('LQ_VALIDATE_PAUSE',None);env['LQ_DIAGNOSTIC_DIR']=str((root/'events').resolve())
with (root/'run.log').open('w') as log:
 p=subprocess.Popen(['python3','scripts/benchmark-service.py','--binary',str(binary),'--task-rows','5000','--string-bytes','96','--engine','async','--seconds','120','--repeats','1','--output',str(root/'report.json')],env=env,stdout=log,stderr=log)
 pid=None
 for _ in range(100):
  for child in subprocess.run(['pgrep','-P',str(p.pid)],capture_output=True,text=True).stdout.split():
   if 'livequery-workload' in subprocess.run(['ps','-p',child,'-o','comm='],capture_output=True,text=True).stdout:pid=int(child);break
  if pid:break
  if p.poll() is not None:raise RuntimeError('benchmark exited before workload')
  time.sleep(.05)
 assert pid
 (root/'pid').write_text(str(pid)+'\n')
 with (root/'trace.log').open('w') as slog:
  trace=subprocess.Popen(['xcrun','xctrace','record','--template','System Trace','--attach',str(pid),'--time-limit','125s','--output',str(root/'scheduler.trace')],stdout=slog,stderr=slog)
  code=p.wait()
  (root/'benchmark-completed.json').write_text(json.dumps({'benchmark_exit':code,'trace_status':'saving'})+'\n')
  trace_code=trace.wait()
(root/'completed.json').write_text(json.dumps({'benchmark_exit':code,'trace_exit':trace_code})+'\n')
