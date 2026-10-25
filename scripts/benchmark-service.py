#!/usr/bin/env python3
"""Real TCP workload: child CPU/RSS, request tails, and independent diagnostic observers."""
import argparse, asyncio, json, os, pathlib, subprocess, sys, time
ROOT=pathlib.Path(__file__).resolve().parents[1]
def quantile(values,q):
    return sorted(values)[int((len(values)-1)*q)] if values else None
async def drive(meta,mode,seconds,subscribers,poll_ms):
    finish=time.perf_counter()+seconds
    latency=[]; query_latency=[]; failures=0; observations=0; unchanged=0; query_errors=0
    async def load():
        nonlocal failures
        reader,writer=await asyncio.open_connection('127.0.0.1',meta['port'])
        while time.perf_counter()<finish:
            start=time.perf_counter()
            try:
                writer.write(b'x'*64);await writer.drain()
                assert await asyncio.wait_for(reader.readexactly(64),1)==b'x'*64
                latency.append((time.perf_counter()-start)*1e6)
            except Exception:
                failures+=1;break
        writer.close();await writer.wait_closed()
    async def observe():
        nonlocal observations,unchanged,query_errors
        revision=None
        while time.perf_counter()<finish:
            start=time.perf_counter()
            try:
                reader,writer=await asyncio.open_unix_connection(meta['socket'])
                request={'min_completed':0} if mode=='manual' else {'command':'observe','sql':'SELECT completed, bytes FROM counters WHERE completed > 0','since':revision}
                writer.write(json.dumps(request).encode()+b'\n');await writer.drain()
                reply=json.loads(await asyncio.wait_for(reader.readline(),1))
                writer.close();await writer.wait_closed()
                if mode=='published':
                    if reply['status']=='error': raise RuntimeError(reply)
                    revision=reply['revision']
                    unchanged+=reply['status']=='not_modified'
                observations+=1;query_latency.append((time.perf_counter()-start)*1e6)
            except Exception: query_errors+=1
            await asyncio.sleep(poll_ms/1000)
    start=time.perf_counter()
    await asyncio.gather(*(load() for _ in range(4)),*(observe() for _ in range(subscribers)))
    elapsed=time.perf_counter()-start
    return dict(requests=len(latency),requests_per_second=len(latency)/elapsed,p50_us=quantile(latency,.5),p95_us=quantile(latency,.95),p99_us=quantile(latency,.99),request_errors=failures,observations=observations,not_modified=unchanged,observation_errors=query_errors,observation_p99_us=quantile(query_latency,.99))
def main():
    p=argparse.ArgumentParser();p.add_argument('--repeats',type=int,default=3);p.add_argument('--seconds',type=int,default=2);p.add_argument('--output',default='docs/service-benchmark.json');p.add_argument('--binary',default='experiments/workload/target/release/livequery-workload');p.add_argument('--only',choices=['baseline','soak']);args=p.parse_args()
    binary=ROOT/args.binary
    rows=[]
    for repeat in range(args.repeats):
        for engine in ['sync','async']:
            cases=[('baseline',0,10),('manual',1,10),('published',0,10),('published',1,10),('published',4,10),('published',4,50)]
            if args.only=='baseline': cases=[('baseline',0,10)]
            if args.only=='soak': cases=[('published',4,10)]
            # Rotate order to avoid always measuring baseline on a colder machine.
            cases=cases[repeat:]+cases[:repeat]
            for mode,subscribers,publish_ms in cases:
                proc=subprocess.Popen([str(binary),engine,mode,str(args.seconds+1),str(publish_ms)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                try:
                    meta=json.loads(proc.stdout.readline());time.sleep(.1)
                    metrics=asyncio.run(drive(meta,mode,args.seconds,subscribers,10))
                    _,status,usage=os.wait4(proc.pid,0);proc.returncode=os.waitstatus_to_exitcode(status)
                    if proc.returncode: raise RuntimeError(proc.stderr.read())
                    summary=json.loads(proc.stdout.readline())
                    row=dict(repeat=repeat,engine=engine,mode=mode,subscribers=subscribers,publish_ms=publish_ms,poll_ms=10,host_cpu_seconds=usage.ru_utime+usage.ru_stime,host_peak_rss_bytes=usage.ru_maxrss*(1 if sys.platform=='darwin' else 1024),**metrics,**summary)
                    rows.append(row);print(json.dumps(row),flush=True)
                finally:
                    if proc.poll() is None: proc.terminate();proc.wait()
    (ROOT/args.output).write_text(json.dumps({'platform':sys.platform,'binary':args.binary,'binary_bytes':binary.stat().st_size,'repeats':args.repeats,'load_seconds':args.seconds,'load_clients':4,'payload_bytes':64,'results':rows},indent=2)+'\n')
if __name__=='__main__': main()
