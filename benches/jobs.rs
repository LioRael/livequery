//! Reproducible task-table costs. Run with `cargo bench -p livequery-benchmarks --bench jobs`.
use livequery::{Inspect, Limits, Published, Query, Registry, Snapshot, Source, Table};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
struct CountingAllocator;
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
    static BYTES: Cell<u64> = const { Cell::new(0) };
}
// SAFETY: all allocation operations are delegated unchanged to System; counters are thread-local.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = TRACK.try_with(|track| {
            if track.get() {
                ALLOCS.with(|n| n.set(n.get() + 1));
                BYTES.with(|n| n.set(n.get() + layout.size() as u64));
            }
        });
        // SAFETY: the caller supplies a valid layout, forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the pointer and layout for this allocation.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
#[derive(Debug, Clone, Inspect)]
struct Job {
    id: u64,
    state: String,
    worker_id: u64,
    elapsed_ms: u64,
    retry_count: u64,
}
fn data(rows: usize, length: usize) -> Vec<Job> {
    (0..rows)
        .map(|i| Job {
            id: i as u64,
            state: "r".repeat(length),
            worker_id: i as u64 % 4,
            elapsed_ms: i as u64,
            retry_count: 0,
        })
        .collect()
}
fn measure<T>(name: &str, mut operation: impl FnMut() -> T) -> livequery::serde_json::Value {
    measure_prepared(name, || (), |_| operation())
}
fn measure_prepared<S, T>(
    name: &str,
    mut setup: impl FnMut() -> S,
    mut operation: impl FnMut(S) -> T,
) -> livequery::serde_json::Value {
    let mut nanos = 0;
    let mut allocations = 0;
    let mut bytes = 0;
    for _ in 0..20 {
        let input = setup();
        ALLOCS.with(|n| n.set(0));
        BYTES.with(|n| n.set(0));
        TRACK.with(|t| t.set(true));
        let start = Instant::now();
        let result = black_box(operation(input));
        nanos += start.elapsed().as_nanos();
        TRACK.with(|t| t.set(false));
        allocations += ALLOCS.with(Cell::get);
        bytes += BYTES.with(Cell::get);
        drop(result);
    }
    livequery::serde_json::json!({"operation":name,"mean_us":nanos as f64/20_000.0,"allocations":allocations/20,"allocated_bytes":bytes/20})
}
fn contention(
    rows: usize,
    copy_first: bool,
    publish_ms: u64,
    observers: usize,
) -> livequery::serde_json::Value {
    let state = Arc::new(RwLock::new(data(rows, 32)));
    let table = Table::from_shared(state.clone());
    let limits = Limits::default();
    let published = Published::new::<Job>(limits).unwrap();
    published.refresh(&table).unwrap();
    let registry = Registry::new();
    let _guard = registry
        .expose_published("jobs", published.clone())
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let publisher = {
        let stop = stop.clone();
        let state = state.clone();
        std::thread::spawn(move || {
            let mut errors = std::collections::BTreeMap::<String, u64>::new();
            if publish_ms == 0 {
                return errors;
            }
            while !stop.load(Ordering::Relaxed) {
                let result = if copy_first {
                    let copied = state.read().unwrap().clone();
                    Snapshot::capture(copied.iter(), limits)
                        .and_then(|snapshot| published.publish(snapshot))
                } else {
                    published.refresh(&table)
                };
                if let Err(error) = result {
                    *errors.entry(error.code().into()).or_default() += 1;
                }
                std::thread::sleep(Duration::from_millis(publish_ms));
            }
            errors
        })
    };
    let mut readers = Vec::new();
    for _ in 0..observers {
        let registry = registry.clone();
        let stop = stop.clone();
        readers.push(std::thread::spawn(move || {
            let query =
                Query::parse("SELECT id, elapsed_ms FROM jobs ORDER BY elapsed_ms DESC LIMIT 20")
                    .unwrap();
            let mut errors = std::collections::BTreeMap::<String, u64>::new();
            while !stop.load(Ordering::Relaxed) {
                if let Err(error) = query.execute(&registry, limits) {
                    *errors.entry(error.code().into()).or_default() += 1;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            errors
        }));
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let business = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_nodelay(true).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut byte = [0];
        while stream.read_exact(&mut byte).is_ok() {
            state.write().unwrap()[0].retry_count += 1;
            stream.write_all(&byte).unwrap();
        }
    });
    let mut client = TcpStream::connect(address).unwrap();
    client.set_nodelay(true).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut latency = Vec::new();
    while Instant::now() < deadline {
        let start = Instant::now();
        client.write_all(b"x").unwrap();
        let mut response = [0];
        client.read_exact(&mut response).unwrap();
        assert_eq!(response, *b"x");
        latency.push(start.elapsed().as_nanos() as u64);
        std::thread::sleep(Duration::from_micros(100));
    }
    drop(client);
    business.join().unwrap();
    stop.store(true, Ordering::Relaxed);
    let publication_errors = publisher.join().unwrap();
    let mut query_errors = std::collections::BTreeMap::<String, u64>::new();
    for reader in readers {
        for (code, count) in reader.join().unwrap() {
            *query_errors.entry(code).or_default() += count;
        }
    }
    latency.sort_unstable();
    livequery::serde_json::json!({"rows":rows,"copy_before_serialization":copy_first,"publish_ms":publish_ms,"observers":observers,"business_requests":latency.len(),"business_request_p99_us":latency[latency.len()*99/100] as f64/1000.0,"publication_errors":publication_errors,"query_errors":query_errors})
}
fn main() {
    let limits = Limits::default();
    let mut costs = Vec::new();
    for rows in [100, 1_000, 5_000] {
        for length in [16, 128, 256] {
            let state = Arc::new(RwLock::new(data(rows, length)));
            let table = Table::from_shared(state.clone());
            let published = Published::new::<Job>(limits).unwrap();
            if let Err(error) = published.refresh(&table) {
                costs.push(livequery::serde_json::json!({"rows":rows,"string_bytes":length,"rejected":error.code()}));
                continue;
            }
            let registry = Registry::new();
            let _guard = registry
                .expose_published("jobs", published.clone())
                .unwrap();
            let query =
                Query::parse("SELECT id, elapsed_ms FROM jobs ORDER BY elapsed_ms DESC LIMIT 20")
                    .unwrap();
            let metrics = vec![
                measure("capture_under_lock", || table.snapshot(limits).unwrap()),
                measure("copy_lock_hold", || state.read().unwrap().clone()),
                measure("owned_snapshot_read", || {
                    registry.snapshot("jobs", limits).unwrap()
                }),
                measure("shared_snapshot_read", || {
                    registry.read_snapshot("jobs", limits).unwrap()
                }),
                measure("projected_sorted_query", || {
                    query.execute(&registry, limits).unwrap()
                }),
                measure_prepared(
                    "publication_validation_and_swap",
                    || registry.snapshot("jobs", limits).unwrap(),
                    |snapshot| published.publish(snapshot).unwrap(),
                ),
            ];
            costs.push(
                livequery::serde_json::json!({"rows":rows,"string_bytes":length,"metrics":metrics}),
            );
        }
    }
    let mut contention_runs = Vec::new();
    for rows in [100, 1000] {
        contention_runs.push(contention(rows, false, 0, 0));
        for copy_first in [false, true] {
            for publish_ms in [10, 100] {
                for observers in [1, 4] {
                    contention_runs.push(contention(rows, copy_first, publish_ms, observers));
                }
            }
        }
    }
    println!(
        "{}",
        livequery::serde_json::json!({"micro_repetitions":20,"contention_seconds_per_case":1,"costs":costs,"contention":contention_runs})
    );
}
