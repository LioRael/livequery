use livequery::{Inspect,Column,Row,Error,Snapshot,Limits,Published};
use std::{sync::atomic::{AtomicU64,Ordering},time::Instant};
static INSPECT:AtomicU64=AtomicU64::new(0);
#[derive(Inspect)]
struct Job {id:u64,state:String,worker_id:u64,elapsed_ms:u64,retry_count:u64,detail:String}
struct Timed(Job);
impl Inspect for Timed {
fn schema()->Vec<Column>{Job::schema()}
fn inspect(&self)->Result<Row,Error>{self.inspect_bounded(Limits::default().bytes)}
fn inspect_bounded(&self,n:usize)->Result<Row,Error>{let t=Instant::now();let r=self.0.inspect_bounded(n);INSPECT.fetch_add(t.elapsed().as_nanos() as u64,Ordering::Relaxed);r}
}
fn main()->Result<(),Box<dyn std::error::Error>>{
let jobs:Vec<_>=(0..5000).map(|id|Timed(Job{id,state:"running".into(),worker_id:id%4,elapsed_ms:id,retry_count:0,detail:"x".repeat(96)})).collect();
let limits=Limits::default();let published=Published::from_snapshot(Snapshot::capture(jobs.iter(),limits)?,limits)?;
let mut rows=vec![];
for _ in 0..500 {INSPECT.store(0,Ordering::Relaxed);let t=Instant::now();let s=Snapshot::capture(jobs.iter(),limits);let capture=t.elapsed().as_nanos();let inspect=INSPECT.load(Ordering::Relaxed);let t=Instant::now();let outcome=s.and_then(|s|published.publish(s));rows.push(serde_json::json!({"capture_ns":capture,"inspect_ns":inspect,"publish_ns":t.elapsed().as_nanos(),"error":outcome.err().map(|e|e.code())}));}
println!("{}",serde_json::to_string(&rows)?);Ok(())}
