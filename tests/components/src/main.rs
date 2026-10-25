use state::{Inspect, Limits, Published, Registry, Table};
#[derive(Debug, Inspect)]
struct Item<T> {
    id: T,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new();
    let items = Table::new(vec![Item { id: 42_u64 }]);
    let _local = registry.expose("local_items", items.clone())?;
    let published = Published::new::<Item<u64>>(Limits::default())?;
    published.refresh(&items)?;
    let _public = registry.expose_published("items", published)?;
    let result =
        query::Query::parse("SELECT id FROM items")?.execute(&registry, Limits::default())?;
    assert_eq!(result.rows[0]["id"], 42);
    let agent = agent::Agent::new(registry, Limits::default());
    match agent.handle(wire::Request::Schema)? {
        wire::Response::Ok { data } => {
            assert!(data.get("items").is_some());
            assert!(data.get("local_items").is_none());
        }
        other => panic!("unexpected response: {other:?}"),
    }
    assert!(matches!(
        agent.handle(wire::Request::Query {
            sql: "SELECT * FROM local_items".into()
        }),
        Err(state::Error::NotFound(_))
    ));
    let response = agent.handle(wire::Request::Observe {
        sql: "SELECT * FROM items".into(),
        since: None,
    })?;
    let wire::Response::Observation { revision, data } = response else {
        panic!("expected observation")
    };
    assert_eq!(data["rows"][0]["id"], 42);
    assert!(matches!(
        agent.handle(wire::Request::Observe {
            sql: "SELECT * FROM items".into(),
            since: Some(revision)
        })?,
        wire::Response::NotModified { .. }
    ));
    println!("standalone renamed Core, Query, Agent and Transport: PASS");
    Ok(())
}
