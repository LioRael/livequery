mod session;
use livequery_query::QueryResult;
use livequery_transport::{Observation, PROTOCOL_VERSION, Request, Response, local};
use std::{
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};
fn main() {
    if let Err(error) = run() {
        eprintln!("livequery: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_args(std::env::args().skip(1).collect())
}
fn run_args(mut args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let json = remove_flag(&mut args, "--json");
    let socket = option(&mut args, "--socket")?.map(PathBuf::from);
    let pid = option(&mut args, "--pid")?
        .map(|p| p.parse::<u32>())
        .transpose()?;
    let interval = option(&mut args, "--interval-ms")?
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(1000);
    let count = option(&mut args, "--count")?
        .map(|v| v.parse::<usize>())
        .transpose()?;
    let command = args.first().map(String::as_str).unwrap_or("help");
    if command == "help" || command == "--help" {
        println!(
            "LiveQuery — explore an explicitly enabled Rust program\n\n  livequery list [--json]\n  livequery attach <pid>\n  livequery session --pid <pid>\n  livequery status --pid <pid> [--json]\n  livequery schema --pid <pid>\n  livequery query 'SELECT * FROM players' --pid <pid> [--json]\n  livequery watch 'SELECT * FROM players' --pid <pid> [--interval-ms 1000] [--count 10]\n\nUse --socket <path> instead of --pid. WATCH emits full changed results; --count counts polls."
        );
        return Ok(());
    }
    if command == "list" {
        if args.len() != 1 {
            return Err("unexpected arguments".into());
        }
        let sockets = local::discover()?;
        for socket in sockets {
            if let Ok(Response::Ok { .. }) = local::request(&socket, &Request::Schema) {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"socket":socket,"pid":socket.file_stem().and_then(|s| s.to_str())})
                    );
                } else {
                    println!(
                        "{}  {}",
                        socket.file_stem().unwrap_or_default().to_string_lossy(),
                        socket.display()
                    );
                }
            }
        }
        return Ok(());
    }
    let attach_pid = if command == "attach" {
        Some(args.get(1).ok_or("attach requires a PID")?.parse::<u32>()?)
    } else {
        None
    };
    let socket = socket
        .or_else(|| {
            pid.or(attach_pid)
                .map(|id| local::directory().join(format!("{id}.sock")))
        })
        .ok_or("specify --pid or --socket; target must enable LiveQuery")?;
    if command == "session" {
        if args.len() != 1 {
            return Err("unexpected session arguments".into());
        }
        return session::run(&socket);
    }
    let mut request = match command {
        "schema" | "attach" => Request::Schema,
        "status" => Request::Status,
        "query" | "watch" => Request::Query {
            sql: args.get(1).ok_or("query requires SQL")?.clone(),
        },
        _ => return Err("unknown command; use --help".into()),
    };
    let expected = if matches!(command, "schema" | "status") {
        1
    } else {
        2
    };
    if args.len() != expected {
        return Err("unexpected arguments".into());
    }
    if !(100..=60_000).contains(&interval) {
        return Err("interval must be 100..60000 ms".into());
    }
    match local::request(
        &socket,
        &Request::Hello {
            protocol: PROTOCOL_VERSION,
        },
    )? {
        Response::Ok { data } if data["protocol"].as_u64() == Some(PROTOCOL_VERSION as u64) => {
            if command == "query"
                && data["capabilities"]
                    .as_array()
                    .is_some_and(|caps| caps.iter().any(|cap| cap == "query_details"))
            {
                request = Request::QueryDetails {
                    sql: args[1].clone(),
                };
            }
        }
        _ => return Err("incompatible agent protocol; use a matching CLI/Agent version".into()),
    }
    let mut previous = None;
    let mut revision = None;
    let mut polls = 0;
    let mut connection_failures = 0;
    let mut freshness: Option<(std::time::Instant, Option<u64>)> = None;
    loop {
        if command == "watch" && count.is_some_and(|c| polls >= c) {
            break;
        }
        let active_request = if command == "watch" {
            Request::ObserveV2 {
                sql: args[1].clone(),
                since: revision.clone(),
            }
        } else {
            request.clone()
        };
        let response = match local::request(&socket, &active_request) {
            Ok(response) => {
                connection_failures = 0;
                response
            }
            Err(error) if command == "watch" && error.code() == "io" && connection_failures < 3 => {
                connection_failures += 1;
                revision = None;
                previous = None;
                polls += 1;
                eprintln!("agent connection interrupted; retry {connection_failures}/3: {error}");
                std::thread::sleep(Duration::from_millis(interval));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let data = match response {
            Response::Ok { data } if command == "watch" => {
                match serde_json::from_value::<Observation>(data)? {
                    Observation::Changed { cursor, result } => {
                        revision = Some(cursor);
                        result
                    }
                    Observation::Unchanged { cursor } => {
                        if !json {
                            eprintln!("unchanged · publication {}", cursor.revision.sequence());
                            show_age(freshness);
                        }
                        revision = Some(cursor);
                        polls += 1;
                        std::thread::sleep(Duration::from_millis(interval));
                        continue;
                    }
                    _ => return Err("unsupported observation payload".into()),
                }
            }
            Response::Ok { data } => data,
            Response::Error { code, .. } if command == "watch" && code == "busy" => {
                eprintln!("observation failed: busy; retaining previous result");
                polls += 1;
                std::thread::sleep(Duration::from_millis(interval));
                continue;
            }
            Response::Error { code, message } => return Err(format!("{code}: {message}").into()),
            _ => return Err("unexpected response for negotiated protocol".into()),
        };
        if matches!(command, "query" | "watch") {
            if let Some(publication) = data.get("publication").filter(|p| !p.is_null()) {
                freshness = Some((std::time::Instant::now(), publication["age_ms"].as_u64()));
                if !json {
                    show_age(freshness);
                }
            } else if !json {
                eprintln!("publication freshness unavailable from this Agent");
            }
        }
        let mut comparable = data.clone();
        if let Some(object) = comparable.as_object_mut() {
            object.remove("scanned");
            object.remove("publication");
            object.remove("revision");
        }
        if previous.as_ref() != Some(&comparable) {
            if json {
                println!("{}", serde_json::to_string(&data)?);
            } else if matches!(request, Request::Schema | Request::Status) {
                println!("{}", serde_json::to_string_pretty(&data)?);
            } else {
                table(&serde_json::from_value::<QueryResult>(data)?)?;
            }
            previous = Some(comparable);
        } else if command == "watch" && !json {
            eprintln!("rows unchanged in a new publication");
        }
        polls += 1;
        if command != "watch" {
            break;
        }
        std::thread::sleep(Duration::from_millis(interval));
    }
    Ok(())
}
fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(i) = args.iter().position(|s| s == flag) {
        args.remove(i);
        true
    } else {
        false
    }
}
fn option(
    args: &mut Vec<String>,
    flag: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if let Some(i) = args.iter().position(|s| s == flag) {
        args.remove(i);
        if i >= args.len() {
            return Err(format!("{flag} needs a value").into());
        }
        Ok(Some(args.remove(i)))
    } else {
        Ok(None)
    }
}
fn table(result: &QueryResult) -> io::Result<()> {
    // JSON quoting escapes terminal control characters in untrusted field values.
    let headers: Vec<_> = result.columns.iter().map(|c| c.name.clone()).collect();
    let rows: Vec<Vec<String>> = result
        .rows
        .iter()
        .map(|row| headers.iter().map(|h| row[h].to_string()).collect())
        .collect();
    let widths: Vec<_> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            rows.iter()
                .map(|r| r[i].chars().count())
                .max()
                .unwrap_or(0)
                .max(h.len())
                .min(60)
        })
        .collect();
    let mut out = io::stdout().lock();
    for row in std::iter::once(&headers).chain(rows.iter()) {
        for (cell, width) in row.iter().zip(&widths) {
            let cell: String = cell.chars().take(*width).collect();
            write!(out, "{cell:width$}  ")?;
        }
        writeln!(out)?;
    }
    writeln!(
        out,
        "{} rows · {} scanned{}",
        result.rows.len(),
        result.scanned,
        if result.truncated { " · limited" } else { "" }
    )
}

fn show_age(freshness: Option<(std::time::Instant, Option<u64>)>) {
    match freshness {
        Some((time, Some(age))) => eprintln!(
            "snapshot age: {} ms",
            u128::from(age) + time.elapsed().as_millis()
        ),
        Some((_, None)) => eprintln!("snapshot: never successfully published"),
        None => eprintln!("snapshot age: unavailable"),
    }
}
