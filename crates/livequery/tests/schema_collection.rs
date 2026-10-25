use livequery::{
    CollectionSource, Inspect, Limits, Published, Query, Registry, Snapshot, Source, Table,
    schemars,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{Arc, Mutex},
};

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[schemars(crate = "livequery::schemars")]
struct Node {
    name: String,
    children: Vec<Node>,
    #[serde(skip)]
    secret: String,
}
#[derive(Debug, Serialize, schemars::JsonSchema, Inspect)]
#[schemars(crate = "livequery::schemars")]
enum Status {
    Idle,
    Running { progress: u32 },
    Failed(String),
}
#[derive(Debug, Inspect)]
struct Record {
    id: u64,
    node: Option<Node>,
    state: Status,
    tags: BTreeMap<String, Vec<u32>>,
}
#[test]
fn schemas_exist_on_empty_sources_and_describe_recursive_enums() {
    let source = Table::<Record>::new(Vec::new());
    let schema = source.schema();
    let json = serde_json::to_string(&schema).unwrap();
    assert!(
        json.contains("$ref") && json.contains("$defs"),
        "recursive node should use references: {json}"
    );
    assert!(json.contains("null") && json.contains("Running") && json.contains("progress"));
    assert!(
        !json.contains("secret"),
        "serde skip must apply to nested schemas"
    );
    assert!(source.snapshot(Limits::default()).unwrap().rows.is_empty());
}
#[test]
fn nested_values_and_enum_root_use_their_declared_serde_representation() {
    let row = Record {
        id: 1,
        node: Some(Node {
            name: "root".into(),
            children: Vec::new(),
            secret: "hidden".into(),
        }),
        state: Status::Running { progress: 42 },
        tags: BTreeMap::new(),
    };
    let _secret = &row.node.as_ref().unwrap().secret;
    let record = row.inspect().unwrap();
    assert_eq!(record["state"]["Running"]["progress"], 42);
    assert!(record["node"].get("secret").is_none());
    let source = Table::new(vec![Status::Idle, Status::Failed("network".into())]);
    let registry = Registry::new();
    let _guard = registry.expose("states", source).unwrap();
    let result = Query::parse("SELECT * FROM states")
        .unwrap()
        .execute(&registry, Limits::default())
        .unwrap();
    assert_eq!(result.rows[0]["value"], "Idle");
    assert_eq!(result.rows[1]["value"]["Failed"], "network");
}
#[derive(Debug, Inspect)]
struct Item {
    id: u64,
}
#[test]
fn adapters_preserve_collection_order_and_mutex_consistency() {
    let deque = CollectionSource::new(VecDeque::from([Item { id: 2 }, Item { id: 1 }]));
    assert_eq!(deque.snapshot(Limits::default()).unwrap().rows[0]["id"], 2);
    let map = CollectionSource::new(HashMap::from([(2, Item { id: 2 }), (1, Item { id: 1 })]));
    assert_eq!(map.snapshot(Limits::default()).unwrap().rows[0]["id"], 1);
    let lock = Arc::new(Mutex::new(BTreeMap::from([(1, Item { id: 1 })])));
    let source = CollectionSource::from_mutex(lock.clone());
    let held = lock.lock().unwrap();
    assert!(matches!(
        source.snapshot(Limits::default()),
        Err(livequery::Error::Busy)
    ));
    drop(held);
    source
        .update(|map| {
            map.insert(2, Item { id: 2 });
        })
        .unwrap();
    assert_eq!(source.snapshot(Limits::default()).unwrap().rows.len(), 2);
}
#[test]
fn publication_is_atomic_and_invalid_replacements_do_not_erase_data() {
    let limits = Limits::default();
    let published = Published::new::<Item>(limits).unwrap();
    let registry = Registry::new();
    let guard = registry
        .expose_published("items", published.clone())
        .unwrap();
    let first_revision = registry.revision("items").unwrap();
    published
        .publish(Snapshot::capture([&Item { id: 42 }], limits).unwrap())
        .unwrap();
    assert_ne!(first_revision, registry.revision("items").unwrap());
    assert!(
        published
            .publish(Snapshot {
                columns: Item::schema(),
                rows: vec![Default::default()]
            })
            .is_err()
    );
    assert_eq!(published.snapshot(limits).unwrap().rows[0]["id"], 42);
    let previous = registry.revision("items").unwrap();
    drop(guard);
    let _guard = registry.expose_published("items", published).unwrap();
    assert_ne!(
        previous,
        registry.revision("items").unwrap(),
        "a new registration must invalidate watch caches"
    );
}
#[test]
fn query_has_a_cooperative_deadline() {
    let registry = Registry::new();
    let _guard = registry
        .expose_published(
            "items",
            Published::from_snapshot(
                Snapshot::capture([&Item { id: 1 }], Limits::default()).unwrap(),
                Limits::default(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        Query::parse("SELECT * FROM items").unwrap().execute(
            &registry,
            Limits {
                query_timeout: std::time::Duration::ZERO,
                ..Limits::default()
            }
        ),
        Err(livequery::Error::Deadline)
    ));
}

#[test]
fn watch_does_not_reuse_revision_from_another_registry() {
    let limits = Limits::default();
    let first = Registry::new();
    let second = Registry::new();
    let publish = |id| {
        Published::from_snapshot(Snapshot::capture([&Item { id }], limits).unwrap(), limits)
            .unwrap()
    };
    let _first = first.expose_published("items", publish(1)).unwrap();
    let _second = second.expose_published("items", publish(2)).unwrap();
    let mut watch = livequery::Watch::new(Query::parse("SELECT * FROM items").unwrap());
    assert_eq!(
        watch.poll(&first, limits).unwrap().unwrap().rows[0]["id"],
        1
    );
    assert_eq!(
        watch.poll(&second, limits).unwrap().unwrap().rows[0]["id"],
        2
    );
}
