#![cfg(unix)]
use livequery::{
    Inspect, Limits, Registry, Table,
    local::{self, Request, Response, Server},
};
use std::{
    fs,
    io::Write,
    os::unix::{fs::PermissionsExt, net::UnixStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
#[derive(Debug, Inspect)]
struct Item {
    id: u64,
}
#[test]
fn real_socket_query_disconnect_invalid_frame_and_cleanup() {
    let dir = std::env::temp_dir().join(format!("lq-test-{}", std::process::id()));
    let path = dir.join("agent.sock");
    let r = Registry::new();
    let _g = r
        .expose("items", Table::new(vec![Item { id: 42 }]))
        .unwrap();
    let server = Server::bind(&path, r.clone(), Limits::default()).unwrap();
    assert!(Server::bind(&path, r, Limits::default()).is_err());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let stop = stop.clone();
        std::thread::spawn(move || server.serve(&stop).unwrap())
    };
    assert!(matches!(
        local::request(&path, &Request::Schema).unwrap(),
        Response::Ok { .. }
    ));
    let result = local::request(
        &path,
        &Request::Query {
            sql: "SELECT * FROM items".into(),
        },
    )
    .unwrap();
    match result {
        Response::Ok { data } => assert_eq!(data["rows"][0]["id"], 42),
        other => panic!("unexpected {other:?}"),
    }
    assert!(
        matches!(local::request(&path, &Request::Query { sql: "DROP TABLE items".into() }).unwrap(), Response::Error { code, .. } if code == "invalid")
    );
    drop(UnixStream::connect(&path).unwrap());
    {
        let mut bad = UnixStream::connect(&path).unwrap();
        bad.write_all(b"{bad json}\n").unwrap();
    }
    {
        let mut large = UnixStream::connect(&path).unwrap();
        let _ = large.write_all(&vec![b'x'; 9000]);
    }
    assert!(matches!(
        local::request(&path, &Request::Schema).unwrap(),
        Response::Ok { .. }
    ));
    let idle = UnixStream::connect(&path).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    drop(idle);
    assert!(!path.exists());
    fs::remove_dir(dir).unwrap();
}
#[test]
fn rejects_public_directory() {
    let dir = std::env::temp_dir().join(format!("lq-insecure-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Server::bind(dir.join("agent.sock"), Registry::new(), Limits::default()).is_err());
    fs::remove_dir(dir).unwrap();
}

#[test]
fn idle_client_does_not_delay_healthy_queries() {
    let dir = std::env::temp_dir().join(format!("lq-fair-{}", std::process::id()));
    let path = dir.join("agent.sock");
    let server = Server::bind(&path, Registry::new(), Limits::default()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let stop = stop.clone();
        std::thread::spawn(move || server.serve(&stop).unwrap())
    };
    let idle = UnixStream::connect(&path).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let started = std::time::Instant::now();
    let response = local::request(&path, &Request::Schema);
    let elapsed = started.elapsed();
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    drop(idle);
    fs::remove_dir(dir).unwrap();
    assert!(matches!(response.unwrap(), Response::Ok { .. }));
    assert!(
        elapsed < Duration::from_secs(1),
        "idle peer delayed query by {elapsed:?}"
    );
}

#[test]
fn trickling_request_does_not_extend_deadline() {
    let dir = std::env::temp_dir().join(format!("lq-trickle-{}", std::process::id()));
    let path = dir.join("agent.sock");
    let server = Server::bind(&path, Registry::new(), Limits::default()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let stop = stop.clone();
        std::thread::spawn(move || server.serve(&stop).unwrap())
    };
    let mut stream = UnixStream::connect(&path).unwrap();
    let start = std::time::Instant::now();
    let mut closed = false;
    while start.elapsed() < Duration::from_secs(3) {
        if stream.write_all(b" ").is_err() {
            closed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    fs::remove_dir(dir).unwrap();
    assert!(
        closed,
        "trickling request kept its connection beyond the deadline"
    );
}

#[derive(Debug, Inspect)]
struct LargeRow {
    value: String,
}
#[test]
fn slow_reader_is_disconnected_without_stalling_other_clients() {
    use std::io::Read;
    let dir = std::env::temp_dir().join(format!("lq-slow-read-{}", std::process::id()));
    let path = dir.join("agent.sock");
    let registry = Registry::new();
    let _guard = registry
        .expose(
            "large",
            Table::new(vec![LargeRow {
                value: "x".repeat(900_000),
            }]),
        )
        .unwrap();
    let server = Server::bind(&path, registry, Limits::default()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let stop = stop.clone();
        std::thread::spawn(move || server.serve(&stop).unwrap())
    };
    let mut slow = UnixStream::connect(&path).unwrap();
    slow.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    slow.write_all(b"{\"command\":\"query\",\"sql\":\"SELECT * FROM large\"}\n")
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let start = std::time::Instant::now();
    let response = local::request(&path, &Request::Schema).unwrap();
    assert!(matches!(response, Response::Ok { .. }));
    assert!(start.elapsed() < Duration::from_secs(1));
    std::thread::sleep(Duration::from_millis(2300));
    let mut received = Vec::new();
    let result = slow.read_to_end(&mut received);
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    fs::remove_dir(dir).unwrap();
    result.expect("slow client must see EOF after the write deadline");
    assert!(
        received.len() < 900_000,
        "fixture should exceed the socket send buffer"
    );
}

#[derive(Debug)]
struct LargeSchema;
impl livequery::Source for LargeSchema {
    fn schema(&self) -> Vec<livequery::Column> {
        vec![livequery::Column {
            name: "id".into(),
            rust_type: "x".repeat(2_000_000),
        }]
    }
    fn snapshot(&self, _: Limits) -> Result<livequery::Snapshot, livequery::Error> {
        Ok(livequery::Snapshot {
            columns: self.schema(),
            rows: Vec::new(),
        })
    }
}
#[test]
fn oversized_schema_returns_a_bounded_error() {
    let dir = std::env::temp_dir().join(format!("lq-schema-{}", std::process::id()));
    let path = dir.join("agent.sock");
    let registry = Registry::new();
    let _guard = registry.expose("large", LargeSchema).unwrap();
    let server = Server::bind(&path, registry, Limits::default()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let stop = stop.clone();
        std::thread::spawn(move || server.serve(&stop).unwrap())
    };
    let response = local::request(&path, &Request::Schema).unwrap();
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    fs::remove_dir(dir).unwrap();
    assert!(matches!(response, Response::Error { code, .. } if code == "limit"));
}

#[test]
fn cleanup_does_not_remove_a_replacement_file() {
    let dir = std::env::temp_dir().join(format!("lq-replaced-{}", std::process::id()));
    let path = dir.join("agent.sock");
    let server = Server::bind(&path, Registry::new(), Limits::default()).unwrap();
    fs::remove_file(&path).unwrap();
    fs::write(&path, "replacement").unwrap();
    drop(server);
    assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
    fs::remove_file(path).unwrap();
    fs::remove_dir(dir).unwrap();
}

#[test]
fn full_listener_backlog_cannot_block_client_indefinitely() {
    use std::os::unix::fs::DirBuilderExt;
    let dir = std::env::temp_dir().join(format!("lq-backlog-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    let path = dir.join("agent.sock");
    let address = socket2::SockAddr::unix(&path).unwrap();
    let listener =
        socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
    listener.bind(&address).unwrap();
    listener.listen(1).unwrap();
    let mut pending = Vec::new();
    let mut full = false;
    for _ in 0..128 {
        let socket =
            socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
        socket.set_nonblocking(true).unwrap();
        match socket.connect(&address) {
            Ok(()) => pending.push(socket),
            Err(_) => {
                full = true;
                break;
            }
        }
    }
    let start = std::time::Instant::now();
    let response = local::request(&path, &Request::Schema);
    let elapsed = start.elapsed();
    drop(pending);
    drop(listener);
    fs::remove_file(path).unwrap();
    fs::remove_dir(dir).unwrap();
    assert!(full, "test must fill the listener backlog");
    assert!(
        response.is_err(),
        "unserved listener cannot produce a valid response"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "connect exceeded its budget: {elapsed:?}"
    );
}

#[test]
fn symlinked_socket_directory_is_rejected() {
    use std::os::unix::fs::{DirBuilderExt, symlink};
    let dir = std::env::temp_dir().join(format!("lq-symlink-{}", std::process::id()));
    let link = dir.with_extension("link");
    fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    symlink(&dir, &link).unwrap();
    assert!(Server::bind(link.join("agent.sock"), Registry::new(), Limits::default()).is_err());
    fs::remove_file(link).unwrap();
    fs::remove_dir(dir).unwrap();
}
