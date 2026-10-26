use livequery::{
    Agent, Error, Inspect, Limits, Published, Query, Registry, Snapshot, Source, Table,
};
use livequery_transport::{Cursor, Observation, PROTOCOL_VERSION, Request, Response};

#[derive(Debug, Inspect)]
struct Item {
    id: u64,
    name: String,
}
fn fixture() -> (Registry, livequery::Registration, Published) {
    let registry = Registry::new();
    let published = Published::new::<Item>(Limits::default()).unwrap();
    published
        .refresh(&Table::new(vec![Item {
            id: 42,
            name: "Ada".into(),
        }]))
        .unwrap();
    let registration = registry
        .expose_published("items", published.clone())
        .unwrap();
    (registry, registration, published)
}
fn observe(agent: &Agent, sql: &str, since: Option<Cursor>) -> Observation {
    let Response::Ok { data } = agent
        .handle(Request::ObserveV2 {
            sql: sql.into(),
            since,
        })
        .unwrap()
    else {
        panic!("expected success")
    };
    serde_json::from_value(data).unwrap()
}
#[test]
fn session_query_and_source_replacement_invalidate_cursors() {
    let (registry, guard, published) = fixture();
    let agent = Agent::new(registry.clone(), Limits::default());
    let Observation::Changed { cursor, .. } = observe(&agent, "SELECT id FROM items", None) else {
        panic!("initial result")
    };
    assert!(matches!(
        observe(&agent, "SELECT id FROM items", Some(cursor.clone())),
        Observation::Unchanged { .. }
    ));
    assert!(matches!(
        observe(&agent, "SELECT name FROM items", Some(cursor.clone())),
        Observation::Changed { .. }
    ));
    let restarted = Agent::new(registry.clone(), Limits::default());
    assert!(matches!(
        observe(&restarted, "SELECT id FROM items", Some(cursor.clone())),
        Observation::Changed { .. }
    ));
    drop(guard);
    let _new = registry.expose_published("items", published).unwrap();
    assert!(matches!(
        observe(&agent, "SELECT id FROM items", Some(cursor)),
        Observation::Changed { .. }
    ));
}
#[test]
fn negotiation_rejects_incompatible_versions_and_legacy_queries_still_work() {
    let (registry, _guard, _) = fixture();
    let agent = Agent::new(registry, Limits::default());
    assert!(
        matches!(agent.handle(Request::Hello { protocol: 99 }).unwrap(), Response::Error { code, .. } if code == "incompatible_protocol")
    );
    assert!(
        matches!(agent.handle(Request::Hello { protocol: PROTOCOL_VERSION }).unwrap(), Response::Ok { data } if data["protocol"] == PROTOCOL_VERSION)
    );
    assert!(
        matches!(agent.handle(Request::Query { sql: "SELECT id FROM items".into() }).unwrap(), Response::Ok { data } if data["rows"][0]["id"] == 42)
    );
}
#[derive(Debug)]
struct BusySource;
impl Source for BusySource {
    fn schema(&self) -> Vec<livequery::Column> {
        Item::schema()
    }
    fn snapshot(&self, _: Limits) -> Result<Snapshot, Error> {
        Err(Error::Busy)
    }
}
#[test]
fn stale_publication_remains_queryable_and_failures_are_classified_once() {
    let empty = Published::new::<Item>(Limits::default()).unwrap();
    assert!(empty.status().unwrap().age_ms.is_none());
    let (registry, _guard, published) = fixture();
    let previous = published.status().unwrap();
    assert!(published.refresh(&BusySource).is_err());
    assert!(
        published
            .publish(Snapshot {
                columns: Item::schema(),
                rows: vec![Default::default()]
            })
            .is_err()
    );
    let status = published.status().unwrap();
    assert_eq!(status.sequence, previous.sequence);
    assert_eq!(status.published_at_unix_ms, previous.published_at_unix_ms);
    assert_eq!(status.failures["busy"], 1);
    assert_eq!(status.failures["invalid"], 1);
    let Response::Ok { data } = Agent::new(registry, Limits::default())
        .handle(Request::Status)
        .unwrap()
    else {
        panic!("status")
    };
    assert_eq!(data["items"]["failures"]["busy"], 1);
    assert_eq!(
        published.snapshot(Limits::default()).unwrap().rows[0]["id"],
        42
    );
    published
        .refresh(&Table::new(vec![Item {
            id: 43,
            name: "Lin".into(),
        }]))
        .unwrap();
    assert_eq!(published.status().unwrap().sequence, previous.sequence + 1);
}
#[test]
fn repeated_projection_and_aggregate_catalog_are_bounded() {
    let (registry, _guard, _) = fixture();
    assert!(matches!(
        Query::parse("SELECT id,id FROM items"),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        registry.schemas_bounded(Limits {
            bytes: 10,
            ..Limits::default()
        }),
        Err(Error::Limit(_))
    ));
}
use proptest::prelude::*;
proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn arbitrary_queries_and_wire_frames_do_not_panic(input in prop::collection::vec(any::<u8>(), 0..9000)) {
        let _: Result<Request, _> = serde_json::from_slice(&input);
        if let Ok(sql) = std::str::from_utf8(&input)
            && let Ok(query) = Query::parse(sql) {
                let (registry, _guard, _) = fixture();
                let _ = query.execute(&registry, Limits::default());
        }
    }
    #[test]
    fn selection_matches_numeric_predicate(threshold in 0u64..100, ids in prop::collection::vec(0u64..100, 0..100)) {
        let registry = Registry::new();
        let _guard = registry.expose("items", Table::new(ids.iter().map(|id| Item { id: *id, name: "x".into() }).collect::<Vec<_>>())).unwrap();
        // This property checks selection semantics, not scheduler-dependent latency.
        let limits = Limits { query_timeout: std::time::Duration::from_secs(5), ..Limits::default() };
        let result = Query::parse(&format!("SELECT id FROM items WHERE id > {threshold}")).unwrap().execute(&registry, limits).unwrap();
        let actual: Vec<_> = result.rows.iter().map(|row| row["id"].as_u64().unwrap()).collect();
        let expected: Vec<_> = ids.into_iter().filter(|id| *id > threshold).collect();
        prop_assert_eq!(actual, expected);
    }
}

#[test]
fn nested_enum_secrets_are_omitted_from_values_and_schema() {
    #[derive(Debug, Inspect, livequery::serde::Serialize, livequery::schemars::JsonSchema)]
    #[serde(crate = "livequery::serde", tag = "kind")]
    #[schemars(crate = "livequery::schemars")]
    enum Details {
        Active {
            visible: u64,
            #[serde(skip)]
            secret: String,
        },
    }
    #[derive(Debug, Inspect)]
    struct Outer {
        details: Details,
    }
    let details = Details::Active {
        visible: 7,
        secret: "never-export-this".into(),
    };
    let Details::Active { secret, .. } = &details;
    assert!(!secret.is_empty());
    let root_snapshot = Snapshot::capture([&details], Limits::default()).unwrap();
    let root_serialized = serde_json::to_string(&root_snapshot).unwrap();
    assert!(
        !root_serialized.contains("\"secret\"") && !root_serialized.contains("never-export-this")
    );
    let snapshot = Snapshot::capture([&Outer { details }], Limits::default()).unwrap();
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("\"secret\"") && !serialized.contains("never-export-this"));
    assert!(serialized.contains("visible"));
}

#[test]
fn repeated_registration_and_snapshot_lifetimes_release_sources() {
    use std::sync::{Arc, RwLock};
    let registry = Registry::new();
    let source = Arc::new(RwLock::new(vec![Item {
        id: 1,
        name: "x".into(),
    }]));
    let weak = Arc::downgrade(&source);
    for _ in 0..500 {
        let guard = registry
            .expose("items", Table::from_shared(source.clone()))
            .unwrap();
        let snapshot = registry.snapshot("items", Limits::default()).unwrap();
        drop(guard);
        assert!(registry.schemas().is_empty());
        assert_eq!(snapshot.rows[0]["id"], 1);
        assert_eq!(Arc::strong_count(&source), 1);
    }
    drop(source);
    assert!(weak.upgrade().is_none());
}
