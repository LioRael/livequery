use lq::{Inspect, Limits, Query, Registry, Table};
#[derive(Debug)]
struct Secret;
#[derive(Debug, Inspect)]
struct Record<T> {
    id: u64,
    value: T,
    #[inspect(skip)]
    secret: Secret,
}
fn main() -> Result<(), lq::Error> {
    let row = Record { id: 42, value: "external", secret: Secret };
    let _secret = &row.secret;
    let registry = Registry::new();
    let _guard = registry.expose("records", Table::new(vec![row]))?;
    let result = Query::parse("SELECT value FROM records WHERE id = 42")?.execute(&registry, Limits::default())?;
    assert_eq!(result.rows[0]["value"], "external");
    assert_eq!(result.columns.len(), 1);
    Ok(())
}
