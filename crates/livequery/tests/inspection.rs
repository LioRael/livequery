use livequery::{Error, Inspect, Limits, Query, Registry, Source, Table, Watch};
use serde::Serialize;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};
#[derive(Debug, Serialize)]
enum Status {
    Online,
    Offline,
}
#[derive(Debug, Inspect)]
struct SafePlayer {
    id: u64,
    health: i64,
    name: String,
    note: Option<String>,
    tags: Vec<String>,
    status: Status,
    #[inspect(skip)]
    secret: Secret,
}
#[derive(Debug)]
struct Secret;
fn player(id: u64, health: i64) -> SafePlayer {
    SafePlayer {
        id,
        health,
        name: "Ada's cat".into(),
        note: None,
        tags: vec!["admin".into()],
        status: if health > 0 {
            Status::Online
        } else {
            Status::Offline
        },
        secret: Secret,
    }
}
fn fixture() -> (Registry, Table<SafePlayer>, livequery::Registration) {
    let r = Registry::new();
    let table = Table::new(vec![player(1, 10), player(u64::MAX, 90)]);
    let guard = r.expose("players", table.clone()).unwrap();
    (r, table, guard)
}
fn execute(r: &Registry, sql: &str) -> livequery::QueryResult {
    Query::parse(sql)
        .unwrap()
        .execute(r, Limits::default())
        .unwrap()
}
#[test]
fn schema_projection_privacy_and_complex_values() {
    let (r, t, _g) = fixture();
    t.update(|rows| {
        let _secret = &rows[0].secret;
    })
    .unwrap();
    let result = execute(
        &r,
        "SELECT status, tags, note FROM players WHERE name = 'Ada''s cat' LIMIT 1",
    );
    assert_eq!(result.columns[0].name, "status");
    assert!(result.truncated);
    assert_eq!(result.rows[0]["status"], "Online");
    assert!(result.rows[0]["note"].is_null());
    assert!(
        !serde_json::to_string(&r.snapshot("players", Limits::default()).unwrap())
            .unwrap()
            .contains("secret")
    );
    assert!(
        Query::parse("SELECT secret FROM players")
            .unwrap()
            .execute(&r, Limits::default())
            .is_err()
    );
    t.update(Vec::clear).unwrap();
    assert_eq!(r.schemas()["players"].len(), 6);
    assert!(
        Query::parse("SELECT * FROM players WHERE unknown = 3")
            .unwrap()
            .execute(&r, Limits::default())
            .is_err()
    );
}
#[test]
fn integer_precision_boolean_precedence_and_null_logic() {
    let (r, _, _g) = fixture();
    assert_eq!(
        execute(&r, "SELECT id FROM players WHERE id = 18446744073709551615")
            .rows
            .len(),
        1
    );
    assert_eq!(
        execute(
            &r,
            "SELECT id FROM players WHERE health > -1 AND (health < 20 OR id > 1)"
        )
        .rows
        .len(),
        2
    );
    assert_eq!(
        execute(&r, "SELECT * FROM players WHERE NOT note = NULL")
            .rows
            .len(),
        0
    );
    assert_eq!(
        execute(
            &r,
            "SELECT * FROM players WHERE note IS NULL AND NOT health > 20"
        )
        .rows
        .len(),
        1
    );
    assert_eq!(
        execute(&r, "SELECT * FROM players WHERE note = NULL OR health = 10")
            .rows
            .len(),
        1
    );
    assert!(execute(&r, "SELECT * FROM players LIMIT 0").truncated);
}
#[test]
fn invalid_sql_is_never_silently_accepted() {
    for sql in [
        "DELETE FROM players",
        "SELECT * FROM players ORDER BY id",
        "SELECT * FROM players JOIN other ON id = id",
        "SELECT * FROM players; SELECT * FROM players",
        "SELECT count(*) FROM players",
        "SELECT id, id FROM players",
        "SELECT * FROM players LIMIT -1",
        "SELECT * FROM players OFFSET 2",
        "SELECT * FROM players WHERE id = -'x'",
    ] {
        assert!(Query::parse(sql).is_err(), "accepted {sql}");
    }
    assert!(
        Query::parse(&format!(
            "SELECT * FROM players WHERE {}id = 1{}",
            "(".repeat(40),
            ")".repeat(40)
        ))
        .is_err()
    );
    assert!(Query::parse(&"x".repeat(4097)).is_err());
}
#[test]
fn lifecycle_and_watch_cancellation_release_sources() {
    let (r, t, g) = fixture();
    assert!(r.expose("players", t.clone()).is_err());
    let mut watch = Watch::new(Query::parse("SELECT id FROM players WHERE health < 20").unwrap());
    assert!(watch.poll(&r, Limits::default()).unwrap().is_some());
    assert!(watch.poll(&r, Limits::default()).unwrap().is_none());
    t.update(|rows| rows[0].health = 30).unwrap();
    assert!(
        watch
            .poll(&r, Limits::default())
            .unwrap()
            .unwrap()
            .rows
            .is_empty()
    );
    drop(watch);
    drop(g);
    assert!(r.schemas().is_empty());
    assert!(matches!(
        r.snapshot("players", Limits::default()),
        Err(Error::NotFound(_))
    ));
    let _new = r.expose("players", t).unwrap();
}
#[test]
fn busy_and_resource_limits() {
    let shared = Arc::new(RwLock::new(vec![player(1, 10)]));
    let table = Table::from_shared(shared.clone());
    let guard = shared.write().unwrap();
    assert!(matches!(
        table.snapshot(Limits::default()),
        Err(Error::Busy)
    ));
    drop(guard);
    assert!(matches!(
        table.snapshot(Limits {
            scan_rows: 0,
            ..Limits::default()
        }),
        Err(Error::Limit(_))
    ));
    assert!(matches!(
        table.snapshot(Limits {
            bytes: 1,
            ..Limits::default()
        }),
        Err(Error::Limit(_))
    ));
}
#[derive(Debug, Inspect)]
struct Pair {
    a: u64,
    b: u64,
}
#[test]
fn concurrent_snapshots_never_tear() {
    let table = Table::new(vec![Pair { a: 0, b: 0 }]);
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let table = table.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let _ = table.update(|r| {
                    r[0].a += 1;
                    r[0].b = r[0].a;
                });
            }
        })
    };
    let mut snapshots = 0;
    for _ in 0..10_000 {
        if let Ok(s) = table.snapshot(Limits::default()) {
            assert_eq!(s.rows[0]["a"], s.rows[0]["b"]);
            snapshots += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
    assert!(snapshots > 0);
}
#[test]
fn public_handles_are_send_sync() {
    fn check<T: Send + Sync>() {}
    check::<Registry>();
    check::<Table<Pair>>();
    check::<Query>();
    check::<Error>();
}

#[test]
fn derived_serialization_rejects_large_fields_before_copying_entire_row() {
    let row = player(1, 2);
    assert!(matches!(row.inspect_bounded(8), Err(Error::Limit(_))));
    let mut large = player(2, 3);
    large.name = "x".repeat(2_000_000);
    assert!(matches!(large.inspect(), Err(Error::Limit(_))));
}

#[test]
fn malformed_input_corpus_never_panics() {
    let alphabet = b"SELECT FROM WHERE()'\"=<>;-0123456789*\\\n\0";
    let mut seed = 42_u64;
    for length in 0..256 {
        let input: String = (0..length)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                alphabet[(seed as usize) % alphabet.len()] as char
            })
            .collect();
        let _ = Query::parse(&input);
    }
}

#[derive(Debug)]
struct MutexSource {
    rows: std::sync::Mutex<Vec<Pair>>,
    dropped: Arc<AtomicBool>,
}
impl Drop for MutexSource {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::Relaxed);
    }
}
impl Source for MutexSource {
    fn schema(&self) -> Vec<livequery::Column> {
        Pair::schema()
    }
    fn snapshot(&self, limits: Limits) -> Result<livequery::Snapshot, Error> {
        let rows = self.rows.try_lock().map_err(|_| Error::Busy)?;
        if rows.len() > limits.scan_rows {
            return Err(Error::Limit("adapter rows".into()));
        }
        let mut remaining = limits.bytes;
        let rows = rows
            .iter()
            .map(|row| {
                let row = row.inspect_bounded(remaining)?;
                let bytes = serde_json::to_vec(&row)?.len();
                remaining = remaining
                    .checked_sub(bytes)
                    .ok_or_else(|| Error::Limit("adapter bytes".into()))?;
                Ok(row)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(livequery::Snapshot {
            columns: self.schema(),
            rows,
        })
    }
}
#[test]
fn custom_mutex_adapter_releases_on_unregistration() {
    let dropped = Arc::new(AtomicBool::new(false));
    let registry = Registry::new();
    let guard = registry
        .expose(
            "pairs",
            MutexSource {
                rows: std::sync::Mutex::new(vec![Pair { a: 1, b: 1 }]),
                dropped: dropped.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        execute(&registry, "SELECT a FROM pairs WHERE b = 1")
            .rows
            .len(),
        1
    );
    drop(guard);
    assert!(
        dropped.load(Ordering::Relaxed),
        "catalog must not retain unregistered source"
    );
    assert!(registry.schemas().is_empty());
}
#[derive(Debug)]
struct BadSource;
impl Source for BadSource {
    fn schema(&self) -> Vec<livequery::Column> {
        Pair::schema()
    }
    fn snapshot(&self, _: Limits) -> Result<livequery::Snapshot, Error> {
        Ok(livequery::Snapshot {
            columns: Pair::schema(),
            rows: vec![livequery::Row::new()],
        })
    }
}
#[test]
fn inconsistent_custom_snapshots_fail_before_query_evaluation() {
    let registry = Registry::new();
    let _guard = registry.expose("bad", BadSource).unwrap();
    assert!(matches!(
        registry.snapshot("bad", Limits::default()),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn integer_literals_reject_overflow_and_preserve_negative_zero() {
    assert!(Query::parse("SELECT * FROM players WHERE id = 18446744073709551616").is_err());
    assert!(Query::parse("SELECT * FROM players WHERE health = -9223372036854775809").is_err());
    let (registry, _, _guard) = fixture();
    assert_eq!(
        execute(&registry, "SELECT * FROM players WHERE health > -0")
            .rows
            .len(),
        2
    );
}
