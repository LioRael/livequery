"""Diagnostic subprocess guard. Polling limits can overshoot; not an OS quota."""
import argparse,json,os,pathlib,shutil,signal,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--root',required=True);p.add_argument('--max-bytes',type=int,default=134217728);p.add_argument('--max-seconds',type=float,default=30);p.add_argument('--min-free-bytes',type=int,default=8589934592);p.add_argument('command',nargs=argparse.REMAINDER);a=p.parse_args()
assert a.max_bytes>0 and 0<a.max_seconds<=60 and a.min_free_bytes>=0
cmd=a.command[1:] if a.command[:1]==['--'] else a.command
assert cmd
root=pathlib.Path(a.root).resolve();root.mkdir(parents=True,exist_ok=False);tmp=root/'tmp';tmp.mkdir();env=os.environ.copy();env['TMPDIR']=str(tmp)+'/'
def used():
 total=0
 for base,dirs,files in os.walk(root,followlinks=False):
  for name in files:
   try:total+=(pathlib.Path(base)/name).lstat().st_size
   except FileNotFoundError:pass
 return total
started=time.monotonic();reason=None;peak=0;proc=None
try:
 with (root/'output.log').open('wb') as log:
  if shutil.disk_usage(root).free<a.min_free_bytes:raise RuntimeError('insufficient free space before launch')
  proc=subprocess.Popen(cmd,stdout=log,stderr=log,env=env,start_new_session=True)
  while True:
   size=used();peak=max(peak,size);free=shutil.disk_usage(root).free;elapsed=time.monotonic()-started
   if size>a.max_bytes:reason='artifact_bytes'
   elif free<a.min_free_bytes:reason='free_space'
   elif elapsed>a.max_seconds:reason='total_duration_including_finalization'
   if reason:
    # Dedicated process group, created here; stop writes immediately on a breached limit.
    try:os.killpg(proc.pid,signal.SIGKILL)
    except ProcessLookupError:pass
    proc.wait();break
   if proc.poll() is not None:break
   time.sleep(.05)
finally:
 if proc is not None:
  # Do not leave a recorder helper alive after its parent exits.
  try:os.killpg(proc.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  code=proc.wait()
 else:code=None
 result={'exit_code':code,'stop_reason':reason,'elapsed_seconds':time.monotonic()-started,'peak_observed_bytes':peak,'final_bytes':used(),'private_tmpdir':str(tmp),'limits':{'bytes':a.max_bytes,'seconds':a.max_seconds,'min_free_bytes':a.min_free_bytes},'limitations':'50 ms polling can overshoot; only files under root counted. Recorder must be verified to honor TMPDIR. Process groups do not constrain helpers that deliberately detach.'}
 (root/'guard-result.json').write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result))
