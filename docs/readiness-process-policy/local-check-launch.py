import pathlib,subprocess,tempfile,plistlib,os,time,json,uuid
root=pathlib.Path('target/policy-review').resolve();probe=str(root/'read-policy')
record={'outside':subprocess.check_output([probe],text=True)}
label='org.livequery.policy-check.'+uuid.uuid4().hex;domain=f'gui/{os.getuid()}'
with tempfile.TemporaryDirectory(prefix='livequery-policy-check-') as d:
 d=pathlib.Path(d);o=d/'output';e=d/'error';p=d/'job.plist'
 p.write_bytes(plistlib.dumps({'Label':label,'ProgramArguments':[probe],'ProcessType':'Interactive','RunAtLoad':True,'StandardOutPath':str(o),'StandardErrorPath':str(e)}))
 try:
  r=subprocess.run(['launchctl','bootstrap',domain,str(p)],capture_output=True,text=True);record['bootstrap_exit']=r.returncode;record['bootstrap_error']=r.stderr;r.check_returncode()
  end=time.monotonic()+5
  while time.monotonic()<end:
   if o.exists() and len(o.read_text().splitlines())==2:break
   time.sleep(.05)
  record['inside']=o.read_text() if o.exists() else None
 finally:
  r=subprocess.run(['launchctl','bootout',domain+'/'+label],capture_output=True,text=True);record['cleanup_exit']=r.returncode
  record['absent']=subprocess.run(['launchctl','print',domain+'/'+label],capture_output=True).returncode!=0
  (root/'launch-check.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
