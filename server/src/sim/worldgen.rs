//! Deterministic suns, asteroid spawning, and the starter ship layout.

use std::collections::HashSet;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::protocol::{Cell, Dir, Material, ShipId};
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

/// Starter ship, nose up (+y). One char per cell: columns are x = -4..=4, the
/// first row is y = `STARTER_TOP_Y`. See `starter_block` for the legend.
///
/// The uranium reactor sits in a plastic shell; its only outlets are the two
/// silicon blocks, pointing at the copper spine (north, to the tungsten battery
/// and the laser at the nose tip) and at the copper bus (south, to the engines).
/// Plastic also lines the spine and the bus, so the iron hull only touches the
/// power circuits at the nose and at the lead thrusters.
///
/// The laser and the copper block feeding it are heavy: conduction equalises
/// fill ratios, so their mass sets how much energy the laser holds and how
/// much the feed passes on per tick.
///
/// The default key binds in `client/src/binds.ts` refer to these cells.
const STARTER_LAYOUT: [&str; 14] = [
    "....A....", // 10  laser
    "..LCKCL..", //  9  laser feed
    "..IPTPI..", //  8  battery
    "..IPTPI..", //  7
    "..IPCPI..", //  6
    "..IPCPI..", //  5
    "..lCCCl..", //  4  front thrusters
    ".IIPCPII.", //  3
    ".IPP^PPI.", //  2  reactor outlet to the spine
    ".IPPUPPI.", //  1  reactor
    ".IPPvPPI.", //  0  reactor outlet to the engines
    "IIPPCPPII", // -1
    "IPCCCCCPI", // -2  engine bus
    "IILLLLLII", // -3  main engines
];
const STARTER_TOP_Y: i32 = 10;

/// Starter ship legend: (material, mass in kg, energy fill ratio, silicon direction).
fn starter_block(ch: char) -> Option<(Material, f32, f32, Option<Dir>)> {
    Some(match ch {
        'I' => (Material::Iron, 300.0, 0.1, None),
        'P' => (Material::Plastic, 100.0, 0.1, None),
        'C' => (Material::Copper, 300.0, 0.1, None),
        'A' => (Material::Iron, 1500.0, 0.1, None),
        'K' => (Material::Copper, 1500.0, 0.1, None),
        'T' => (Material::Tungsten, 1500.0, 0.3, None),
        'U' => (Material::Uranium, 1000.0, 0.1, None),
        '^' => (Material::Silicon, 200.0, 0.1, Some(Dir::N)),
        'v' => (Material::Silicon, 200.0, 0.1, Some(Dir::S)),
        'L' => (Material::Lead, 4000.0, 0.5, None),
        'l' => (Material::Lead, 2000.0, 0.5, None),
        _ => return None,
    })
}

/// Builds and places a starter ship for `owner`, inserts it into the world,
/// and returns its id.
pub fn make_starter_ship(world: &mut World, owner: String) -> ShipId {
    let id = world.next_ship_id;
    world.next_ship_id += 1;

    let mut ship = Ship::new(id, Some(owner));
    for (row, line) in STARTER_LAYOUT.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let Some((material, mass, fill, dir)) = starter_block(ch) else { continue };
            let cell = [col as i32 - 4, STARTER_TOP_Y - row as i32];
            let mut block = Block::new(material, mass, 0.0);
            block.energy = block.energy_capacity() * fill;
            block.dir = dir;
            ship.blocks.insert(cell, block);
        }
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

/// Grows a random 4-connected blob of `n` cells starting at (0,0). Each new
/// cell usually copies the material of the cell it grew from, so materials
/// form veins.
fn random_blob(rng: &mut ChaCha8Rng, n: usize) -> Vec<(Cell, Material)> {
    let random_material = |rng: &mut ChaCha8Rng| ASTEROID_MATERIALS[rng.random_range(0..ASTEROID_MATERIALS.len())];
    let mut cells: Vec<(Cell, Material)> = vec![([0, 0], random_material(rng))];
    let mut taken: HashSet<Cell> = HashSet::from([[0, 0]]);
    let dirs = [[1, 0], [-1, 0], [0, 1], [0, -1]];
    let mut guard = 0;
    while cells.len() < n && guard < n * 100 {
        guard += 1;
        let (base, base_material) = cells[rng.random_range(0..cells.len())];
        let d = dirs[rng.random_range(0..4)];
        let cand = [base[0] + d[0], base[1] + d[1]];
        if taken.insert(cand) {
            let material =
                if rng.random_bool(consts::ASTEROID_VEIN_CHANCE) { base_material } else { random_material(rng) };
            cells.push((cand, material));
        }
    }
    cells
}

const ASTEROID_MATERIALS: [Material; 7] = [
    Material::Iron,
    Material::Copper,
    Material::Lead,
    Material::Plastic,
    Material::Tungsten,
    Material::Uranium,
    Material::Silicon,
];

/// Builds an unowned asteroid ship at `pos` with random blocks/mass/velocity/spin.
pub fn build_asteroid(rng: &mut ChaCha8Rng, id: ShipId, pos: [f32; 2]) -> Ship {
    let n = rng.random_range(consts::ASTEROID_BLOCKS_MIN..=consts::ASTEROID_BLOCKS_MAX);
    let cells = random_blob(rng, n);

    let mut ship = Ship::new(id, None);
    for (cell, material) in cells {
        let max_mass = consts::props(material).max_mass;
        let frac: f32 = rng.random_range(consts::ASTEROID_MASS_FRAC_MIN..=consts::ASTEROID_MASS_FRAC_MAX);
        ship.blocks.insert(cell, Block::new(material, max_mass * frac, 0.0));
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
    fn starter_reactor_is_insulated_and_feeds_both_circuits() {
        assert!(STARTER_LAYOUT.iter().all(|row| row.len() == 9));
        let mut world = World::new(1);
        let id = make_starter_ship(&mut world, "p".into());
        let blocks = &world.ships[&id].blocks;

        // The reactor only touches plastic and its two silicon outlets.
        let (&reactor, _) = blocks.iter().find(|(_, b)| b.material == Material::Uranium).expect("reactor");
        for d in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
            let m = blocks[&[reactor[0] + d[0], reactor[1] + d[1]]].material;
            assert!(matches!(m, Material::Plastic | Material::Silicon), "reactor touches {m:?}");
        }

        // Both outlets start open, otherwise the reactor would burst right away.
        let energy = |w: &World, cell: Cell| w.ships[&id].blocks[&cell].energy;
        let (spine, bus) = ([0, 5], [0, -2]);
        let (spine_before, bus_before) = (energy(&world, spine), energy(&world, bus));
        for _ in 0..500 {
            world.step();
        }
        assert!(energy(&world, spine) > spine_before, "spine should be fed by the reactor");
        assert!(energy(&world, bus) > bus_before, "engine bus should be fed by the reactor");
        assert_eq!(world.ships[&id].blocks.len(), 86, "nothing should burst while idling");
    }

    #[test]
    fn origin_chunk_always_has_a_sun() {
        for seed in [0u64, 1, 42, 999_999] {
            assert!(sun_in_chunk(seed, 0, 0).is_some());
        }
    }
}
