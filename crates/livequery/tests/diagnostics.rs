use livequery::{Inspect, Limits, PublishedHandle, Query, Registry, Snapshot, Table};
#[derive(Debug, Inspect)]
struct Job {
    id: u64,
    elapsed_ms: Option<u64>,
    state: String,
}
fn rows() -> Vec<Job> {
    vec![(1, Some(10)), (2, None), (3, Some(50)), (4, Some(50))]
        .into_iter()
        .map(|(id, elapsed_ms)| Job {
            id,
            elapsed_ms,
            state: "running".into(),
        })
        .collect()
}
#[test]
fn ordering_is_stable_nulls_last_and_limit_counts_full_scan() {
    let registry = Registry::new();
    let _guard = registry.expose("jobs", Table::new(rows())).unwrap();
    for (direction, expected) in [("DESC", vec![3, 4, 1, 2]), ("ASC", vec![1, 3, 4, 2])] {
        let result = Query::parse(&format!(
            "SELECT id FROM jobs ORDER BY elapsed_ms {direction}"
        ))
        .unwrap()
        .execute(&registry, Limits::default())
        .unwrap();
        assert_eq!(
            result
                .rows
                .iter()
                .map(|r| r["id"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
    }
    let result = Query::parse(
        "SELECT id FROM jobs WHERE state = 'running' ORDER BY elapsed_ms DESC LIMIT 2;",
    )
    .unwrap()
    .execute(&registry, Limits::default())
    .unwrap();
    assert_eq!(result.scanned, 4);
    assert_eq!(result.rows.len(), 2);
    assert!(result.truncated);
    assert!(
        Query::parse("SELECT id FROM jobs ORDER BY missing")
            .unwrap()
            .execute(&registry, Limits::default())
            .is_err()
    );
}
#[test]
fn aggregates_scan_all_rows_and_have_defined_empty_null_semantics() {
    let registry = Registry::new();
    let _guard = registry.expose("jobs", Table::new(rows())).unwrap();
    for (sql, key, value) in [
        (
            "SELECT COUNT(*) FROM jobs",
            "count",
            livequery::serde_json::json!(4),
        ),
        (
            "SELECT MAX(elapsed_ms) FROM jobs",
            "max",
            livequery::serde_json::json!(50),
        ),
        (
            "SELECT COUNT(*) FROM jobs WHERE id > 100",
            "count",
            livequery::serde_json::json!(0),
        ),
        (
            "SELECT MAX(elapsed_ms) FROM jobs WHERE id = 2",
            "max",
            livequery::serde_json::Value::Null,
        ),
    ] {
        let result = Query::parse(sql)
            .unwrap()
            .execute(&registry, Limits::default())
            .unwrap();
        assert_eq!(result.rows[0][key], value);
        assert_eq!(result.scanned, 4);
        assert!(!result.truncated);
    }
    assert!(Query::parse("SELECT COUNT(*), id FROM jobs").is_err());
    assert!(Query::parse("SELECT MAX(elapsed_ms) FROM jobs ORDER BY id").is_err());
    let limited = Query::parse("SELECT COUNT(*) FROM jobs LIMIT 0")
        .unwrap()
        .execute(&registry, Limits::default())
        .unwrap();
    assert!(limited.rows.is_empty() && limited.truncated);
    assert!(
        Query::parse("SELECT COUNT(*) FROM jobs LIMIT 1")
            .unwrap()
            .execute(
                &registry,
                Limits {
                    scan_rows: 2,
                    ..Limits::default()
                }
            )
            .is_err()
    );
}
#[test]
fn immutable_views_and_query_metadata_refer_to_the_same_version() {
    let registry = Registry::new();
    let handle = PublishedHandle::register::<Job>(&registry, "jobs", Limits::default()).unwrap();
    assert!(handle.status().unwrap().age_ms.is_none());
    handle
        .publish(Snapshot::capture(rows().iter(), Limits::default()).unwrap())
        .unwrap();
    let first = registry.read_snapshot("jobs", Limits::default()).unwrap();
    let same = registry.read_snapshot("jobs", Limits::default()).unwrap();
    assert!(std::ptr::eq(first.snapshot(), same.snapshot()));
    handle
        .publish(
            Snapshot::capture(
                [&Job {
                    id: 99,
                    elapsed_ms: Some(100),
                    state: "done".into(),
                }],
                Limits::default(),
            )
            .unwrap(),
        )
        .unwrap();
    let result = Query::parse("SELECT MAX(elapsed_ms) FROM jobs")
        .unwrap()
        .execute_detailed(&registry, Limits::default())
        .unwrap();
    assert_eq!(result.result.rows[0]["max"], 100);
    assert_eq!(
        result.revision.unwrap().sequence(),
        result.publication.unwrap().sequence
    );
    assert_eq!(first.snapshot().rows.len(), 4);
    assert_eq!(first.publication().unwrap().sequence, 1);
    assert!(
        handle
            .refresh(&Table::new(vec![Job {
                id: 1,
                elapsed_ms: None,
                state: "x".repeat(2_000_000)
            }]))
            .is_err()
    );
    assert_eq!(handle.status().unwrap().sequence, 2);
    drop(handle);
    assert!(registry.read_snapshot("jobs", Limits::default()).is_err());
    assert_eq!(first.snapshot().rows.len(), 4);
}
#[test]
fn incompatible_sort_values_are_rejected_without_lossy_numeric_coercion() {
    #[derive(Debug, Inspect)]
    struct Item {
        value: livequery::serde_json::Value,
    }
    let registry = Registry::new();
    let _guard = registry
        .expose(
            "items",
            Table::new(vec![
                Item { value: 1.into() },
                Item {
                    value: livequery::serde_json::json!(1.5),
                },
            ]),
        )
        .unwrap();
    for sql in [
        "SELECT * FROM items ORDER BY value",
        "SELECT MAX(value) FROM items",
    ] {
        assert!(
            Query::parse(sql)
                .unwrap()
                .execute(&registry, Limits::default())
                .is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn running_agent_explicit_and_drop_shutdown_release_socket() {
    use livequery::local::RunningAgent;
    use std::os::unix::fs::DirBuilderExt;
    let dir = std::env::temp_dir().join(format!(
        "lqr-{}-{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    let path = dir.join("a.sock");
    let agent = RunningAgent::start(&path, Registry::new(), Limits::default()).unwrap();
    assert!(RunningAgent::start(&path, Registry::new(), Limits::default()).is_err());
    agent.stop();
    agent.stop();
    agent.wait().unwrap();
    assert!(!path.exists());
    drop(RunningAgent::start(&path, Registry::new(), Limits::default()).unwrap());
    assert!(!path.exists());
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn ordered_integer_extremes_and_null_max_preserve_exact_values() {
    #[derive(Debug, Inspect)]
    struct Number {
        value: livequery::serde_json::Value,
    }
    let registry = Registry::new();
    let _guard = registry
        .expose(
            "numbers",
            Table::new(vec![
                Number {
                    value: u64::MAX.into(),
                },
                Number {
                    value: i64::MIN.into(),
                },
                Number {
                    value: 9_007_199_254_740_993u64.into(),
                },
            ]),
        )
        .unwrap();
    let result = Query::parse("SELECT value FROM numbers ORDER BY value ASC")
        .unwrap()
        .execute(&registry, Limits::default())
        .unwrap();
    assert_eq!(result.rows[0]["value"], i64::MIN);
    assert_eq!(result.rows[1]["value"], 9_007_199_254_740_993u64);
    assert_eq!(result.rows[2]["value"], u64::MAX);
    assert_eq!(
        Query::parse("SELECT MAX(value) FROM numbers")
            .unwrap()
            .execute(&registry, Limits::default())
            .unwrap()
            .rows[0]["max"],
        u64::MAX
    );
}
