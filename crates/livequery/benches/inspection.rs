use livequery::{Inspect, Limits, Query, Registry, Table, Watch};
use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Debug, Inspect)]
struct Player {
    id: u64,
    health: u64,
    name: String,
}
fn rows(n: usize) -> Vec<Player> {
    (0..n)
        .map(|id| Player {
            id: id as u64,
            health: 50,
            name: "Ada".into(),
        })
        .collect()
}
fn percentile(samples: &mut [u128], percent: usize) -> u128 {
    samples.sort_unstable();
    samples[(samples.len() - 1) * percent / 100]
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "kind,rows,subscribers,poll_hz,p50_us,p99_us,operations_per_second,busy,observations,observation_errors"
    );
    for n in [100, 1_000, 10_000] {
        let r = Registry::new();
        let _guard = r.expose("players", Table::new(rows(n)))?;
        let q = Query::parse("SELECT id, health FROM players WHERE health < 60 LIMIT 100")?;
        let mut samples = Vec::new();
        for _ in 0..30 {
            let start = Instant::now();
            black_box(q.execute(&r, Limits::default())?);
            samples.push(start.elapsed().as_micros());
        }
        let p50 = percentile(&mut samples, 50);
        let p99 = percentile(&mut samples, 99);
        println!("query,{n},0,0,{p50},{p99},0,0,0,0");
    }
    // The raw lock baseline uses the same synchronization and data; only registration differs.
    for update_hz in [0, 100, 1000] {
        for subscribers in [0, 1, 8, 32] {
            for hz in [10, 100] {
                let table = Table::new(rows(100));
                let r = Registry::new();
                let _guard = r.expose("players", table.clone())?;
                let stop = Arc::new(AtomicBool::new(false));
                let reader = {
                    let stop = stop.clone();
                    std::thread::spawn(move || {
                        let mut watches: Vec<_> = (0..subscribers)
                            .map(|_| {
                                Watch::new(
                                    Query::parse("SELECT * FROM players")
                                        .expect("benchmark SQL is valid"),
                                )
                            })
                            .collect();
                        let mut observations = 0_u64;
                        let mut errors = 0_u64;
                        while !stop.load(Ordering::Relaxed) {
                            for watch in &mut watches {
                                match black_box(watch.poll(&r, Limits::default())) {
                                    Ok(_) => observations += 1,
                                    Err(_) => errors += 1,
                                }
                            }
                            std::thread::sleep(Duration::from_micros(1_000_000 / hz));
                        }
                        (observations, errors)
                    })
                };
                let start = Instant::now();
                let duration = Duration::from_millis(400);
                let mut count = 0;
                let mut busy = 0;
                let mut samples = Vec::new();
                while start.elapsed() < duration {
                    let update_start = Instant::now();
                    match table.update(|r| {
                        r[0].health = r[0].health.wrapping_add(1);
                    }) {
                        Ok(()) => count += 1,
                        Err(_) => busy += 1,
                    }
                    if update_hz > 0 || (count + busy) % 1000 == 0 {
                        samples.push(update_start.elapsed().as_nanos());
                    }
                    if let Some(micros) = 1_000_000_u64.checked_div(update_hz) {
                        std::thread::sleep(Duration::from_micros(micros));
                    }
                }
                let elapsed = start.elapsed().as_secs_f64();
                stop.store(true, Ordering::Relaxed);
                let (observations, errors) = reader.join().expect("benchmark reader completes");
                let p50 = percentile(&mut samples, 50) as f64 / 1000.;
                let p99 = percentile(&mut samples, 99) as f64 / 1000.;
                println!(
                    "watch_update_{update_hz},100,{subscribers},{hz},{p50:.3},{p99:.3},{:.0},{busy},{observations},{errors}",
                    count as f64 / elapsed
                );
            }
        }
    }
    let raw = std::sync::RwLock::new(rows(100));
    let start = Instant::now();
    for _ in 0..1_000_000 {
        let mut rows = raw
            .try_write()
            .expect("single-threaded benchmark lock is free");
        rows[0].health = black_box(rows[0].health.wrapping_add(1));
    }
    println!(
        "no_livequery,100,0,0,0,0,{:.0},0,0,0",
        1_000_000. / start.elapsed().as_secs_f64()
    );
    for registered in [false, true] {
        let table = Table::new(rows(100));
        let registry = Registry::new();
        let _guard = if registered {
            Some(registry.expose("players", table.clone())?)
        } else {
            None
        };
        let start = Instant::now();
        for _ in 0..1_000_000 {
            table.update(|r| {
                r[0].health = black_box(r[0].health.wrapping_add(1));
            })?;
        }
        println!(
            "idle_{registered},100,0,0,0,0,{:.0},0,0,0",
            1_000_000. / start.elapsed().as_secs_f64()
        );
    }
    Ok(())
}
