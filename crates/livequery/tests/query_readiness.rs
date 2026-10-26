use livequery::{Error, Inspect, Limits, Published, Query, Registry, Snapshot, Table};
use proptest::prelude::*;
use serde_json::{Value, json};

#[derive(Debug, Inspect)]
struct Item {
    id: u64,
    value: Option<i64>,
}

#[test]
fn detailed_query_budgets_metadata_without_reducing_legacy_result_budget() {
    let limits = Limits::default();
    let snapshot = Snapshot::capture(
        [&Item {
            id: 1,
            value: Some(2),
        }],
        limits,
    )
    .unwrap();
    let snapshot_bytes = serde_json::to_vec(&snapshot).unwrap().len();
    let published = Published::from_snapshot(snapshot, limits).unwrap();
    let registry = Registry::new();
    let _guard = registry.expose_published("items", published).unwrap();
    let query = Query::parse("SELECT * FROM items").unwrap();
    let plain = query.execute(&registry, limits).unwrap();
    let bytes = snapshot_bytes.max(serde_json::to_vec(&plain).unwrap().len());
    let detailed = query.execute_detailed(&registry, limits).unwrap();
    assert!(serde_json::to_vec(&detailed).unwrap().len() > bytes);
    let limits = Limits { bytes, ..limits };
    assert_eq!(query.execute(&registry, limits).unwrap(), plain);
    assert!(matches!(
        query.execute_detailed(&registry, limits),
        Err(Error::Limit(_))
    ));
}

#[test]
fn syntax_errors_identify_the_unexpected_token_or_end_of_input() {
    for (sql, location) in [
        ("SELECT id jobs", "line 1, column 11"),
        ("", "line 1, column 1"),
        ("  \n ", "line 2, column 2"),
        ("SELECT id FROM jobs; extra", "line 1, column 22"),
        ("SELECT id, id FROM jobs", "line 1, column 12"),
        ("SELECT id, id, value FROM jobs", "line 1, column 12"),
        ("SELECT id FROM jobs extra", "line 1, column 21"),
        ("SELECT id FROM jobs LIMIT nope", "line 1, column 27"),
        ("SELECT id\nFROM jobs\nLIMIT -1", "line 3, column 7"),
        ("SELECT id FROM", "line 1, column 15"),
        ("SELECT id FROM jobs\nWHERE ", "line 2, column 7"),
        ("SELECT COUNT(id) FROM jobs", "line 1, column 14"),
        ("SELECT MAX(id FROM jobs", "line 1, column 15"),
    ] {
        let message = Query::parse(sql).unwrap_err().to_string();
        assert!(message.ends_with(location), "{sql:?}: {message}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn ordering_and_aggregates_match_filtered_values(
        values in prop::collection::vec(prop::option::of(any::<i64>()), 0..80),
        cutoff in 0u64..80,
        limit in 0usize..80,
        descending in any::<bool>(),
    ) {
        let limits = Limits::default();
        let registry = Registry::new();
        let items = values.iter().enumerate().map(|(id, value)| Item { id: id as u64, value: *value }).collect();
        let _guard = registry.expose("items", Table::new(items)).unwrap();
        let direction = if descending { "DESC" } else { "ASC" };
        let result = Query::parse(&format!("SELECT id, value FROM items WHERE id >= {cutoff} ORDER BY value {direction} LIMIT {limit}"))
            .unwrap().execute(&registry, limits).unwrap();
        let mut expected: Vec<_> = values.iter().enumerate().filter(|(id, _)| *id as u64 >= cutoff).collect();
        expected.sort_by(|(_, left), (_, right)| match (left, right) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, _) => std::cmp::Ordering::Greater,
            (_, None) => std::cmp::Ordering::Less,
            (Some(left), Some(right)) => if descending { right.cmp(left) } else { left.cmp(right) },
        });
        prop_assert_eq!(result.scanned, values.len());
        prop_assert_eq!(result.truncated, expected.len() > limit);
        let ids: Vec<_> = result.rows.iter().map(|row| row["id"].as_u64().unwrap()).collect();
        prop_assert_eq!(ids, expected.iter().take(limit).map(|(id, _)| *id as u64).collect::<Vec<_>>());
        for (sql, field, expected_value) in [
            (format!("SELECT COUNT(*) FROM items WHERE id >= {cutoff}"), "count", json!(expected.len())),
            (format!("SELECT MAX(value) FROM items WHERE id >= {cutoff}"), "max", expected.iter().filter_map(|(_, value)| **value).max().map_or(Value::Null, Value::from)),
        ] {
            let result = Query::parse(&sql).unwrap().execute(&registry, limits).unwrap();
            prop_assert_eq!(result.scanned, values.len());
            prop_assert_eq!(&result.rows[0][field], &expected_value);
        }
    }
}
