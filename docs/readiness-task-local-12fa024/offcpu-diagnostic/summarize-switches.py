"""Correlate recorded thread transitions; realtime alignment has sub-ms TOC rounding uncertainty."""
import json,pathlib,sys,xml.etree.ElementTree as E,datetime,collections
p=pathlib.Path(sys.argv[1]);a=min(json.load(open(p/'clock-anchor.json')),key=lambda x:x['monotonic_after_ns']-x['monotonic_before_ns']);offset=a['realtime_ns']-(a['monotonic_before_ns']+a['monotonic_after_ns'])//2
start=datetime.datetime.fromisoformat(E.parse(p/'toc.xml').findtext('.//start-date')).timestamp()*1e9
calls=[]
for f in (p/'events').glob('*.jsonl'):
 for l in f.read_text().splitlines():
  x=json.loads(l)
  if 'points' not in x:continue
  x['trace_start_ns']=x['points'][0]['wall_ns']+offset-start;x['trace_end_ns']=x['points'][-1]['wall_ns']+offset-start;calls.append(x)
tids={x['tid'] for x in calls};refs={};events=collections.defaultdict(list)
for _,e in E.iterparse(p/'switches.xml',events=['end']):
 if e.tag!='row':continue
 for c in e.iter():
  if 'id' in c.attrib:
   refs[c.attrib['id']]={'text':c.text,'fmt':c.get('fmt'),'tid':c.findtext('tid')}
 def val(c):return refs.get(c.get('ref'),{}) if c.get('ref') else {'text':c.text,'fmt':c.get('fmt'),'tid':c.findtext('tid')}
 c=list(e)
 if len(c)>=3:
  t=val(c[1]).get('tid')
  if t and int(t) in tids:events[int(t)].append((int(val(c[0])['text']),val(c[2])['text']))
 e.clear()
out=[]
for c in calls:
 counts=collections.defaultdict(float);seq=events[c['tid']];parts=[]
 for (t,state),(u,_) in zip(seq,seq[1:]):
  lo=max(t,c['trace_start_ns']);hi=min(u,c['trace_end_ns'])
  if hi>lo:counts[state]+=(hi-lo)/1e6;parts.append({'state':state,'ms':(hi-lo)/1e6,'trace_start_ns':t})
 out.append({'kind':c['kind'],'tid':c['tid'],'deadline':c['deadline'],'wall_ms':c['wall_ns']/1e6,'cpu_ms':(c['points'][-1]['cpu_ns']-c['points'][0]['cpu_ns'])/1e6,'state_ms':dict(counts),'largest_intervals':sorted(parts,key=lambda x:x['ms'],reverse=True)[:5]})
(p/'switch-correlation.json').write_text(json.dumps({'alignment_note':'CLOCK_REALTIME bridge, TOC start rounded to milliseconds; no assumption of zero clock adjustment. Boundary precision approximately 1ms. Check trace loss before interpreting unaccounted time.','calls':out},indent=2)+'\n')
print(json.dumps(out[:4],indent=2));print('calls',len(out))
