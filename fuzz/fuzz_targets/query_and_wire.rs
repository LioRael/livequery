#![no_main]
use libfuzzer_sys::fuzz_target;
use livequery::{Inspect, Limits, Query, Registry, Table};
#[derive(Debug, Inspect)]
struct Record { id: u64, name: Option<String>, active: bool }
fuzz_target!(|data: &[u8]| {
    if data.len() > 8192 { return; }
    let _: Result<livequery_transport::Request, _> = serde_json::from_slice(data);
    let records: Vec<_> = data.chunks(8).take(32).enumerate().map(|(i, bytes)| {
        let mut integer = [0; 8];
        integer[..bytes.len()].copy_from_slice(bytes);
        Record { id: u64::from_le_bytes(integer), name: (i % 3 != 0).then(|| String::from_utf8_lossy(bytes).into_owned()), active: bytes[0] % 2 == 0 }
    }).collect();
    let expected_count = records.iter().filter(|row| row.active).count();
    let expected_max = records.iter().map(|row| row.id).max();
    let registry = Registry::new();
    let _guard = registry.expose("items", Table::new(records)).unwrap();
    let limits = Limits::default();
    let execute = |sql| match Query::parse(sql).unwrap().execute(&registry, limits) {
        Ok(result) => Some(result),
        Err(livequery::Error::Deadline) => None,
        Err(error) => panic!("generated valid query failed: {error}"),
    };
    let Some(count) = execute("SELECT COUNT(*) FROM items WHERE active = TRUE") else { return; };
    assert_eq!(count.rows[0]["count"], serde_json::json!(expected_count));
    let Some(maximum) = execute("SELECT MAX(id) FROM items") else { return; };
    assert_eq!(maximum.rows[0]["max"], serde_json::json!(expected_max));
    let Some(sorted) = execute("SELECT id FROM items ORDER BY id DESC LIMIT 20") else { return; };
    assert!(sorted.rows.windows(2).all(|pair| pair[0]["id"].as_u64() >= pair[1]["id"].as_u64()));
    if let Ok(sql) = std::str::from_utf8(data) {
        if let Ok(query) = Query::parse(sql) {
            if let Ok(result) = query.execute_detailed(&registry, limits) {
                assert!(result.result.rows.len() <= limits.result_rows);
                assert!(serde_json::to_vec(&result).unwrap().len() <= limits.bytes);
            }
        }
    }
});
