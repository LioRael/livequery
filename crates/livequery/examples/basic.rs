use livequery::{
    Inspect, Limits, Registry, Table,
    local::{self, Server},
};
#[derive(Debug, Inspect)]
struct Player {
    id: u64,
    username: String,
    health: u32,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new();
    let _players = registry.expose(
        "players",
        Table::new(vec![Player {
            id: 1,
            username: "Ada".into(),
            health: 18,
        }]),
    )?;
    let path = local::directory().join(format!("{}.sock", std::process::id()));
    let server = Server::bind(&path, registry, Limits::default())?;
    println!(
        "LiveQuery enabled: livequery query 'SELECT * FROM players' --pid {}",
        std::process::id()
    );
    server.serve(&std::sync::atomic::AtomicBool::new(false))?;
    Ok(())
}
