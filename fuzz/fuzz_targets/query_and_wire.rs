#![no_main]
use libfuzzer_sys::fuzz_target;
use livequery::{Inspect, Limits, Query, Registry, Table};
#[derive(Debug, Inspect)]
struct Record { id: u64, name: String, active: bool }
fuzz_target!(|data: &[u8]| {
    if data.len() > 8192 { return; }
    let _: Result<livequery_transport::Request, _> = serde_json::from_slice(data);
    if let Ok(sql) = std::str::from_utf8(data) {
        if let Ok(query) = Query::parse(sql) {
            let registry = Registry::new();
            let _guard = registry.expose("items", Table::new(vec![Record { id: 42, name: "Ada".into(), active: true }])).unwrap();
            if let Ok(result) = query.execute(&registry, Limits::default()) {
                assert!(result.rows.len() <= Limits::default().result_rows);
                assert!(serde_json::to_vec(&result).unwrap().len() <= Limits::default().bytes);
            }
        }
    }
});
