use console_api::instrument::{InstrumentRequest, instrument_client::InstrumentClient};
use tracing_subscriber::prelude::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let (layer, server) = console_subscriber::ConsoleLayer::builder()
        .server_addr(address)
        .build();
    tracing_subscriber::registry().with(layer).init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let server = tokio::spawn(server.serve());
        let work = tokio::spawn(async {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });
        let mut client = None;
        for _ in 0..100 {
            if let Ok(connected) = InstrumentClient::connect(format!("http://{address}")).await {
                client = Some(connected);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let mut stream = client
            .expect("console server starts")
            .watch_updates(InstrumentRequest {})
            .await
            .unwrap()
            .into_inner();
        let count = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Some(update) = stream.message().await.unwrap() {
                if let Some(tasks) = update.task_update {
                    if !tasks.new_tasks.is_empty() {
                        return tasks.new_tasks.len();
                    }
                }
            }
            0
        })
        .await
        .expect("task telemetry arrives");
        assert!(count > 0);
        println!("console gRPC task telemetry received: {count} tasks");
        work.abort();
        server.abort();
    });
    Ok(())
}
