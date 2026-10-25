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
#[derive(Debug, Clone, Copy, Serialize, Default)]
#[cfg_attr(feature = "inspection", derive(livequery::Inspect))]
struct Counters {
    completed: u64,
    bytes: u64,
}
type State = Arc<Mutex<Counters>>;
fn record(state: &State) {
    let mut counters = state.lock().expect("application counters poisoned");
    counters.completed += 1;
    counters.bytes += 64;
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
                        let counters = *state.lock().expect("application counters poisoned");
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
    let state = Arc::new(Mutex::new(Counters::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let publication_errors = Arc::new(AtomicU64::new(0));
    let dir = std::env::temp_dir().join(format!("lq-workload-{}", std::process::id()));
    let path = dir.join("agent.sock");
    let mut workers = Vec::new();
    if mode != "baseline" {
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    }
    match mode {
        "baseline" => {}
        "manual" => workers.push(manual_debug(path.clone(), state.clone(), stop.clone())),
        "published" => {
            #[cfg(feature = "inspection")]
            {
                let limits = livequery::Limits::default();
                let published = livequery::Published::new::<Counters>(limits)?;
                let registry = livequery::Registry::new();
                let guard = registry.expose_published("counters", published.clone())?;
                let server = livequery::local::Server::bind(&path, registry, limits)?;
                let shutdown = stop.clone();
                workers.push(std::thread::spawn(move || {
                    let _guard = guard;
                    server.serve(&shutdown).expect("agent runs");
                }));
                let state = state.clone();
                let shutdown = stop.clone();
                let errors = publication_errors.clone();
                workers.push(std::thread::spawn(move || {
                    while !shutdown.load(Ordering::Relaxed) {
                        let counters = *state.lock().expect("application counters poisoned");
                        if livequery::Snapshot::capture([&counters], limits)
                            .and_then(|snapshot| published.publish(snapshot))
                            .is_err()
                        {
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
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
        serde_json::json!({"port":listener.local_addr()?.port(),"socket":path,"pid":std::process::id()})
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
        serde_json::json!({"publication_errors":publication_errors.load(Ordering::Relaxed),"completed":state.lock().expect("counters poisoned").completed})
    );
    Ok(())
}
