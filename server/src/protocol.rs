//! Wire protocol: JSON over a single websocket at `/ws`.
//! Mirrored by `client/src/protocol.ts` — keep both in sync.
//!
//! Conventions:
//! - World space is metric (m), y points up, angles in radians counter-clockwise.
//! - A ship's blocks live on a ship-local integer grid. Block `p = [i, j]` is a
//!   1 m × 1 m square centred at local point (i, j).
//! - `x, y` of a ship is the world position of its centre of mass, `com` is the
//!   centre of mass in local grid coordinates. World position of a block:
//!   `[x, y] + rotate(rot) * (p - com)`.
//! - Face directions are ship-local: n = +y, e = +x, s = -y, w = -x.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Material {
    Iron,
    Copper,
    Lead,
    Plastic,
    Tungsten,
    Uranium,
    Silicon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    N,
    E,
    S,
    W,
    All,
}

impl Dir {
    pub fn opposite(self) -> Dir {
        match self {
            Dir::N => Dir::S,
            Dir::E => Dir::W,
            Dir::S => Dir::N,
            Dir::W => Dir::E,
            Dir::All => Dir::All,
        }
    }
}

pub type ShipId = u64;
pub type Cell = [i32; 2];

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Create or resume a player. Must be the first message.
    Login { name: String },
    /// Area of interest for `state` snapshots (world centre + radius in m).
    View { x: f32, y: f32, r: f32 },
    /// Move `kg` of mass from block `from` to block/empty cell `to` of the same ship.
    MoveMass { ship: ShipId, from: Cell, to: Cell, kg: f32 },
    /// One-shot emission for the next tick. Client resends while a key is held.
    /// `energy` in J, `mass` in kg; both clamped by the server.
    /// Requesting >= 1 J also acts as a control signal: it points a silicon
    /// block at `dir` (`all` = off).
    Emit { ship: ShipId, block: Cell, dir: Dir, energy: f32, mass: f32 },
    /// Collect nearby mass packets.
    /// Without `block`: into all matching-material blocks of the ship.
    /// With `block` (+ `material` if it is an empty adjacent cell): into that cell only.
    Collect {
        ship: ShipId,
        #[serde(default)]
        block: Option<Cell>,
        #[serde(default)]
        material: Option<Material>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome { player: String, ships: Vec<ShipId> },
    State(StateMsg),
    Error { msg: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct StateMsg {
    pub tick: u64,
    pub ships: Vec<ShipView>,
    pub suns: Vec<SunView>,
    pub packets: Vec<PacketView>,
    /// Energy rays emitted this tick, for drawing only.
    pub rays: Vec<RayView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShipView {
    pub id: ShipId,
    /// `None` for asteroids.
    pub owner: Option<String>,
    pub x: f32,
    pub y: f32,
    pub rot: f32,
    pub vx: f32,
    pub vy: f32,
    pub omega: f32,
    pub com: [f32; 2],
    pub blocks: Vec<BlockView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockView {
    pub p: Cell,
    pub m: Material,
    /// kg
    pub mass: f32,
    /// J
    pub energy: f32,
    /// Silicon only: the face energy flows out of. Absent = not conducting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<Dir>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SunView {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PacketView {
    pub x: f32,
    pub y: f32,
    /// Velocity (m/s), so the client can draw fast packets as streaks.
    pub vx: f32,
    pub vy: f32,
    pub m: Material,
    pub mass: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RayView {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    /// Energy delivered at the end point (J), for line intensity.
    pub energy: f32,
    /// Energy at the origin (J), before falloff.
    pub emitted: f32,
    /// True for rays from a player's emit command; drawn even when they hit nothing.
    pub beam: bool,
}
