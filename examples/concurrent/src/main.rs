use livequery::{
    Inspect, Limits, PublishedHandle, Registry, Table,
    local::{self, RunningAgent},
};
use std::time::Duration;
#[derive(Debug, Inspect)]
struct Player {
    id: u64,
    username: String,
    health: u32,
    #[inspect(skip)]
    token: String,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let players = Table::new(vec![
        Player {
            id: 1,
            username: "Ada".into(),
            health: 100,
            token: "never exposed".into(),
        },
        Player {
            id: 2,
            username: "Lin".into(),
            health: 50,
            token: String::new(),
        },
    ]);
    let registry = Registry::new();
    let published = PublishedHandle::register::<Player>(&registry, "players", Limits::default())?;
    if let Err(error) = published.refresh(&players) {
        eprintln!("diagnostics unavailable: {error}");
    }
    let path = local::directory().join(format!("{}.sock", std::process::id()));
    let agent = RunningAgent::start(&path, registry, Limits::default())?;
    println!(
        "PID {} · livequery watch 'SELECT * FROM players WHERE health < 80' --pid {}",
        std::process::id(),
        std::process::id()
    );
    let ticks = std::env::var("LIVEQUERY_DEMO_TICKS")
        .ok()
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(120);
    for _ in 0..ticks {
        let _ = players.update(|rows| {
            for player in rows {
                player.health = player.health.saturating_sub(7);
                if player.health == 0 {
                    player.health = 100;
                }
                std::hint::black_box(&player.token);
            }
        });
        if let Err(error) = published.refresh(&players) {
            eprintln!("diagnostics stale: {error}");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    agent.stop();
    agent.wait()?;
    Ok(())
}
