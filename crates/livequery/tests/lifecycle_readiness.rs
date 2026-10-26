use livequery::{Inspect, Limits, PublishedHandle, Query, Registry, Table};
use std::sync::{Arc, RwLock};
#[derive(Debug, Inspect)]
struct Item {
    id: u64,
}
#[test]
fn failed_refresh_preserves_data_then_recovers_across_registration_cycles() {
    let registry = Registry::new();
    let shared = Arc::new(RwLock::new(vec![Item { id: 1 }]));
    let table = Table::from_shared(shared.clone());
    let query = Query::parse("SELECT id FROM items").unwrap();
    for id in 1..=50 {
        let handle =
            PublishedHandle::register::<Item>(&registry, "items", Limits::default()).unwrap();
        shared.write().unwrap()[0].id = id;
        handle.refresh(&table).unwrap();
        let mut lock = shared.write().unwrap();
        lock[0].id = id + 1;
        assert_eq!(handle.refresh(&table).unwrap_err().code(), "busy");
        assert_eq!(
            query.execute(&registry, Limits::default()).unwrap().rows[0]["id"],
            id
        );
        drop(lock);
        handle.refresh(&table).unwrap();
        assert_eq!(
            query.execute(&registry, Limits::default()).unwrap().rows[0]["id"],
            id + 1
        );
        assert_eq!(handle.status().unwrap().failures["busy"], 1);
        drop(handle);
        assert_eq!(
            query
                .execute(&registry, Limits::default())
                .unwrap_err()
                .code(),
            "not_found"
        );
    }
}
#[cfg(unix)]
#[test]
fn running_agent_closes_slow_clients_without_waiting_for_io_deadlines() {
    use livequery::local::RunningAgent;
    use std::{
        io::Write,
        os::unix::{fs::DirBuilderExt, net::UnixStream},
        time::{Duration, Instant},
    };
    let dir = std::env::temp_dir().join(format!("lqslow-{}", std::process::id()));
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    let path = dir.join("a.sock");
    let agent = RunningAgent::start(&path, Registry::new(), Limits::default()).unwrap();
    let mut peers = Vec::new();
    for _ in 0..16 {
        let mut peer = UnixStream::connect(&path).unwrap();
        peer.write_all(b"{").unwrap();
        peers.push(peer);
    }
    std::thread::sleep(Duration::from_millis(30));
    let start = Instant::now();
    agent.stop();
    agent.wait().unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "shutdown waited for incomplete request deadlines"
    );
    assert!(!path.exists());
    drop(peers);
    std::fs::remove_dir(dir).unwrap();
}
