import pathlib,json,sys,bisect,collections
root=pathlib.Path(sys.argv[1]);frames=[json.loads(x) for x in (root/'threads.jsonl').read_text().splitlines()];frames=[f for f in frames if 'task_counters' in f];stamps=[f['monotonic_ns'] for f in frames];spans=[]
for f in (root/'events').glob('*.jsonl'):
 for line in f.read_text().splitlines():
  e=json.loads(line)
  if 'points' not in e:continue
  ps=e['points'];end=next((p for p in ps if p['phase']=='deadline'),ps[-1]);start=ps[0]
  intervals=[]
  for a,b in zip(ps,ps[1:]):
   if a['phase']=='deadline':break
   wall=b['wall_ns']-a['wall_ns']
   if wall<10_000_000:continue
   lo=bisect.bisect_left(stamps,a['wall_ns']);hi=bisect.bisect_right(stamps,b['wall_ns']);samples=[]
   for frame in frames[lo:hi]:
    for t in frame['threads']:
     if t['tid']==e['tid']:samples.append({'stamp':frame['monotonic_ns'],**t})
   deltas=None;bracket=None
   # Extra earlier sample guards against counters being read just after a stamp.
   if lo>=2 and hi<len(frames):
    left=frames[lo-2];right=frames[hi];x=left['task_counters'];y=right['task_counters']
    if 'faults' in x and 'faults' in y:
     deltas={k:y[k]-x[k] for k in ['faults','pageins','cow_faults','csw']};bracket=[left['monotonic_ns'],right['monotonic_ns']]
   intervals.append({'phase':a['phase']+' -> '+b['phase'],'start_ns':a['wall_ns'],'end_ns':b['wall_ns'],'wall_ms':wall/1e6,'cpu_ms':(b['cpu_ns']-a['cpu_ns'])/1e6,'allocator_ms':(b['allocator']['wall_ns']-a['allocator']['wall_ns'])/1e6,'thread_samples':samples,'task_counter_delta':deltas,'counter_bracket':bracket})
  spans.append({'kind':e['kind'],'deadline':e['deadline'],'tid':e['tid'],'pre_return_wall_ms':(end['wall_ns']-start['wall_ns'])/1e6,'pre_return_cpu_ms':(end['cpu_ns']-start['cpu_ns'])/1e6,'intervals':intervals})
host=[json.loads(line) for line in (root/'host-activity.jsonl').read_text().splitlines()]
for span in spans:
 for interval in span['intervals']:
  interval['host_cpu_windows']=[{'start_ns': h['interval_start_ns'], 'end_ns': h['monotonic_ns'], 'busy_percent': h['interval_busy_percent']} for h in host if 'interval_start_ns' in h and h['interval_start_ns'] < interval['end_ns'] and h['monotonic_ns'] > interval['start_ns']]
r={'spans':spans,'counts':dict(collections.Counter(str((s['kind'],s['deadline'])) for s in spans)),'limitations':'Task counters cover all threads and an expanded time bracket. Positive faults cannot be attributed to measured thread. State snapshots can miss intervening waits; state 1 is not proof of running on CPU. Post-Deadline cleanup excluded.'}
(root/'failure-correlation.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps({'counts':r['counts'],'file':str(root/'failure-correlation.json')}))
