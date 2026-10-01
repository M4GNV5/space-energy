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
//!
//! `state` messages are deltas against what this connection was sent before,
//! see `StateDelta`. The client acknowledges each one with `ack`; the server
//! skips ticks rather than queueing them for a connection that falls behind.

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
    /// Asteroid filler. Leaves no mass packets when it bursts.
    Rock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
    /// Emission from one block in one direction, repeated every tick until a
    /// new `emit` for the same block and `dir` replaces it or `EMIT_HOLD_TICKS`
    /// pass, so resend it while it should last. `energy: 0, mass: 0` stops it.
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
    /// The `state` of `tick` has arrived.
    Ack { tick: u64 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome { player: String, ships: Vec<ShipId> },
    State(StateDelta),
    Error { msg: String },
}

/// What changed in the client's view since the last `state` it was sent.
/// Empty lists are left out.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StateDelta {
    pub tick: u64,
    /// First message of a connection: forget everything known so far.
    #[serde(skip_serializing_if = "is_false")]
    pub reset: bool,
    /// New and changed ships.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ships: Vec<ShipDelta>,
    /// Ships that left the view or no longer exist.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gone: Vec<ShipId>,
    /// All suns in view, only when that set changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suns: Option<Vec<SunView>>,
    /// New packets and packets whose mass changed. Packets fly straight, so
    /// the client moves them by `vx, vy` each tick.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub packets: Vec<PacketView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pgone: Vec<u64>,
    /// Player beams of this tick, and a sample of the other rays that delivered
    /// energy (sunlight, bursts). For drawing only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rays: Vec<RayView>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ShipDelta {
    pub id: ShipId,
    /// A ship the client does not know yet (or must rebuild): `owner`, `pose`,
    /// `com` and all blocks follow.
    #[serde(skip_serializing_if = "is_false")]
    pub new: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// `[x, y, rot, vx, vy, omega]`. Left out while the ship is where the last
    /// pose plus `vx, vy, omega` per tick puts it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pose: Option<[f32; 6]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub com: Option<[f32; 2]>,
    /// New and changed blocks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockDelta>,
    /// Cells whose block is gone.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub del: Vec<Cell>,
}

/// On the wire: `[i, j, material, mass, energy]`, plus `dir` if set.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDelta(pub BlockView);

impl Serialize for BlockDelta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let b = &self.0;
        let mut seq = serializer.serialize_seq(Some(5 + b.dir.is_some() as usize))?;
        seq.serialize_element(&b.p[0])?;
        seq.serialize_element(&b.p[1])?;
        seq.serialize_element(&b.m)?;
        seq.serialize_element(&b.mass)?;
        seq.serialize_element(&b.energy)?;
        if let Some(dir) = b.dir {
            seq.serialize_element(&dir)?;
        }
        seq.end()
    }
}

/// Everything in one connection's view at one tick. Not sent as such: the
/// connection's `DeltaEncoder` turns it into a `StateDelta`.
#[derive(Debug, Clone)]
pub struct StateMsg {
    pub tick: u64,
    pub ships: Vec<ShipView>,
    pub suns: Vec<SunView>,
    pub packets: Vec<PacketView>,
    /// Energy rays emitted this tick, for drawing only.
    pub rays: Vec<RayView>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct BlockView {
    pub p: Cell,
    pub m: Material,
    /// kg
    pub mass: f32,
    /// J
    pub energy: f32,
    /// Silicon only: the face energy flows out of. `None` = not conducting.
    pub dir: Option<Dir>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SunView {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

/// On the wire: `[id, x, y, vx, vy, material, mass]`.
#[derive(Debug, Clone)]
pub struct PacketView {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    /// Velocity (m/s), so the client can draw fast packets as streaks.
    pub vx: f32,
    pub vy: f32,
    pub m: Material,
    pub mass: f32,
}

impl Serialize for PacketView {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.id, self.x, self.y, self.vx, self.vy, self.m, self.mass).serialize(serializer)
    }
}

/// On the wire: `[x1, y1, x2, y2, energy, emitted, beam]` with `beam` as 0 or 1.
#[derive(Debug, Clone)]
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

impl Serialize for RayView {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.x1, self.y1, self.x2, self.y2, self.energy, self.emitted, self.beam as u8).serialize(serializer)
    }
}
