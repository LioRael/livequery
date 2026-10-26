use serde::Serialize;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Serialize, Default)]
#[cfg_attr(feature = "inspection", derive(livequery::Inspect))]
struct Counters {
    completed: u64,
    bytes: u64,
    #[serde(skip)]
    #[cfg_attr(feature = "inspection", inspect(skip))]
    jobs: Vec<Job>,
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "inspection", derive(livequery::Inspect))]
struct Job {
    id: u64,
    state: String,
    worker_id: u64,
    elapsed_ms: u64,
    retry_count: u64,
    detail: String,
}
type State = Arc<Mutex<Counters>>;
fn record(state: &State) {
    let mut counters = state.lock().expect("application counters poisoned");
    counters.completed += 1;
    counters.bytes += 64;
    let completed = counters.completed;
    if !counters.jobs.is_empty() {
        let index = completed as usize % counters.jobs.len();
        counters.jobs[index].elapsed_ms += 1;
        counters.jobs[index].retry_count = counters.jobs[index].elapsed_ms / 100;
    }
}
fn sync_client(mut stream: TcpStream, state: State, stop: Arc<AtomicBool>) {
    stream
        .set_nonblocking(false)
        .expect("blocking application connection");
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("set read timeout");
    stream
        .set_write_timeout(Some(Duration::from_millis(100)))
        .expect("set write timeout");
    let mut buffer = [0; 64];
    while !stop.load(Ordering::Relaxed) {
        match stream.read_exact(&mut buffer) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => break,
        }
        record(&state);
        if stream.write_all(&buffer).is_err() {
            break;
        }
    }
}
fn synchronous(listener: TcpListener, state: State, stop: Arc<AtomicBool>, deadline: Instant) {
    listener
        .set_nonblocking(true)
        .expect("set listener nonblocking");
    let mut clients = Vec::new();
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = state.clone();
                let stop = stop.clone();
                clients.push(std::thread::spawn(move || sync_client(stream, state, stop)));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1))
            }
            Err(error) => panic!("accept failed: {error}"),
        }
    }
    stop.store(true, Ordering::Relaxed);
    for client in clients {
        client.join().expect("client thread completes");
    }
}
#[cfg(feature = "async")]
fn asynchronous(listener: TcpListener, state: State, stop: Arc<AtomicBool>, deadline: Instant) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    listener
        .set_nonblocking(true)
        .expect("set listener nonblocking");
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("build runtime")
        .block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("convert listener");
            let mut clients = tokio::task::JoinSet::new();
            while Instant::now() < deadline {
                match tokio::time::timeout_at(deadline.into(), listener.accept()).await {
                    Ok(Ok((mut stream, _))) => {
                        let state = state.clone();
                        let stop = stop.clone();
                        clients.spawn(async move {
                            let mut buffer = [0; 64];
                            while !stop.load(Ordering::Relaxed) {
                                match tokio::time::timeout(
                                    Duration::from_millis(100),
                                    stream.read_exact(&mut buffer),
                                )
                                .await
                                {
                                    Ok(Ok(_)) => {}
                                    Err(_) => continue,
                                    _ => break,
                                }
                                record(&state);
                                if stream.write_all(&buffer).await.is_err() {
                                    break;
                                }
                            }
                        });
                    }
                    Err(_) => break,
                    Ok(Err(error)) => panic!("accept failed: {error}"),
                }
            }
            stop.store(true, Ordering::Relaxed);
            while clients.join_next().await.is_some() {}
        });
}
fn manual_debug(path: PathBuf, state: State, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    use std::os::unix::net::UnixListener;
    let listener = UnixListener::bind(&path).expect("bind debug endpoint");
    listener
        .set_nonblocking(true)
        .expect("nonblocking debug listener");
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .expect("blocking debug connection");
                    stream
                        .set_read_timeout(Some(Duration::from_millis(100)))
                        .expect("debug timeout");
                    stream
                        .set_write_timeout(Some(Duration::from_millis(100)))
                        .expect("debug timeout");
                    let mut bytes = Vec::new();
                    let mut byte = [0];
                    while bytes.len() < 4096 && stream.read_exact(&mut byte).is_ok() {
                        if byte[0] == b'\n' {
                            break;
                        }
                        bytes.push(byte[0]);
                    }
                    if let Ok(request) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        let minimum = request
                            .get("min_completed")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let counters = state.lock().expect("application counters poisoned").clone();
                        let rows = if counters.completed > minimum {
                            vec![counters]
                        } else {
                            Vec::new()
                        };
                        let mut reply = serde_json::to_vec(&rows).expect("serialize counters");
                        reply.push(b'\n');
                        let _ = stream.write_all(&reply);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("debug accept failed: {error}"),
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = std::fs::remove_file(path);
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::DirBuilderExt;
    let args: Vec<_> = std::env::args().collect();
    let engine = args.get(1).map(String::as_str).unwrap_or("sync");
    let mode = args.get(2).map(String::as_str).unwrap_or("baseline");
    let seconds = args
        .get(3)
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(4);
    let publish_ms = args
        .get(4)
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(10)
        .max(1);
    let task_rows = args
        .get(5)
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    let string_bytes = args
        .get(6)
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(32);
    if task_rows > 10_000 || string_bytes > 4096 {
        return Err("fixture size too large".into());
    }
    let jobs = (0..task_rows)
        .map(|id| Job {
            id: id as u64,
            state: "running".into(),
            worker_id: id as u64 % 4,
            elapsed_ms: 0,
            retry_count: 0,
            detail: "x".repeat(string_bytes),
        })
        .collect();
    let state = Arc::new(Mutex::new(Counters {
        jobs,
        ..Counters::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let publication_errors = Arc::new(AtomicU64::new(0));
    let publication_attempts = Arc::new(AtomicU64::new(0));
    let capture_lock_ns = Arc::new(AtomicU64::new(0));
    let capture_lock_max_ns = Arc::new(AtomicU64::new(0));
    let publication_ns = Arc::new(AtomicU64::new(0));
    let publication_max_ns = Arc::new(AtomicU64::new(0));
    let publication_error_codes =
        Arc::new(Mutex::new(std::collections::BTreeMap::<String, u64>::new()));
    let dir = std::env::temp_dir().join(format!("lq-workload-{}", std::process::id()));
    let path = dir.join("agent.sock");
    let mut workers = Vec::new();
    let mut diagnostic_snapshot_bytes = 0usize;
    if mode != "baseline" {
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    }
    match mode {
        "baseline" => {}
        "manual" => workers.push(manual_debug(path.clone(), state.clone(), stop.clone())),
        "published" | "jobs" => {
            #[cfg(feature = "inspection")]
            {
                let limits = livequery::Limits::default();
                let task_mode = mode == "jobs";
                if task_mode && task_rows == 0 {
                    return Err("jobs requires positive rows".into());
                }
                let published = if task_mode {
                    livequery::Published::new::<Job>(limits)?
                } else {
                    livequery::Published::new::<Counters>(limits)?
                };
                if task_mode {
                    let initial = state
                        .lock()
                        .expect("application counters poisoned")
                        .jobs
                        .clone();
                    let snapshot = livequery::Snapshot::capture(initial.iter(), limits)?;
                    diagnostic_snapshot_bytes = serde_json::to_vec(&snapshot)?.len();
                    published.publish(snapshot)?;
                }
                let registry = livequery::Registry::new();
                let guard = registry.expose_published(
                    if task_mode { "jobs" } else { "counters" },
                    published.clone(),
                )?;
                let server = livequery::local::Server::bind(&path, registry, limits)?;
                let shutdown = stop.clone();
                workers.push(std::thread::spawn(move || {
                    let _guard = guard;
                    server.serve(&shutdown).expect("agent runs");
                }));
                let state = state.clone();
                let shutdown = stop.clone();
                let errors = publication_errors.clone();
                let attempts = publication_attempts.clone();
                let error_codes = publication_error_codes.clone();
                let lock_total = capture_lock_ns.clone();
                let lock_max = capture_lock_max_ns.clone();
                let publish_total = publication_ns.clone();
                let publish_max = publication_max_ns.clone();
                workers.push(std::thread::spawn(move || {
                    while !shutdown.load(Ordering::Relaxed) {
                        let counters = {
                            let guard = state.lock().expect("application counters poisoned");
                            let start = Instant::now();
                            let copy = guard.clone();
                            let elapsed =
                                start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                            lock_total.fetch_add(elapsed, Ordering::Relaxed);
                            lock_max.fetch_max(elapsed, Ordering::Relaxed);
                            copy
                        };
                        let publication_started = Instant::now();
                        attempts.fetch_add(1, Ordering::Relaxed);
                        let snapshot = if task_mode {
                            livequery::Snapshot::capture(counters.jobs.iter(), limits)
                        } else {
                            livequery::Snapshot::capture([&counters], limits)
                        };
                        if let Err(error) =
                            snapshot.and_then(|snapshot| published.publish(snapshot))
                        {
                            errors.fetch_add(1, Ordering::Relaxed);
                            *error_codes
                                .lock()
                                .expect("diagnostic counters poisoned")
                                .entry(error.code().into())
                                .or_default() += 1;
                        }
                        let elapsed = publication_started
                            .elapsed()
                            .as_nanos()
                            .min(u128::from(u64::MAX)) as u64;
                        publish_total.fetch_add(elapsed, Ordering::Relaxed);
                        publish_max.fetch_max(elapsed, Ordering::Relaxed);
                        std::thread::sleep(Duration::from_millis(publish_ms));
                    }
                }));
            }
            #[cfg(not(feature = "inspection"))]
            {
                let _ = publish_ms;
                return Err("build with inspection to use published mode".into());
            }
        }
        _ => return Err("unknown mode".into()),
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    println!(
        "{}",
        serde_json::json!({"port":listener.local_addr()?.port(),"socket":path,"pid":std::process::id(),"diagnostic_snapshot_bytes":diagnostic_snapshot_bytes})
    );
    std::io::stdout().flush()?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    match engine {
        "sync" => synchronous(listener, state.clone(), stop.clone(), deadline),
        #[cfg(feature = "async")]
        "async" => asynchronous(listener, state.clone(), stop.clone(), deadline),
        _ => return Err("unknown engine".into()),
    }
    for worker in workers {
        worker.join().expect("diagnostic worker completes");
    }
    if mode != "baseline" {
        std::fs::remove_dir(dir)?;
    }
    println!(
        "{}",
        serde_json::json!({"capture_lock_ns":capture_lock_ns.load(Ordering::Relaxed),"capture_lock_max_ns":capture_lock_max_ns.load(Ordering::Relaxed),"publication_ns":publication_ns.load(Ordering::Relaxed),"publication_max_ns":publication_max_ns.load(Ordering::Relaxed),"publication_errors":publication_errors.load(Ordering::Relaxed),"publication_attempts":publication_attempts.load(Ordering::Relaxed),"publication_error_codes":*publication_error_codes.lock().expect("diagnostic counters poisoned"),"completed":state.lock().expect("counters poisoned").completed})
    );
    Ok(())
}
