import pathlib,subprocess,time,os,signal,json
r=pathlib.Path('target/offcpu-diagnostic/injection-id64');(r/'events').mkdir(parents=True,exist_ok=True)
env=os.environ.copy();env['LQ_DIAGNOSTIC_DIR']=str((r/'events').resolve());env['LQ_VALIDATE_PAUSE']='1'
with (r/'process.log').open('w') as log:
 p=subprocess.Popen(['target/offcpu-diagnostic/source/experiments/workload/target/release/livequery-workload','async','jobs','3','10','5000','96'],env=env,stdout=log,stderr=log)
 try:
  for _ in range(100):
   status=subprocess.check_output(['ps','-p',str(p.pid),'-o','stat='],text=True).strip()
   if 'T' in status:break
   time.sleep(.01)
  assert 'T' in status,'injected stop was not observed'
  s=subprocess.Popen(['python3','target/offcpu-diagnostic/thread-sampler.py',str(p.pid),'--seconds','3','--identities',str(r/'events'),'--output',str(r/'threads.jsonl')],stdout=subprocess.DEVNULL)
  end=time.time()+3
  while time.time()<end:
   file=r/'threads.jsonl'
   if file.exists() and '"threads"' in file.read_text():break
   time.sleep(.01)
  else:raise RuntimeError('sampler did not become ready')
  time.sleep(.1)
 finally:os.kill(p.pid,signal.SIGCONT)
 code=p.wait(timeout=10);s.wait(timeout=5)
 (r/'validation.json').write_text(json.dumps({'intentional_injection_not_root_cause_evidence':True,'observed_stopped':True,'exit_code':code})+'\n')
print('controlled injection completed')
