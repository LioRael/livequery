"""Read process-wide fault and context-switch counters; never per-thread attribution."""
import ctypes, json, os, mmap, time
class TaskInfo(ctypes.Structure):
    _fields_=[(k,ctypes.c_uint64) for k in ['virtual_size','resident_size','total_user','total_system','threads_user','threads_system']]+[(k,ctypes.c_int32) for k in ['policy','faults','pageins','cow_faults','messages_sent','messages_received','syscalls_mach','syscalls_unix','csw','threadnum','numrunning','priority']]
lib=ctypes.CDLL('/usr/lib/libproc.dylib',use_errno=True)
lib.proc_pidinfo.argtypes=[ctypes.c_int,ctypes.c_int,ctypes.c_uint64,ctypes.c_void_p,ctypes.c_int];lib.proc_pidinfo.restype=ctypes.c_int
assert ctypes.sizeof(TaskInfo)==96
def read(pid):
    v=TaskInfo();n=lib.proc_pidinfo(pid,4,0,ctypes.byref(v),ctypes.sizeof(v))
    if n!=ctypes.sizeof(v):return {'error':ctypes.get_errno(),'returned_bytes':n}
    return {k:getattr(v,k) for k in ['faults','pageins','cow_faults','csw','total_user','total_system','threadnum','numrunning']}
if __name__=='__main__':
    read(os.getpid())
    before=read(os.getpid());m=mmap.mmap(-1,16*1024*1024)
    for offset in range(0,len(m),mmap.PAGESIZE):m[offset]=1
    after=read(os.getpid());m.close()
    assert 'faults' in before and after['faults']>before['faults']
    start=time.perf_counter_ns()
    for _ in range(1000):read(os.getpid())
    elapsed=time.perf_counter_ns()-start
    print(json.dumps({'struct_bytes':ctypes.sizeof(TaskInfo),'before':before,'after':after,'delta':{k:after[k]-before[k] for k in before},'calls':1000,'mean_read_ns':elapsed/1000,'limitation':'mmap touch validates fault-counter sensitivity, not disk-pagein detection or kernel wait duration. Counters are process-wide; positive changes cannot identify the affected thread.'},indent=2))
