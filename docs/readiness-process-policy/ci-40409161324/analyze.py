import json, pathlib, sys, bisect, collections
root=pathlib.Path(sys.argv[1])
f=[json.loads(x) for x in (root/'threads.jsonl').read_text().splitlines()]
f=[x for x in f if x.get('sample_end_ns')]
stamps=[x['monotonic_ns'] for x in f]
c=json.loads((root/'failure-correlation.json').read_text())
result=[]
for span in c['spans']:
 if not span['deadline']:continue
 for interval in span['intervals']:
  a,b=interval['start_ns'],interval['end_ns'];pairs=[]
  for x,y in zip(f,f[1:]):
   if x['monotonic_ns']<a or y['sample_end_ns']>b:continue
   tx=next((t for t in x['threads'] if t['tid']==span['tid'] and 'cpu_ns' in t),None)
   ty=next((t for t in y['threads'] if t['tid']==span['tid'] and 'cpu_ns' in t),None)
   if tx is None or ty is None:continue
   keys=['total_user_raw','total_system_raw','syscalls_mach','syscalls_unix','csw','faults','pageins','cow_faults']
   delta={k:y['task_counters'][k]-x['task_counters'][k] for k in keys}
   pairs.append({'start_ns':x['monotonic_ns'],'end_ns':y['sample_end_ns'],'gap_ms':(y['monotonic_ns']-x['monotonic_ns'])/1e6,'thread_cpu_delta_ns':ty['cpu_ns']-tx['cpu_ns'],'states':[tx['state'],ty['state']],'priorities':[tx['priority'],ty['priority']],'observer_cpu_delta_ns':y['observer_cpu_ns']-x['observer_cpu_ns'],'process_delta':delta})
  result.append({'kind':span['kind'],'tid':span['tid'],'phase':interval['phase'],'wall_ms':interval['wall_ms'],'cpu_ms':interval['cpu_ms'],'pairs':pairs})
summary={'failure_intervals':result,'samples':len(f),'max_observer_gap_ms':max((y['monotonic_ns']-x['monotonic_ns'])/1e6 for x,y in zip(f,f[1:])) if len(f)>1 else None,'observer_cpu_ms':(f[-1]['observer_cpu_ns']-f[0]['observer_cpu_ns'])/1e6 if f else None,'limitations':'Adjacent sample pairs only; waiting between samples is possible. Process-wide CPU and syscall deltas do not identify a thread. Zero deltas require no time-unit assumption. Observer gaps do not by themselves prove VM suspension.'}
(root/'native-correlation.json').write_text(json.dumps(summary,indent=2)+'\n')
pairs=[p for i in result for p in i['pairs']]
print(json.dumps({'intervals':len(result),'pairs':len(pairs),'states':dict(collections.Counter(str(p['states']) for p in pairs)),'runnable_zero_cpu_pairs':sum(p['states']==[1,1] and p['thread_cpu_delta_ns']==0 for p in pairs),'target_stalled_other_cpu_progress_pairs':sum(p['states']==[1,1] and p['thread_cpu_delta_ns']==0 and (p['process_delta']['total_user_raw']+p['process_delta']['total_system_raw'])>0 for p in pairs),'observer_cpu_ms':summary['observer_cpu_ms'],'max_observer_gap_ms':summary['max_observer_gap_ms']}))
