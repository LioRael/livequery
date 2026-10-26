"""Read-only macOS per-thread sampling. Diagnostic snapshots, not scheduler traces."""
import argparse, ctypes, json, time, pathlib, io
p=argparse.ArgumentParser();p.add_argument('pid',type=int);p.add_argument('--seconds',type=float,default=60);p.add_argument('--interval',type=float,default=.005);p.add_argument('--identities');p.add_argument('--output',required=True);a=p.parse_args()
assert 0 < a.seconds <= 600 and a.interval >= .001
class ThreadInfo(ctypes.Structure):
    _fields_=[('user_ns',ctypes.c_uint64),('system_ns',ctypes.c_uint64)]+[(k,ctypes.c_int32) for k in ['cpu_usage','policy','state','flags','sleep_seconds','current_priority','priority','max_priority']]+[('name',ctypes.c_char*64)]
lib=ctypes.CDLL('/usr/lib/libproc.dylib',use_errno=True)
lib.proc_pidinfo.argtypes=[ctypes.c_int,ctypes.c_int,ctypes.c_uint64,ctypes.c_void_p,ctypes.c_int];lib.proc_pidinfo.restype=ctypes.c_int
class Timespec(ctypes.Structure):
    _fields_=[('seconds',ctypes.c_long),('nanoseconds',ctypes.c_long)]
clocklib=ctypes.CDLL('/usr/lib/libSystem.B.dylib',use_errno=True)
clocklib.clock_gettime.argtypes=[ctypes.c_int,ctypes.POINTER(Timespec)];clocklib.clock_gettime.restype=ctypes.c_int
def monotonic_ns():
    value=Timespec()
    if clocklib.clock_gettime(6,ctypes.byref(value)) != 0:raise OSError(ctypes.get_errno(),'CLOCK_MONOTONIC failed')
    return value.seconds*1_000_000_000+value.nanoseconds
ids=(ctypes.c_uint64*4096)();start=time.monotonic();samples=0
with io.StringIO() as f:
    f.write(json.dumps({'pid':a.pid,'interval_seconds':a.interval,'started_unix_ns':time.time_ns(),'struct_bytes':ctypes.sizeof(ThreadInfo),'note':'state=1 is running normally, not proof of being scheduled on CPU; observer can also be delayed'})+'\n')
    while time.monotonic()-start<a.seconds:
        stamp=monotonic_ns();n=lib.proc_pidinfo(a.pid,6,0,ids,ctypes.sizeof(ids))
        if n<=0:
            f.write(json.dumps({'monotonic_ns':stamp,'list_error':ctypes.get_errno()})+'\n');break
        rows=[]
        selected=[int(f.stem.split('-')[1]) for f in pathlib.Path(a.identities).glob(str(a.pid)+'-*.tid')] if a.identities else ids[:n//8]
        for tid in selected:
            info=ThreadInfo();got=lib.proc_pidinfo(a.pid,15 if a.identities else 5,tid,ctypes.byref(info),ctypes.sizeof(info))
            if got!=ctypes.sizeof(info):rows.append({'tid':tid,'error':ctypes.get_errno(),'returned_bytes':got});continue
            rows.append({'tid':tid,'cpu_ns':info.user_ns+info.system_ns,'state':info.state,'priority':info.current_priority,'name':info.name.decode(errors='replace')})
        f.write(json.dumps({'monotonic_ns':stamp,'threads':rows})+'\n');samples+=1;time.sleep(a.interval)
    pathlib.Path(a.output).write_text(f.getvalue())
print(json.dumps({'samples':samples,'elapsed_seconds':time.monotonic()-start}))
