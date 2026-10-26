use livequery::{
    Inspect, Limits, PublishedHandle, Registry, Table,
    local::{self, RunningAgent},
};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
#[derive(Debug, Inspect)]
struct Requests {
    completed: u64,
    bytes: u64,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stats = Table::new(vec![Requests {
        completed: 0,
        bytes: 0,
    }]);
    let registry = Registry::new();
    let published = Arc::new(PublishedHandle::register::<Requests>(
        &registry,
        "requests",
        Limits::default(),
    )?);
    let agent = RunningAgent::start(
        local::directory().join(format!("{}.sock", std::process::id())),
        registry,
        Limits::default(),
    )?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "echo {} · LiveQuery PID {}",
        listener.local_addr()?,
        std::process::id()
    );
    let seconds = std::env::var("LIVEQUERY_DEMO_SECONDS")
        .ok()
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(30);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while let Ok(accepted) = tokio::time::timeout_at(deadline, listener.accept()).await {
        let (mut stream, _) = accepted?;
        let stats = stats.clone();
        let published = published.clone();
        tokio::spawn(async move {
            let mut buffer = [0; 4096];
            if let Ok(Ok(n)) =
                tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buffer))
                    .await
                && stream.write_all(&buffer[..n]).await.is_ok()
            {
                loop {
                    if stats
                        .update(|rows| {
                            rows[0].completed += 1;
                            rows[0].bytes += n as u64;
                        })
                        .is_ok()
                    {
                        match tokio::task::spawn_blocking(move || published.refresh(&stats)).await {
                            Ok(Ok(_)) => {}
                            Ok(Err(error)) => eprintln!("diagnostics stale: {error}"),
                            Err(error) => eprintln!("diagnostic task failed: {error}"),
                        }
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            }
        });
    }
    agent.stop();
    tokio::task::spawn_blocking(move || agent.wait()).await??;
    Ok(())
}
