//! Performance sanity check: 4 players + their asteroids, measure tick time.
//! Run with `cargo run --example perf` (debug build, matches the game server).

use server::sim::world::World;

fn main() {
    let mut world = World::new(1);

    for i in 0..4 {
        let name = format!("player{i}");
        let id = world.spawn_starter_ship(name);
        // Spread them out so their asteroid fields don't overlap chunks too much.
        if let Some(ship) = world.ships.get_mut(&id) {
            ship.pos[0] += i as f32 * 2000.0;
        }
    }

    // Let asteroids spawn in to steady state (ASTEROIDS_PER_PLAYER each).
    for _ in 0..200 {
        world.step();
    }

    let asteroids = world.ships.values().filter(|s| s.owner.is_none()).count();
    println!("ships: {} (asteroids: {asteroids})", world.ships.len());

    const N: u32 = 200;
    let start = std::time::Instant::now();
    for _ in 0..N {
        world.step();
    }
    let elapsed = start.elapsed();
    println!("avg tick time over {N} ticks: {:.3} ms", elapsed.as_secs_f64() * 1000.0 / N as f64);
}
