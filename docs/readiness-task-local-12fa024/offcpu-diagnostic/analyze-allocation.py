import pathlib,json,sys
root=pathlib.Path(sys.argv[1]);spans=[];dropped=0
for f in (root/'events').glob('*.jsonl'):
 for line in f.read_text().splitlines():
  e=json.loads(line)
  if 'diagnostic_dropped_events' in e:dropped+=e['diagnostic_dropped_events'];continue
  intervals=[]
  for a,b in zip(e['points'],e['points'][1:]):
   if a['wall_ns'] is None or b['wall_ns'] is None:continue
   wall=b['wall_ns']-a['wall_ns'];alloc=b['allocator']['wall_ns']-a['allocator']['wall_ns']
   intervals.append({'phase':a['phase']+' -> '+b['phase'],'wall_ms':wall/1e6,'cpu_ms':None if a['cpu_ns'] is None or b['cpu_ns'] is None else (b['cpu_ns']-a['cpu_ns'])/1e6,'allocator_wall_ms':alloc/1e6,'allocator_calls':b['allocator']['calls']-a['allocator']['calls'],'slow_allocator_calls':b['allocator']['slow_calls']-a['allocator']['slow_calls']})
  spans.append({'kind':e['kind'],'tid':e['tid'],'deadline':e['deadline'],'wall_ms':e['wall_ns']/1e6,'intervals':sorted(intervals,key=lambda i:i['wall_ms'],reverse=True)[:3]})
report={'spans':spans,'dropped_events':dropped,'limitations':'Allocator wall time includes time descheduled while within System allocator. Positive allocator time is not proof of allocator lock contention. CPU is thread CPU. Unrecorded fast spans are not summarized.'}
(root/'analysis.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
