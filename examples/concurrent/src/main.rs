use livequery::{
    Inspect, Limits, Published, Registry, Table,
    local::{self, Server},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
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
    let published = Published::new::<Player>(Limits::default())?;
    published.refresh(&players)?;
    let _registration = registry.expose_published("players", published.clone())?;
    let path = local::directory().join(format!("{}.sock", std::process::id()));
    let server = Server::bind(&path, registry, Limits::default())?;
    let stop = Arc::new(AtomicBool::new(false));
    let agent = {
        let stop = stop.clone();
        std::thread::spawn(move || server.serve(&stop))
    };
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
        published.refresh(&players)?;
        std::thread::sleep(Duration::from_millis(250));
    }
    stop.store(true, Ordering::Relaxed);
    agent.join().map_err(|_| "agent thread panicked")??;
    Ok(())
}
