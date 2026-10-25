use bevy_app::App;
use bevy_ecs::{component::Component, reflect::ReflectComponent};
use bevy_reflect::{Reflect, Struct};
use bevy_remote::{RemotePlugin, http::RemoteHttpPlugin};
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
struct Player {
    id: u64,
    health: u32,
}
fn main() {
    bevy_tasks::IoTaskPool::get_or_init(bevy_tasks::TaskPool::new);
    let mut player = Player { id: 42, health: 12 };
    assert_eq!(
        player.field("health").unwrap().try_downcast_ref::<u32>(),
        Some(&12)
    );
    *player
        .field_mut("health")
        .unwrap()
        .try_downcast_mut::<u32>()
        .unwrap() = 20;
    assert_eq!(player.health, 20);
    let port = std::env::args().nth(1).unwrap().parse::<u16>().unwrap();
    let mut app = App::new();
    app.register_type::<Player>();
    app.add_plugins((
        RemotePlugin::default(),
        RemoteHttpPlugin::default().with_port(port),
    ));
    app.world_mut().spawn(player);
    app.finish();
    app.cleanup();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
