import pathlib,json,collections,sys
root=pathlib.Path(sys.argv[1]);by_tid=collections.defaultdict(list)
for line in (root/'threads.jsonl').open():
 s=json.loads(line)
 for t in s.get('threads',[]):
  if 'error' not in t:by_tid[t['tid']].append((s['monotonic_ns'],t))
out=[]
for path in (root/'events').glob('*.jsonl'):
 for line in path.open():
  e=json.loads(line)
  if 'points' not in e:continue
  points=e['points'];pairs=list(zip(points,points[1:]));a,b=max(pairs,key=lambda pair:pair[1]['wall_ns']-pair[0]['wall_ns'])
  samples=[(stamp,t) for stamp,t in by_tid[e['tid']] if a['wall_ns']<=stamp<=b['wall_ns']]
  out.append({'kind':e['kind'],'tid':e['tid'],'deadline':e['deadline'],'wall_ms':e['wall_ns']/1e6,'largest_gap_from':a['phase'],'largest_gap_to':b['phase'],'row_from':a['row'],'row_to':b['row'],'gap_wall_ms':(b['wall_ns']-a['wall_ns'])/1e6,'gap_cpu_ms':(b['cpu_ns']-a['cpu_ns'])/1e6 if a['cpu_ns'] is not None and b['cpu_ns'] is not None else None,'thread_samples':len(samples),'states':dict(collections.Counter(t['state'] for _,t in samples)),'max_observer_gap_ms':max((y[0]-x[0])/1e6 for x,y in zip(samples,samples[1:])) if len(samples)>1 else None})
(root/'correlation.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out[:10],indent=2));print('events',len(out))
