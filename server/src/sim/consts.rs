//! All balancing values. Units: m, kg, s, J. "per tick" = 1/TICK_RATE s.
//! Material max mass / energy per kg are mirrored in `client/src/protocol.ts`.

use crate::protocol::Material;

pub const TICK_RATE: u32 = 25;
pub const DT: f32 = 1.0 / TICK_RATE as f32;

pub struct MaterialProps {
    /// kg per block (a block is 1 m³, so this is roughly the density)
    pub max_mass: f32,
    /// J per kg of block mass. A block's capacity is this times its mass;
    /// exceeding it bursts the block.
    pub energy_per_kg: f32,
    /// Fraction of its energy the fuller block of a pair conducts per tick at a
    /// fill-ratio difference of 1; scales linearly with the difference. A pair
    /// uses the max of both (a good conductor pulls energy through a poor one),
    /// unless one of them is an isolator.
    /// Also the block's own cap: per tick it gives away at most this fraction of
    /// its energy (all faces together) and takes in at most this fraction of its
    /// free capacity, whatever its neighbours are.
    pub conductance: f32,
    /// A pair with an isolator uses the min of both conductances instead.
    pub isolator: bool,
    /// Max energy emitted per tick (J)
    pub energy_emit_rate: f32,
    /// Max mass emitted per tick (kg)
    pub mass_emit_rate: f32,
}

pub fn props(m: Material) -> &'static MaterialProps {
    match m {
        Material::Iron => &IRON,
        Material::Copper => &COPPER,
        Material::Lead => &LEAD,
        Material::Plastic => &PLASTIC,
        Material::Tungsten => &TUNGSTEN,
        Material::Uranium => &URANIUM,
        Material::Silicon => &SILICON,
        Material::Rock => &ROCK,
    }
}

const IRON: MaterialProps = MaterialProps {
    max_mass: 7900.0,
    energy_per_kg: 6500.0,
    conductance: 0.15,
    isolator: false,
    energy_emit_rate: 100_000.0,
    mass_emit_rate: 1.0,
};
const COPPER: MaterialProps = MaterialProps {
    max_mass: 8900.0,
    energy_per_kg: 5000.0,
    conductance: 0.5,
    isolator: false,
    energy_emit_rate: 5_000.0,
    mass_emit_rate: 1.0,
};
const LEAD: MaterialProps = MaterialProps {
    max_mass: 11300.0,
    energy_per_kg: 250.0,
    conductance: 0.05,
    isolator: false,
    energy_emit_rate: 5_000.0,
    mass_emit_rate: 2.0,
};
const PLASTIC: MaterialProps = MaterialProps {
    max_mass: 1200.0,
    energy_per_kg: 5000.0,
    conductance: 0.002,
    isolator: true,
    energy_emit_rate: 1_000.0,
    mass_emit_rate: 1.0,
};
const TUNGSTEN: MaterialProps = MaterialProps {
    max_mass: 19300.0,
    energy_per_kg: 13000.0,
    conductance: 0.1,
    isolator: false,
    energy_emit_rate: 10_000.0,
    mass_emit_rate: 1.0,
};
const URANIUM: MaterialProps = MaterialProps {
    max_mass: 19000.0,
    energy_per_kg: 4000.0,
    conductance: 0.08,
    isolator: false,
    energy_emit_rate: 5_000.0,
    mass_emit_rate: 1.0,
};
/// One-way conductor: `conductance` applies only along its direction, otherwise 0.
const SILICON: MaterialProps = MaterialProps {
    max_mass: 2330.0,
    energy_per_kg: 5000.0,
    conductance: 0.2,
    isolator: false,
    energy_emit_rate: 1_000.0,
    mass_emit_rate: 1.0,
};
/// Asteroid filler: an isolator that bursts easily and leaves nothing behind,
/// so the minerals inside an asteroid can be heated and reached one by one.
const ROCK: MaterialProps = MaterialProps {
    max_mass: 2700.0,
    energy_per_kg: 300.0,
    conductance: 0.002,
    isolator: true,
    energy_emit_rate: 1_000.0,
    mass_emit_rate: 1.0,
};

// --- Conduction ---
/// Every face without a neighbour conducts into the void (fill ratio 0) at the
/// block's own conductance scaled by this. The energy is gone.
pub const VOID_CONDUCTANCE: f32 = 0.001;

// --- Control signals (silicon) ---
/// An emit command requesting at least this much energy (J) counts as a control
/// signal for the block, even if the block has no energy to emit.
pub const SIGNAL_MIN_ENERGY: f32 = 1.0;

/// Minimum mass of an existing block (kg). Below this it is removed.
pub const MIN_BLOCK_MASS: f32 = 1.0;

// --- Energy emission ---
/// delivered = energy / (1 + (d / FALLOFF_DISTANCE)^2)
pub const FALLOFF_DISTANCE: f32 = 50.0;
/// Rays whose falloff factor drops below this deliver nothing (defines max range).
pub const MIN_FALLOFF: f32 = 0.01;
/// Number of rays for an omni-directional emission (random angular offset per tick).
pub const OMNI_RAYS: usize = 16;

// --- Mass emission ---
/// Exhaust speed (m/s) per J of energy emitted together with the mass, relative
/// to the emitting block. Thrust impulse = mass × energy × this.
pub const EXHAUST_SPEED_PER_J: f32 = 0.5;
/// Mass packets disappear after this many seconds.
pub const PACKET_TTL: f32 = 60.0;
/// Packets within this distance of a block can be collected (m).
pub const COLLECT_RANGE: f32 = 10.0;

// --- Burst ---
/// Number of packets a bursting block splits its mass into.
pub const BURST_PACKETS: usize = 8;
/// Speed of burst packets relative to the block (m/s).
pub const BURST_SPEED: f32 = 5.0;

// --- Uranium ---
pub const URANIUM_KG_PER_TICK: f32 = 0.0016;
pub const URANIUM_J_PER_KG: f32 = 5.0e6;

// --- Movement helper (temporary until player scripting) ---
/// Deceleration (m/s²) of every ship that is not speeding up under its own
/// thrust this tick. The same at any speed, down to a full stop.
pub const LINEAR_BRAKE: f32 = 2.0;
/// Fraction of angular velocity lost per tick.
pub const ANGULAR_DAMPING: f32 = 0.02;

// --- World ---
pub const CHUNK_SIZE: f32 = 1000.0;
/// Probability that a chunk contains a sun (chunk 0,0 always has one).
pub const SUN_CHANCE: f64 = 0.3;
pub const SUN_RADIUS_MIN: f32 = 100.0;
pub const SUN_RADIUS_MAX: f32 = 300.0;
/// Energy emitted by a sun per tick, per metre of its radius (J/m), split across
/// SUN_RAYS rays. Scaling with the radius keeps the sunlight a ship receives at a
/// given distance from the surface roughly independent of the sun's size.
pub const SUN_POWER_PER_M: f32 = 2.0e4;
pub const SUN_RAYS: usize = 128;
/// Starting ships spawn this far from the nearest sun's surface (m).
pub const SPAWN_SUN_DISTANCE: f32 = 200.0;

pub const ASTEROIDS_PER_PLAYER: usize = 12;
pub const ASTEROID_SPAWN_MIN: f32 = 200.0;
pub const ASTEROID_SPAWN_MAX: f32 = 900.0;
pub const ASTEROID_DESPAWN: f32 = 1800.0;
pub const ASTEROID_BLOCKS_MIN: usize = 120;
pub const ASTEROID_BLOCKS_MAX: usize = 900;
/// Asteroid blocks spawn with a random fraction of their material's max mass
/// in this range. Kept low so their energy capacity stays laser-sized.
pub const ASTEROID_MASS_FRAC_MIN: f32 = 0.02;
pub const ASTEROID_MASS_FRAC_MAX: f32 = 0.15;
/// Fraction of an asteroid's blocks that are minerals; the rest is rock.
pub const ASTEROID_MINERAL_FRACTION: f32 = 0.15;
/// Minerals sit in deposits (a clump or a straight line) of this many blocks
/// of one material.
pub const ASTEROID_DEPOSIT_MIN: usize = 2;
pub const ASTEROID_DEPOSIT_MAX: usize = 10;
/// Asteroids do not spawn closer than this to a sun's surface (m).
pub const ASTEROID_SUN_MARGIN: f32 = 50.0;
pub const ASTEROID_SPEED_MAX: f32 = 2.0;
/// Max angular speed of a freshly spawned asteroid (rad/s). Added for M3.
pub const ASTEROID_SPIN_MAX: f32 = 0.3;
/// Fraction of sunlight an asteroid block absorbs (rock reflects the rest), so
/// asteroids near a sun do not burst. Lasers and bursts are not reduced.
pub const ASTEROID_SUN_ABSORPTION: f32 = 0.01;
/// Conduction inside asteroids is scaled by this (loose rock conducts badly),
/// so laser heat stays near the block that was hit instead of spreading out.
pub const ASTEROID_CONDUCTION: f32 = 0.1;

/// Max radius a client may request in `view` (m).
pub const MAX_VIEW_RADIUS: f32 = 3000.0;

// --- Spawning (added for M3) ---
/// Starter ships spawn near a random point within this distance of world origin.
pub const SPAWN_NEAR_ORIGIN_RADIUS: f32 = 50.0;
/// Extra random jitter added to a starter ship's distance from its sun, so
/// players spawning at the same time don't overlap exactly.
pub const SPAWN_JITTER: f32 = 80.0;
