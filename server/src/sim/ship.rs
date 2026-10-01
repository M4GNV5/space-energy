//! Ship: rigid body made of 1x1 m blocks on an integer local grid.

use std::collections::HashMap;

use crate::protocol::{Cell, Material, ShipId};
use crate::sim::consts::{self, MIN_BLOCK_MASS};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub material: Material,
    pub mass: f32,
    pub energy: f32,
}

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: ShipId,
    /// `None` for asteroids.
    pub owner: Option<String>,
    /// World position of the centre of mass.
    pub pos: [f32; 2],
    pub rot: f32,
    pub vel: [f32; 2],
    pub omega: f32,
    /// Centre of mass in local grid coordinates.
    pub com: [f32; 2],
    pub inertia: f32,
    pub total_mass: f32,
    pub blocks: HashMap<Cell, Block>,
}

impl Ship {
    pub fn new(id: ShipId, owner: Option<String>) -> Self {
        Ship {
            id,
            owner,
            pos: [0.0, 0.0],
            rot: 0.0,
            vel: [0.0, 0.0],
            omega: 0.0,
            com: [0.0, 0.0],
            inertia: 1.0,
            total_mass: 0.0,
            blocks: HashMap::new(),
        }
    }

    /// Recomputes centre of mass and moment of inertia from block masses.
    /// Shifts `pos` so that world positions of existing blocks do not jump.
    pub fn recompute_com_inertia(&mut self) {
        let mut total_mass = 0.0f32;
        let mut sum = [0.0f32, 0.0f32];
        for (cell, b) in &self.blocks {
            total_mass += b.mass;
            sum[0] += b.mass * cell[0] as f32;
            sum[1] += b.mass * cell[1] as f32;
        }
        if total_mass <= 0.0 {
            self.total_mass = 0.0;
            self.inertia = 1.0;
            return;
        }
        let new_com = [sum[0] / total_mass, sum[1] / total_mass];
        let dcom = [new_com[0] - self.com[0], new_com[1] - self.com[1]];
        let (s, c) = self.rot.sin_cos();
        let rdx = c * dcom[0] - s * dcom[1];
        let rdy = s * dcom[0] + c * dcom[1];
        self.pos[0] += rdx;
        self.pos[1] += rdy;
        self.com = new_com;

        let mut inertia = 0.0f32;
        for (cell, b) in &self.blocks {
            let dx = cell[0] as f32 - new_com[0];
            let dy = cell[1] as f32 - new_com[1];
            inertia += b.mass * (dx * dx + dy * dy) + b.mass / 6.0;
        }
        self.total_mass = total_mass;
        self.inertia = inertia.max(1e-6);
    }

    /// Ship-local point -> world point.
    pub fn local_to_world(&self, p: [f32; 2]) -> [f32; 2] {
        let rel = [p[0] - self.com[0], p[1] - self.com[1]];
        let (s, c) = self.rot.sin_cos();
        [
            self.pos[0] + c * rel[0] - s * rel[1],
            self.pos[1] + s * rel[0] + c * rel[1],
        ]
    }

    /// World point -> ship-local point.
    pub fn world_to_local(&self, p: [f32; 2]) -> [f32; 2] {
        let rel = [p[0] - self.pos[0], p[1] - self.pos[1]];
        let (s, c) = self.rot.sin_cos();
        [
            c * rel[0] + s * rel[1] + self.com[0],
            -s * rel[0] + c * rel[1] + self.com[1],
        ]
    }

    /// Rotates a ship-local direction vector into world space (no translation).
    pub fn local_dir_to_world(&self, d: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.rot.sin_cos();
        [c * d[0] - s * d[1], s * d[0] + c * d[1]]
    }

    /// Rotates a world direction vector into ship-local space (no translation).
    pub fn world_dir_to_local(&self, d: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.rot.sin_cos();
        [c * d[0] + s * d[1], -s * d[0] + c * d[1]]
    }

    /// World-space velocity of the given ship-local point (rigid body motion).
    pub fn point_velocity(&self, local_p: [f32; 2]) -> [f32; 2] {
        let rel = [local_p[0] - self.com[0], local_p[1] - self.com[1]];
        let (s, c) = self.rot.sin_cos();
        let rx = c * rel[0] - s * rel[1];
        let ry = s * rel[0] + c * rel[1];
        [self.vel[0] - self.omega * ry, self.vel[1] + self.omega * rx]
    }

    /// Applies a linear impulse (N*s) at a world point: changes vel and omega.
    pub fn apply_impulse(&mut self, world_point: [f32; 2], impulse: [f32; 2]) {
        if self.total_mass <= 0.0 {
            return;
        }
        self.vel[0] += impulse[0] / self.total_mass;
        self.vel[1] += impulse[1] / self.total_mass;
        let r = [world_point[0] - self.pos[0], world_point[1] - self.pos[1]];
        let torque = r[0] * impulse[1] - r[1] * impulse[0];
        self.omega += torque / self.inertia;
    }

    /// Bounding circle radius (world units) around `pos` containing all blocks.
    pub fn bounding_radius(&self) -> f32 {
        self.blocks
            .keys()
            .map(|c| {
                let dx = c[0] as f32 - self.com[0];
                let dy = c[1] as f32 - self.com[1];
                (dx * dx + dy * dy).sqrt() + std::f32::consts::FRAC_1_SQRT_2
            })
            .fold(0.0f32, f32::max)
    }

    fn is_adjacent_to_ship(&self, cell: Cell) -> bool {
        let adj = [
            [cell[0] + 1, cell[1]],
            [cell[0] - 1, cell[1]],
            [cell[0], cell[1] + 1],
            [cell[0], cell[1] - 1],
        ];
        adj.iter().any(|c| self.blocks.contains_key(c))
    }

    /// Moves `kg` of mass from block `from` to block/empty cell `to`.
    pub fn apply_move_mass(&mut self, from: Cell, to: Cell, kg: f32) -> Result<(), String> {
        let from_block = *self.blocks.get(&from).ok_or("from block does not exist")?;
        let material = from_block.material;
        let max_mass = consts::props(material).max_mass;

        let to_exists = self.blocks.contains_key(&to);
        if let Some(tb) = self.blocks.get(&to) {
            if tb.material != material {
                return Err("target block has different material".into());
            }
        } else if !self.is_adjacent_to_ship(to) {
            return Err("target cell is not adjacent to the ship".into());
        }

        let to_current_mass = self.blocks.get(&to).map(|b| b.mass).unwrap_or(0.0);
        let room = (max_mass - to_current_mass).max(0.0);
        let kg_clamped = kg.max(0.0).min(from_block.mass).min(room);

        if !to_exists && kg_clamped < MIN_BLOCK_MASS {
            return Err("not enough mass to create a new block".into());
        }
        if kg_clamped <= 0.0 {
            return Ok(());
        }

        if let Some(fb) = self.blocks.get_mut(&from) {
            fb.mass -= kg_clamped;
        }
        if !to_exists {
            self.blocks.insert(to, Block { material, mass: 0.0, energy: 0.0 });
        }
        if let Some(tb) = self.blocks.get_mut(&to) {
            tb.mass += kg_clamped;
        }

        if let Some(fb) = self.blocks.get(&from) {
            if fb.mass <= 1e-6 {
                let leftover_energy = fb.energy;
                self.blocks.remove(&from);
                if let Some(tb) = self.blocks.get_mut(&to) {
                    tb.energy += leftover_energy;
                }
            }
        }

        self.recompute_com_inertia();
        Ok(())
    }

    /// Collects nearby mass packets into this ship's blocks.
    /// `block = None`: into all matching-material blocks, nearest first.
    /// `block = Some(cell)`: only into that cell (creating it if empty and adjacent).
    pub fn apply_collect(
        &mut self,
        packets: &mut Vec<crate::sim::world::Packet>,
        block: Option<Cell>,
        material: Option<Material>,
    ) -> Result<(), String> {
        match block {
            None => {
                for packet in packets.iter_mut() {
                    if packet.mass <= 0.0 {
                        continue;
                    }
                    let mut candidates: Vec<(f32, Cell)> = self
                        .blocks
                        .iter()
                        .filter(|(_, b)| b.material == packet.material)
                        .filter_map(|(&c, _)| {
                            let wp = self.local_to_world([c[0] as f32, c[1] as f32]);
                            let d = dist(wp, packet.pos);
                            (d <= consts::COLLECT_RANGE).then_some((d, c))
                        })
                        .collect();
                    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    for (_, c) in candidates {
                        if packet.mass <= 0.0 {
                            break;
                        }
                        let max_mass = consts::props(packet.material).max_mass;
                        let b = self.blocks.get_mut(&c).unwrap();
                        let room = (max_mass - b.mass).max(0.0);
                        let take = room.min(packet.mass);
                        b.mass += take;
                        packet.mass -= take;
                    }
                }
                packets.retain(|p| p.mass > 1e-6);
                self.recompute_com_inertia();
                Ok(())
            }
            Some(cell) => {
                let (target_material, creating) = match self.blocks.get(&cell) {
                    Some(b) => (b.material, false),
                    None => {
                        let m = material.ok_or("material required for an empty cell")?;
                        if !self.is_adjacent_to_ship(cell) {
                            return Err("target cell is not adjacent to the ship".into());
                        }
                        (m, true)
                    }
                };
                let world_p = self.local_to_world([cell[0] as f32, cell[1] as f32]);
                let max_mass = consts::props(target_material).max_mass;
                let room_total = if creating {
                    max_mass
                } else {
                    max_mass - self.blocks.get(&cell).unwrap().mass
                };
                let mut collected = 0.0f32;
                let mut dist_packets: Vec<(f32, usize)> = packets
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.material == target_material && p.mass > 0.0)
                    .filter_map(|(i, p)| {
                        let d = dist(world_p, p.pos);
                        (d <= consts::COLLECT_RANGE).then_some((d, i))
                    })
                    .collect();
                dist_packets.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                for (_, i) in dist_packets {
                    let room = (room_total - collected).max(0.0);
                    if room <= 0.0 {
                        break;
                    }
                    let take = room.min(packets[i].mass);
                    packets[i].mass -= take;
                    collected += take;
                }
                packets.retain(|p| p.mass > 1e-6);
                if creating {
                    if collected < MIN_BLOCK_MASS {
                        return Err("not enough mass collected to create a block".into());
                    }
                    self.blocks.insert(cell, Block { material: target_material, mass: collected, energy: 0.0 });
                } else {
                    self.blocks.get_mut(&cell).unwrap().mass += collected;
                }
                self.recompute_com_inertia();
                Ok(())
            }
        }
    }
}

pub fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::Packet;

    fn approx(a: f32, b: f32, eps: f32) {
        assert!((a - b).abs() < eps, "expected {b}, got {a}");
    }

    #[test]
    fn move_mass_creates_block_and_moves_energy_when_source_empties() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block { material: Material::Iron, mass: 100.0, energy: 50.0 });
        ship.recompute_com_inertia();

        ship.apply_move_mass([0, 0], [1, 0], 100.0).unwrap();

        assert!(!ship.blocks.contains_key(&[0, 0]), "source block should be removed once empty");
        let to = ship.blocks.get(&[1, 0]).expect("target block created");
        approx(to.mass, 100.0, 1e-4);
        approx(to.energy, 50.0, 1e-4);
    }

    #[test]
    fn move_mass_partial_preserves_total_mass_and_leaves_energy_behind() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block { material: Material::Copper, mass: 200.0, energy: 40.0 });
        ship.blocks.insert([1, 0], Block { material: Material::Copper, mass: 50.0, energy: 10.0 });
        ship.recompute_com_inertia();
        let total_before: f32 = ship.blocks.values().map(|b| b.mass).sum();

        ship.apply_move_mass([0, 0], [1, 0], 30.0).unwrap();

        let total_after: f32 = ship.blocks.values().map(|b| b.mass).sum();
        approx(total_after, total_before, 1e-3);
        approx(ship.blocks[&[0, 0]].mass, 170.0, 1e-4);
        approx(ship.blocks[&[1, 0]].mass, 80.0, 1e-4);
        // source didn't empty, so its energy stays put
        approx(ship.blocks[&[0, 0]].energy, 40.0, 1e-4);
        approx(ship.blocks[&[1, 0]].energy, 10.0, 1e-4);
    }

    #[test]
    fn move_mass_rejects_mismatched_material_and_non_adjacent_target() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block { material: Material::Iron, mass: 100.0, energy: 0.0 });
        ship.blocks.insert([5, 5], Block { material: Material::Copper, mass: 100.0, energy: 0.0 });
        ship.recompute_com_inertia();

        assert!(ship.apply_move_mass([0, 0], [5, 5], 10.0).is_err());
        assert!(ship.apply_move_mass([0, 0], [9, 9], 10.0).is_err());
    }

    #[test]
    fn collect_pulls_matching_packets_into_blocks() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block { material: Material::Iron, mass: 10.0, energy: 0.0 });
        ship.recompute_com_inertia();
        let mut packets = vec![Packet { pos: [0.0, 0.0], vel: [0.0, 0.0], material: Material::Iron, mass: 5.0, age: 0.0 }];

        ship.apply_collect(&mut packets, None, None).unwrap();

        approx(ship.blocks[&[0, 0]].mass, 15.0, 1e-4);
        assert!(packets.is_empty(), "packet should be fully consumed");
    }

    #[test]
    fn point_velocity_includes_rotation_component() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block { material: Material::Iron, mass: 100.0, energy: 0.0 });
        ship.recompute_com_inertia();
        ship.vel = [1.0, 0.0];
        ship.omega = 2.0;

        // Point at local (1,0), rot = 0 so world offset from com == (1,0).
        let v = ship.point_velocity([1.0, 0.0]);
        approx(v[0], 1.0, 1e-4); // vel.x - omega*ry, ry = 0
        approx(v[1], 2.0, 1e-4); // vel.y + omega*rx, rx = 1
    }

    #[test]
    fn local_world_dir_round_trip() {
        let mut ship = Ship::new(1, None);
        ship.rot = 0.7;
        let v = [0.3, -0.9];
        let round_tripped = ship.world_dir_to_local(ship.local_dir_to_world(v));
        approx(round_tripped[0], v[0], 1e-4);
        approx(round_tripped[1], v[1], 1e-4);
    }
}
