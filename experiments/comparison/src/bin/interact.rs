extern crate interact;
#[derive(interact::Interact)]
struct Player {
    id: u64,
    health: u32,
}
fn main() {
    let mut root = interact::RootSend::new();
    root.owned
        .insert("players", Box::new(vec![Player { id: 42, health: 12 }]));
    let (value, _) = root.as_root().access("players[0].health");
    assert!(value.is_ok(), "field lookup: {value:?}");
    println!("field lookup: {value:?}");
    let (write, _) = root.as_root().access("players[0].health = 20");
    assert!(write.is_ok(), "assignment: {write:?}");
    let (changed, _) = root.as_root().access("players[0].health");
    println!("after assignment: {changed:?}");
}
