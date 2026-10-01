//! Turns the full view of a connection into `state` deltas: only what differs
//! from what that connection was sent before.

use std::collections::{HashMap, HashSet};

use crate::protocol::{BlockDelta, BlockView, Cell, PacketView, RayView, ShipDelta, ShipId, ShipView, StateDelta, StateMsg, SunView};
use crate::sim::consts;

/// A block's energy is resent once it is off by this fraction of its capacity.
const ENERGY_EPS: f32 = 0.01;
/// A block's mass is resent once it is off by this fraction of itself.
const MASS_EPS: f32 = 1.0e-5;
/// The centre of mass is resent once it is off by this much (m).
const COM_EPS: f32 = 0.001;
/// Of the rays that are not player beams, one in this many is sent.
const RAY_SAMPLE: u64 = 4;
/// A pose is resent once the client's dead reckoning is off by this much (m, m/s).
const POS_EPS: f32 = 0.02;
/// Same for the rotation (rad, rad/s).
const ROT_EPS: f32 = 0.002;

struct SentShip {
    /// As sent, i.e. rounded.
    pose: [f32; 6],
    pose_tick: u64,
    com: [f32; 2],
    blocks: HashMap<Cell, BlockView>,
}

/// Remembers what one connection knows.
#[derive(Default)]
pub struct DeltaEncoder {
    started: bool,
    ships: HashMap<ShipId, SentShip>,
    /// Packet id -> mass, as sent.
    packets: HashMap<u64, f32>,
    suns: Vec<SunView>,
}

fn round_to(x: f32, decimals: i32) -> f32 {
    let f = 10f32.powi(decimals);
    let r = (x * f).round() / f;
    // Far from the origin an f32 has no decimals left to round.
    if r.is_finite() { r } else { x }
}

/// Rounds to `digits` significant digits, to keep the JSON short.
fn round_sig(x: f32, digits: i32) -> f32 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    round_to(x, digits - 1 - x.abs().log10().floor() as i32)
}

fn pose_of(s: &ShipView) -> [f32; 6] {
    [round_to(s.x, 3), round_to(s.y, 3), round_to(s.rot, 4), round_to(s.vx, 3), round_to(s.vy, 3), round_to(s.omega, 4)]
}

/// Where the client believes the ship is at `tick`. Mirrors `GameState.apply` of the client.
fn predicted(sent: &SentShip, tick: u64) -> [f32; 3] {
    let dt = tick.saturating_sub(sent.pose_tick) as f32 * consts::DT;
    let p = &sent.pose;
    [p[0] + p[3] * dt, p[1] + p[4] * dt, p[2] + p[5] * dt]
}

fn pose_is_off(sent: &SentShip, s: &ShipView, tick: u64) -> bool {
    let [x, y, rot] = predicted(sent, tick);
    (x - s.x).abs() > POS_EPS
        || (y - s.y).abs() > POS_EPS
        || (rot - s.rot).abs() > ROT_EPS
        || (sent.pose[3] - s.vx).abs() > POS_EPS
        || (sent.pose[4] - s.vy).abs() > POS_EPS
        || (sent.pose[5] - s.omega).abs() > ROT_EPS
}

/// Material and direction are always exact, mass and energy are allowed to lag a little.
fn block_changed(sent: &BlockView, b: &BlockView) -> bool {
    if sent.m != b.m || sent.dir != b.dir || (sent.mass - b.mass).abs() > MASS_EPS * b.mass {
        return true;
    }
    let capacity = consts::props(b.m).energy_per_kg * b.mass;
    (sent.energy - b.energy).abs() > ENERGY_EPS * capacity || (sent.energy == 0.0) != (b.energy == 0.0)
}

fn block_delta(b: &BlockView) -> BlockDelta {
    BlockDelta(BlockView { energy: round_sig(b.energy, 5), ..b.clone() })
}

impl DeltaEncoder {
    pub fn encode(&mut self, snap: &StateMsg) -> StateDelta {
        let mut out = StateDelta { tick: snap.tick, reset: !self.started, ..Default::default() };
        self.started = true;

        for s in &snap.ships {
            let Some(sent) = self.ships.get_mut(&s.id) else {
                let pose = pose_of(s);
                out.ships.push(ShipDelta {
                    id: s.id,
                    new: true,
                    owner: s.owner.clone(),
                    pose: Some(pose),
                    com: Some(s.com),
                    blocks: s.blocks.iter().map(block_delta).collect(),
                    del: Vec::new(),
                });
                let blocks = s.blocks.iter().map(|b| (b.p, b.clone())).collect();
                self.ships.insert(s.id, SentShip { pose, pose_tick: snap.tick, com: s.com, blocks });
                continue;
            };

            let mut d = ShipDelta { id: s.id, ..Default::default() };
            if pose_is_off(sent, s, snap.tick) {
                sent.pose = pose_of(s);
                sent.pose_tick = snap.tick;
                d.pose = Some(sent.pose);
            }
            if (sent.com[0] - s.com[0]).abs() > COM_EPS || (sent.com[1] - s.com[1]).abs() > COM_EPS {
                sent.com = s.com;
                d.com = Some(s.com);
            }
            let mut known = 0;
            for b in &s.blocks {
                match sent.blocks.get_mut(&b.p) {
                    Some(old) => {
                        known += 1;
                        if block_changed(old, b) {
                            *old = b.clone();
                            d.blocks.push(block_delta(b));
                        }
                    }
                    None => {
                        sent.blocks.insert(b.p, b.clone());
                        known += 1;
                        d.blocks.push(block_delta(b));
                    }
                }
            }
            if known < sent.blocks.len() {
                let alive: HashSet<Cell> = s.blocks.iter().map(|b| b.p).collect();
                sent.blocks.retain(|p, _| alive.contains(p) || {
                    d.del.push(*p);
                    false
                });
            }
            if d.pose.is_some() || d.com.is_some() || !d.blocks.is_empty() || !d.del.is_empty() {
                out.ships.push(d);
            }
        }
        if snap.ships.len() < self.ships.len() {
            let alive: HashSet<ShipId> = snap.ships.iter().map(|s| s.id).collect();
            self.ships.retain(|id, _| alive.contains(id) || {
                out.gone.push(*id);
                false
            });
        }

        if !out.reset && self.suns == snap.suns {
            out.suns = None;
        } else {
            self.suns = snap.suns.clone();
            out.suns = Some(snap.suns.clone());
        }

        for p in &snap.packets {
            if self.packets.insert(p.id, p.mass) != Some(p.mass) {
                out.packets.push(PacketView {
                    x: round_to(p.x, 2),
                    y: round_to(p.y, 2),
                    vx: round_to(p.vx, 3),
                    vy: round_to(p.vy, 3),
                    ..p.clone()
                });
            }
        }
        if snap.packets.len() < self.packets.len() {
            let alive: HashSet<u64> = snap.packets.iter().map(|p| p.id).collect();
            self.packets.retain(|id, _| alive.contains(id) || {
                out.pgone.push(*id);
                false
            });
        }

        out.rays = snap
            .rays
            .iter()
            .enumerate()
            .filter(|(i, ray)| ray.beam || (*i as u64 + snap.tick).is_multiple_of(RAY_SAMPLE))
            .map(|(_, ray)| RayView {
                x1: round_to(ray.x1, 2),
                y1: round_to(ray.y1, 2),
                x2: round_to(ray.x2, 2),
                y2: round_to(ray.y2, 2),
                energy: round_sig(ray.energy, 3),
                emitted: round_sig(ray.emitted, 3),
                beam: ray.beam,
            })
            .collect();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Material;

    fn block(p: Cell, energy: f32) -> BlockView {
        BlockView { p, m: Material::Iron, mass: 1000.0, energy, dir: None }
    }

    fn ship(id: ShipId, x: f32, vx: f32, blocks: Vec<BlockView>) -> ShipView {
        ShipView { id, owner: Some("p".into()), x, y: 0.0, rot: 0.0, vx, vy: 0.0, omega: 0.0, com: [0.0, 0.0], blocks }
    }

    fn snap(tick: u64, ships: Vec<ShipView>) -> StateMsg {
        StateMsg { tick, ships, suns: vec![SunView { x: 0.0, y: 0.0, radius: 5.0 }], packets: Vec::new(), rays: Vec::new() }
    }

    #[test]
    fn first_state_is_complete_and_an_unchanged_one_is_empty() {
        let mut enc = DeltaEncoder::default();
        let s = snap(1, vec![ship(7, 0.0, 0.0, vec![block([0, 0], 5.0), block([1, 0], 0.0)])]);

        let d = enc.encode(&s);
        assert!(d.reset && d.suns.is_some());
        assert!(d.ships[0].new && d.ships[0].pose.is_some() && d.ships[0].blocks.len() == 2);

        let d = enc.encode(&StateMsg { tick: 2, ..s });
        assert_eq!(serde_json::to_string(&d).unwrap(), r#"{"tick":2}"#);
    }

    #[test]
    fn only_changed_blocks_are_sent() {
        let mut enc = DeltaEncoder::default();
        enc.encode(&snap(1, vec![ship(7, 0.0, 0.0, vec![block([0, 0], 5.0), block([1, 0], 0.0), block([2, 0], 0.0)])]));

        // [0,0] heats up a lot, [1,0] by less than the tolerance, [2,0] is gone.
        let capacity = 6500.0 * 1000.0;
        let d = enc.encode(&snap(2, vec![ship(7, 0.0, 0.0, vec![block([0, 0], 0.5 * capacity), block([1, 0], 0.0)])]));
        assert_eq!(serde_json::to_string(&d.ships).unwrap(), r#"[{"id":7,"blocks":[[0,0,"iron",1000.0,3250000.0]],"del":[[2,0]]}]"#);

        // The small errors add up until they are sent.
        let mut resent = false;
        for i in 1..=10 {
            let energy = i as f32 * 0.003 * capacity;
            let d = enc.encode(&snap(2 + i, vec![ship(7, 0.0, 0.0, vec![block([0, 0], 0.5 * capacity), block([1, 0], energy)])]));
            resent |= !d.ships.is_empty();
        }
        assert!(resent);
    }

    #[test]
    fn a_coasting_ship_needs_no_pose_updates() {
        let mut enc = DeltaEncoder::default();
        enc.encode(&snap(1, vec![ship(7, 0.0, 10.0, vec![block([0, 0], 0.0)])]));
        for i in 1..50u64 {
            let d = enc.encode(&snap(1 + i, vec![ship(7, i as f32 * 10.0 * consts::DT, 10.0, vec![block([0, 0], 0.0)])]));
            assert!(d.ships.is_empty(), "tick {i}");
        }
        // It stops dead: the prediction no longer holds.
        let d = enc.encode(&snap(51, vec![ship(7, 20.0, 0.0, vec![block([0, 0], 0.0)])]));
        assert_eq!(d.ships[0].pose, Some([20.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
    }

    #[test]
    fn ships_and_packets_that_leave_are_reported_once() {
        let mut enc = DeltaEncoder::default();
        let packet = PacketView { id: 3, x: 0.0, y: 0.0, vx: 1.0, vy: 0.0, m: Material::Lead, mass: 2.0 };
        let mut s = snap(1, vec![ship(7, 0.0, 0.0, vec![block([0, 0], 0.0)])]);
        s.packets.push(packet.clone());
        assert_eq!(enc.encode(&s).packets.len(), 1);

        // A packet that only moved is not resent.
        s.packets[0].x = 0.04;
        assert!(enc.encode(&s).packets.is_empty());

        let d = enc.encode(&snap(3, Vec::new()));
        assert_eq!((d.gone, d.pgone), (vec![7], vec![3]));
        let d = enc.encode(&snap(4, Vec::new()));
        assert!(d.gone.is_empty() && d.pgone.is_empty());
    }
}
