//! All balancing values. Units: m, kg, s, J. "per tick" = 1/TICK_RATE s.
//! Material max mass / energy capacity are mirrored in `client/src/protocol.ts`.

use crate::protocol::Material;

pub const TICK_RATE: u32 = 25;
pub const DT: f32 = 1.0 / TICK_RATE as f32;

pub struct MaterialProps {
    /// kg per block (a block is 1 m³, so this is roughly the density)
    pub max_mass: f32,
    /// J; exceeding it bursts the block
    pub energy_capacity: f32,
    /// Fraction of the fill-ratio difference equalised per tick with a neighbour
    /// (pair uses the min of both). Must stay <= 0.25 for stability.
    pub conductance: f32,
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
    }
}

const IRON: MaterialProps = MaterialProps {
    max_mass: 7900.0,
    energy_capacity: 2.0e6,
    conductance: 0.15,
    energy_emit_rate: 100_000.0,
    mass_emit_rate: 1.0,
};
const COPPER: MaterialProps = MaterialProps {
    max_mass: 8900.0,
    energy_capacity: 1.5e6,
    conductance: 0.25,
    energy_emit_rate: 5_000.0,
    mass_emit_rate: 1.0,
};
const LEAD: MaterialProps = MaterialProps {
    max_mass: 11300.0,
    energy_capacity: 1.0e6,
    conductance: 0.05,
    energy_emit_rate: 5_000.0,
    mass_emit_rate: 2.0,
};
const PLASTIC: MaterialProps = MaterialProps {
    max_mass: 1200.0,
    energy_capacity: 0.5e6,
    conductance: 0.002,
    energy_emit_rate: 1_000.0,
    mass_emit_rate: 1.0,
};
const TUNGSTEN: MaterialProps = MaterialProps {
    max_mass: 19300.0,
    energy_capacity: 2.0e7,
    conductance: 0.1,
    energy_emit_rate: 10_000.0,
    mass_emit_rate: 1.0,
};
const URANIUM: MaterialProps = MaterialProps {
    max_mass: 19000.0,
    energy_capacity: 4.0e6,
    conductance: 0.08,
    energy_emit_rate: 5_000.0,
    mass_emit_rate: 1.0,
};

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
/// Exhaust speed of directed mass emission relative to the emitting block (m/s).
pub const EXHAUST_SPEED: f32 = 1000.0;
/// Energy drawn from the emitting block per kg of emitted mass (J/kg).
pub const EMIT_ENERGY_PER_KG: f32 = 2000.0;
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
/// Fraction of linear velocity lost per tick.
pub const LINEAR_DAMPING: f32 = 0.005;
/// Fraction of angular velocity lost per tick.
pub const ANGULAR_DAMPING: f32 = 0.02;

// --- World ---
pub const CHUNK_SIZE: f32 = 1000.0;
/// Probability that a chunk contains a sun (chunk 0,0 always has one).
pub const SUN_CHANCE: f64 = 0.3;
pub const SUN_RADIUS_MIN: f32 = 10.0;
pub const SUN_RADIUS_MAX: f32 = 30.0;
/// Energy emitted by a sun per tick (J), split across SUN_RAYS rays.
pub const SUN_POWER: f32 = 2.0e6;
pub const SUN_RAYS: usize = 64;
/// Starting ships spawn this far from the nearest sun's surface (m).
pub const SPAWN_SUN_DISTANCE: f32 = 200.0;

pub const ASTEROIDS_PER_PLAYER: usize = 12;
pub const ASTEROID_SPAWN_MIN: f32 = 150.0;
pub const ASTEROID_SPAWN_MAX: f32 = 600.0;
pub const ASTEROID_DESPAWN: f32 = 1500.0;
pub const ASTEROID_BLOCKS_MIN: usize = 3;
pub const ASTEROID_BLOCKS_MAX: usize = 15;
pub const ASTEROID_SPEED_MAX: f32 = 2.0;
/// Max angular speed of a freshly spawned asteroid (rad/s). Added for M3.
pub const ASTEROID_SPIN_MAX: f32 = 0.3;
/// Energy every asteroid block radiates away per tick (J), so sunlight alone
/// does not burst asteroids. A flat rate: it cancels weak sunlight but an
/// N-block asteroid only shrugs off N times this much laser power.
pub const ASTEROID_COOLING: f32 = 200.0;
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
pub const SPAWN_JITTER: f32 = 30.0;
