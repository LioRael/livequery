import pathlib,subprocess,os,time,json,hashlib,difflib
base=pathlib.Path('target/offcpu-diagnostic');root=base/'allocation-natural';(root/'events').mkdir(parents=True,exist_ok=False)
src=base/'source';patch=[]
for rel in ['crates/livequery-core/Cargo.toml','crates/livequery-core/src/lib.rs','crates/livequery-core/src/diagnostic.rs','crates/livequery-core/src/allocation_probe.rs','crates/livequery-core/src/state.rs','crates/livequery-query/src/lib.rs','crates/livequery/src/lib.rs','experiments/workload/src/main.rs','experiments/workload/Cargo.lock']:
 p=pathlib.Path(rel);old=p.read_text() if p.exists() else '';patch.extend(difflib.unified_diff(old.splitlines(True),(src/rel).read_text().splitlines(True),fromfile='a/'+rel,tofile='b/'+rel))
(root/'instrumentation.patch').write_text(''.join(patch));binary=src/'experiments/workload/target/release/livequery-workload'
(root/'provenance.json').write_text(json.dumps({'base_source':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'diagnostic_only':True,'seconds':120,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'patch_sha256':hashlib.sha256((root/'instrumentation.patch').read_bytes()).hexdigest(),'limitations':'Allocator wall time includes descheduling inside allocator. Two clocks per allocator call add overhead; not acceptance. No external recorder.'},indent=2)+'\n')
env=os.environ.copy();env.pop('LQ_VALIDATE_PAUSE',None);env['LQ_DIAGNOSTIC_DIR']=str((root/'events').resolve())
with (root/'run.log').open('w') as log,(root/'host-activity.jsonl').open('w') as host:
 p=subprocess.Popen(['python3','scripts/benchmark-service.py','--binary',str(binary),'--task-rows','5000','--string-bytes','96','--engine','async','--seconds','120','--repeats','1','--output',str(root/'report.json')],env=env,stdout=log,stderr=log)
 while p.poll() is None:
  # Names and CPU only; do not capture unrelated process arguments or secrets.
  sample=subprocess.run(['ps','-axo','pid=,pcpu=,comm='],capture_output=True,text=True).stdout.splitlines()
  busy=[]
  for line in sample:
   parts=line.strip().split(None,2)
   if len(parts)==3 and (float(parts[1])>=5 or 'xctrace' in parts[2]):busy.append(line.strip())
  host.write(json.dumps({'unix_ns':time.time_ns(),'loadavg':os.getloadavg(),'busy_processes':busy})+'\n');host.flush();time.sleep(5)
(root/'completed.json').write_text(json.dumps({'exit':p.returncode})+'\n')
print('completed',p.returncode)
