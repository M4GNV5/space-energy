//! Deterministic suns, asteroid spawning, and the starter ship layout.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::protocol::{Cell, Material, ShipId};
use crate::sim::consts;
use crate::sim::ship::{Block, Ship};
use crate::sim::world::{Sun, World};

/// Deterministic hash of (seed, cx, cy) -> a fresh 64-bit seed for that chunk.
fn hash_chunk(seed: u64, cx: i32, cy: i32) -> u64 {
    let mut h = seed;
    h ^= (cx as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = h.rotate_left(31);
    h ^= (cy as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    h
}

/// The sun in chunk (cx,cy), if any. Deterministic given the world seed.
/// Chunk (0,0) always has one.
pub fn sun_in_chunk(seed: u64, cx: i32, cy: i32) -> Option<Sun> {
    let mut rng = ChaCha8Rng::seed_from_u64(hash_chunk(seed, cx, cy));
    let present = (cx == 0 && cy == 0) || rng.random_bool(consts::SUN_CHANCE);
    if !present {
        return None;
    }
    let radius = rng.random_range(consts::SUN_RADIUS_MIN..=consts::SUN_RADIUS_MAX);
    let margin = consts::SUN_RADIUS_MAX;
    let ox = cx as f32 * consts::CHUNK_SIZE;
    let oy = cy as f32 * consts::CHUNK_SIZE;
    let x = ox + rng.random_range(margin..(consts::CHUNK_SIZE - margin));
    let y = oy + rng.random_range(margin..(consts::CHUNK_SIZE - margin));
    Some(Sun { x, y, radius })
}

const STARTER_LAYOUT: [(Cell, Material, f32); 9] = [
    ([-1, 1], Material::Lead, 3000.0),
    ([0, 1], Material::Iron, 500.0),
    ([1, 1], Material::Lead, 3000.0),
    ([-1, 0], Material::Copper, 500.0),
    ([0, 0], Material::Uranium, 1000.0),
    ([1, 0], Material::Copper, 500.0),
    ([-1, -1], Material::Lead, 3000.0),
    ([0, -1], Material::Tungsten, 1000.0),
    ([1, -1], Material::Lead, 3000.0),
];

/// Builds and places a starter ship for `owner`, inserts it into the world,
/// and returns its id.
pub fn make_starter_ship(world: &mut World, owner: String) -> ShipId {
    let id = world.next_ship_id;
    world.next_ship_id += 1;

    let mut ship = Ship::new(id, Some(owner));
    for (cell, material, mass) in STARTER_LAYOUT {
        let cap = consts::props(material).energy_capacity;
        ship.blocks.insert(cell, Block { material, mass, energy: cap * 0.1 });
    }
    ship.recompute_com_inertia();

    // Random point near the origin, then the sun nearest to it.
    let angle: f32 = world.rng.random_range(0.0..std::f32::consts::TAU);
    let r: f32 = world.rng.random_range(0.0..consts::SPAWN_NEAR_ORIGIN_RADIUS);
    let point = [r * angle.cos(), r * angle.sin()];
    // 2 chunks covers chunk (0,0)'s diagonal from any point near the origin.
    let candidates = world.suns_in_view(point, 2.0 * consts::CHUNK_SIZE);
    let sun = candidates
        .into_iter()
        .min_by(|a, b| {
            let da = crate::sim::ship::dist([a.x, a.y], point);
            let db = crate::sim::ship::dist([b.x, b.y], point);
            da.partial_cmp(&db).unwrap()
        })
        .expect("chunk (0,0) always has a sun");

    let spawn_angle: f32 = world.rng.random_range(0.0..std::f32::consts::TAU);
    let jitter: f32 = world.rng.random_range(0.0..consts::SPAWN_JITTER);
    let d = sun.radius + consts::SPAWN_SUN_DISTANCE + jitter;
    ship.pos = [sun.x + d * spawn_angle.cos(), sun.y + d * spawn_angle.sin()];
    // Face away from the sun (local +y = world spawn_angle), so forward thrust doesn't dive into it.
    ship.rot = spawn_angle - std::f32::consts::FRAC_PI_2;
    ship.vel = [0.0, 0.0];
    ship.omega = 0.0;

    world.ships.insert(id, ship);
    id
}

/// Grows a random 4-connected blob of `n` cells starting at (0,0).
fn random_blob(rng: &mut ChaCha8Rng, n: usize) -> Vec<Cell> {
    let mut cells: Vec<Cell> = vec![[0, 0]];
    let dirs = [[1, 0], [-1, 0], [0, 1], [0, -1]];
    let mut guard = 0;
    while cells.len() < n && guard < 1000 {
        guard += 1;
        let base = cells[rng.random_range(0..cells.len())];
        let d = dirs[rng.random_range(0..4)];
        let cand = [base[0] + d[0], base[1] + d[1]];
        if !cells.contains(&cand) {
            cells.push(cand);
        }
    }
    cells
}

const ASTEROID_MATERIALS: [Material; 6] =
    [Material::Iron, Material::Copper, Material::Lead, Material::Plastic, Material::Tungsten, Material::Uranium];

/// Builds an unowned asteroid ship at `pos` with random blocks/mass/velocity/spin.
pub fn build_asteroid(rng: &mut ChaCha8Rng, id: ShipId, pos: [f32; 2]) -> Ship {
    let n = rng.random_range(consts::ASTEROID_BLOCKS_MIN..=consts::ASTEROID_BLOCKS_MAX);
    let cells = random_blob(rng, n);

    let mut ship = Ship::new(id, None);
    for cell in cells {
        let material = ASTEROID_MATERIALS[rng.random_range(0..ASTEROID_MATERIALS.len())];
        let max_mass = consts::props(material).max_mass;
        let frac: f32 = rng.random_range(0.2..=1.0);
        ship.blocks.insert(cell, Block { material, mass: max_mass * frac, energy: 0.0 });
    }
    ship.recompute_com_inertia();

    ship.pos = pos;
    ship.rot = rng.random_range(0.0..std::f32::consts::TAU);
    let speed: f32 = rng.random_range(0.0..=consts::ASTEROID_SPEED_MAX);
    let vel_angle: f32 = rng.random_range(0.0..std::f32::consts::TAU);
    ship.vel = [speed * vel_angle.cos(), speed * vel_angle.sin()];
    ship.omega = rng.random_range(-consts::ASTEROID_SPIN_MAX..consts::ASTEROID_SPIN_MAX);

    ship
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_suns() {
        for (cx, cy) in [(0, 0), (3, -2), (-7, 5), (100, 100)] {
            let a = sun_in_chunk(42, cx, cy);
            let b = sun_in_chunk(42, cx, cy);
            match (a, b) {
                (Some(sa), Some(sb)) => {
                    assert_eq!(sa.x, sb.x);
                    assert_eq!(sa.y, sb.y);
                    assert_eq!(sa.radius, sb.radius);
                }
                (None, None) => {}
                _ => panic!("same seed produced different presence at ({cx},{cy})"),
            }
        }
    }

    #[test]
    fn starter_ship_spawns_for_any_seed() {
        for seed in 0..200u64 {
            let mut world = World::new(seed);
            let id = make_starter_ship(&mut world, "p".into());
            assert!(world.ships.contains_key(&id));
        }
    }

    #[test]
    fn origin_chunk_always_has_a_sun() {
        for seed in [0u64, 1, 42, 999_999] {
            assert!(sun_in_chunk(seed, 0, 0).is_some());
        }
    }
}
