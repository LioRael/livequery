#![cfg(unix)]
use livequery_core::{Inspect, Limits, Published, Registry, Table};
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
#[derive(Debug, Inspect)]
struct Item {
    id: u64,
}
fn start(path: &std::path::Path, id: u64) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let registry = Registry::new();
    let source = Published::new::<Item>(Limits::default()).unwrap();
    source.refresh(&Table::new(vec![Item { id }])).unwrap();
    let guard = registry.expose_published("items", source).unwrap();
    let server = livequery_agent::local::Server::bind(path, registry, Limits::default()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    let thread = std::thread::spawn(move || {
        let _guard = guard;
        server.serve(&signal).unwrap();
    });
    (stop, thread)
}
#[test]
fn watch_recovers_after_socket_replacement_without_reusing_cached_rows() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.path().join("agent.sock");
    let (stop, server) = start(&socket, 1);
    let mut child = Command::new(env!("CARGO_BIN_EXE_livequery"))
        .args(["watch", "SELECT * FROM items", "--socket"])
        .arg(&socket)
        .args(["--json", "--interval-ms", "100", "--count", "10"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let first = rx.recv_timeout(Duration::from_secs(5));
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    std::thread::sleep(Duration::from_millis(180));
    let (stop, server) = start(&socket, 2);
    let second = rx.recv_timeout(Duration::from_secs(5));
    // Always clean up before assertions, including failed observation paths.
    if first.is_err() || second.is_err() {
        let _ = child.kill();
    }
    let output = child.wait_with_output().unwrap();
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    reader.join().unwrap();
    let first: serde_json::Value = serde_json::from_str(&first.unwrap()).unwrap();
    let second: serde_json::Value = serde_json::from_str(&second.unwrap()).unwrap();
    assert_eq!(first["rows"][0]["id"], 1);
    assert_eq!(second["rows"][0]["id"], 2);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("retry"));
}
