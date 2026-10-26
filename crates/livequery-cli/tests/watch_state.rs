#![cfg(unix)]

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn cursor(session: &str) -> Value {
    json!({"session": session, "query": "query-id", "revision": {"registration": 1, "sequence": 1}})
}

fn changed(session: &str, metadata: bool) -> Value {
    let mut result = json!({
        "columns": [{"name": "id", "rust_type": "u64"}],
        "rows": [{"id": 42}], "scanned": 1, "truncated": false
    });
    if metadata {
        result["publication"] = json!({"age_ms": 123456});
    }
    json!({"status": "ok", "data": {"kind": "changed", "cursor": cursor(session), "result": result}})
}

fn unchanged(session: &str) -> Value {
    json!({"status": "ok", "data": {"kind": "unchanged", "cursor": cursor(session)}})
}

// None closes an accepted request without a response, reproducing a restart's EOF.
fn watch(script: Vec<(Option<&'static str>, Option<Value>)>, json_output: bool) -> Output {
    let directory = tempfile::Builder::new().prefix("lqw-").tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.path().join("a.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let polls = script.len();
    let server = std::thread::spawn(move || {
        let mut steps = vec![(None, Some(json!({"status": "ok", "data": {"protocol": 2}})))];
        steps.extend(script);
        for (index, (expected_session, response)) in steps.into_iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "CLI did not send request {index}"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("fixture accept failed: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream).read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            if index == 0 {
                assert_eq!(request["command"], "hello");
            } else {
                assert_eq!(request["command"], "observe_v2");
                assert_eq!(request["since"]["session"].as_str(), expected_session);
            }
            if let Some(response) = response {
                writeln!(stream, "{response}").unwrap();
            }
        }
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_livequery"));
    command
        .args(["watch", "SELECT id FROM items", "--socket"])
        .arg(&path)
        .args(["--interval-ms", "100", "--count", &polls.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if json_output {
        command.arg("--json");
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
    }
    let output = child.wait_with_output().unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn new_session_emits_identical_rows_without_an_io_failure() {
    let output = watch(
        vec![
            (None, Some(changed("first", true))),
            (Some("first"), Some(changed("second", true))),
            (Some("second"), Some(unchanged("second"))),
        ],
        true,
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.lines().count(),
        2,
        "a new Agent must emit its first result even when rows match"
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("retry"));
}

#[test]
fn missing_metadata_clears_previous_snapshot_age() {
    let output = watch(
        vec![
            (None, Some(changed("first", true))),
            (Some("first"), Some(changed("first", false))),
            (Some("first"), Some(unchanged("first"))),
        ],
        false,
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("publication freshness unavailable"));
    assert!(stderr.contains("snapshot age: unavailable"), "{stderr}");
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.starts_with("snapshot age:") && line.ends_with(" ms"))
            .count(),
        1,
        "{stderr}"
    );
}

#[test]
fn interrupted_request_clears_cursor_and_result_before_retry() {
    let output = watch(
        vec![
            (None, Some(changed("first", true))),
            (Some("first"), None),
            (None, Some(changed("second", false))),
            (Some("second"), Some(unchanged("second"))),
        ],
        false,
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("retry 1/3"), "{stderr}");
    assert!(stderr.contains("snapshot age: unavailable"), "{stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.matches("1 rows").count(),
        2,
        "retry must emit a new baseline"
    );
}
