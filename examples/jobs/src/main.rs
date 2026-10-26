// Controlled stuck-worker fixture. Business commands use stdin; LiveQuery stays read-only.
use livequery::{Inspect, Limits, PublishedHandle, Registry, Table, local::RunningAgent};
use std::{
    io::{self, BufRead, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Inspect)]
struct Job {
    id: u64,
    state: String,
    worker_id: u64,
    elapsed_ms: u64,
    retry_count: u64,
}
#[derive(Debug)]
struct Scheduler {
    jobs: Vec<(Job, Instant)>,
    next_id: u64,
    stalled: Option<u64>,
}
impl Scheduler {
    fn submit(&mut self) -> bool {
        self.jobs.retain(|(job, _)| job.state != "done");
        if self.jobs.len() >= 128 {
            return false;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.push((
            Job {
                id,
                state: "running".into(),
                worker_id: id % 3 + 1,
                elapsed_ms: 0,
                retry_count: 0,
            },
            Instant::now(),
        ));
        true
    }
    fn tick(&mut self) -> Vec<Job> {
        for (job, started) in &mut self.jobs {
            if job.state == "running" {
                job.elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                if self.stalled != Some(job.worker_id) && job.elapsed_ms >= 250 {
                    job.state = "done".into();
                }
            }
        }
        self.jobs.iter().map(|(job, _)| job.clone()).collect()
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = Limits::default();
    let registry = Registry::new();
    let handle = PublishedHandle::register::<Job>(&registry, "jobs", limits)?;
    let socket = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            livequery::local::directory().join(format!("{}.sock", std::process::id()))
        });
    let agent = RunningAgent::start(&socket, registry, limits)?;
    let state = Arc::new(Mutex::new(Scheduler {
        jobs: Vec::new(),
        next_id: 1,
        stalled: Some(2),
    }));
    for _ in 0..12 {
        state.lock().unwrap().submit();
    }
    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let state = state.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // Copy a small diagnostic view under the business lock, serialize after release.
                let rows = state.lock().expect("scheduler poisoned").tick();
                if let Err(error) = handle.refresh(&Table::new(rows)) {
                    eprintln!("diagnostic refresh failed (old snapshot retained): {error}");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
    };
    println!(
        "{}",
        livequery::serde_json::json!({"pid":std::process::id(),"socket":socket,"stalled_worker":2})
    );
    io::stdout().flush()?;
    let input_result = (|| -> io::Result<()> {
        for line in io::stdin().lock().lines() {
            let line = line?;
            match line.trim() {
                "submit" => println!(
                    "{}",
                    if state.lock().unwrap().submit() {
                        "accepted"
                    } else {
                        "queue full"
                    }
                ),
                "repair 2" => {
                    let mut scheduler = state.lock().unwrap();
                    scheduler.stalled = None;
                    for (job, _) in &mut scheduler.jobs {
                        if job.worker_id == 2 && job.state == "running" {
                            job.retry_count += 1;
                        }
                    }
                    println!("worker 2 repaired");
                }
                "stall 2" => {
                    state.lock().unwrap().stalled = Some(2);
                    println!("worker 2 stalled");
                }
                "quit" => break,
                _ => println!("commands: submit | repair 2 | stall 2 | quit"),
            }
            io::stdout().flush()?;
        }
        Ok(())
    })();
    stop.store(true, Ordering::Relaxed);
    worker.join().map_err(|_| "scheduler thread panicked")?;
    agent.stop();
    agent.wait()?;
    input_result?;
    Ok(())
}
