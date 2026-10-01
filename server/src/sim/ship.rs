//! Ship: rigid body made of 1x1 m blocks on an integer local grid.

use std::collections::HashMap;

use crate::protocol::{Cell, Dir, Material, ShipId};
use crate::sim::consts::{self, MIN_BLOCK_MASS};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub material: Material,
    pub mass: f32,
    pub energy: f32,
    /// Silicon: the face energy flows out of. `None` = not conducting.
    pub dir: Option<Dir>,
}

impl Block {
    pub fn new(material: Material, mass: f32, energy: f32) -> Self {
        Block { material, mass, energy, dir: None }
    }

    /// Energy (J) this block can hold before it bursts. Scales with its mass.
    pub fn energy_capacity(&self) -> f32 {
        consts::props(self.material).energy_per_kg * self.mass
    }

    /// Which way energy may flow between a pair of adjacent blocks a -> b,
    /// where b lies at `pair_dir` from a: (a -> b allowed, b -> a allowed).
    /// Called on both blocks of the pair with the same `pair_dir`.
    pub fn conduction_gate(&self, pair_dir: Dir) -> (bool, bool) {
        match self.material {
            Material::Silicon => match self.dir {
                Some(d) if d == pair_dir => (true, false),
                Some(d) if d == pair_dir.opposite() => (false, true),
                _ => (false, false),
            },
            _ => (true, true),
        }
    }
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
    /// Bounding circle radius around `pos`, updated by `recompute_com_inertia`.
    radius: f32,
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
            radius: 0.0,
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
            self.radius = 0.0;
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
        let mut max_d2 = 0.0f32;
        for (cell, b) in &self.blocks {
            let dx = cell[0] as f32 - new_com[0];
            let dy = cell[1] as f32 - new_com[1];
            inertia += b.mass * (dx * dx + dy * dy) + b.mass / 6.0;
            max_d2 = max_d2.max(dx * dx + dy * dy);
        }
        self.radius = max_d2.sqrt() + std::f32::consts::FRAC_1_SQRT_2;
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
    /// Cached: only valid after `recompute_com_inertia`.
    pub fn bounding_radius(&self) -> f32 {
        self.radius
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

        // The moved mass takes its share of the energy along, so the source keeps its fill ratio.
        let moved_energy = from_block.energy * kg_clamped / from_block.mass;
        if let Some(fb) = self.blocks.get_mut(&from) {
            fb.mass -= kg_clamped;
            fb.energy -= moved_energy;
        }
        if !to_exists {
            self.blocks.insert(to, Block::new(material, 0.0, 0.0));
        }
        if let Some(tb) = self.blocks.get_mut(&to) {
            tb.mass += kg_clamped;
            tb.energy += moved_energy;
        }
        if self.blocks.get(&from).is_some_and(|fb| fb.mass <= 1e-6) {
            self.blocks.remove(&from);
        }

        self.recompute_com_inertia();
        Ok(())
    }

    /// Collects nearby mass packets into this ship's blocks.
    /// `block = None`: into all matching-material blocks, evening out their
    /// masses (lightest first). What does not fit becomes a new block in the free adjacent cell nearest to the
    /// centre of mass.
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
                    // In range of any block = in range of the ship: the mass may then go
                    // into every matching block, also ones further away.
                    let in_range = self
                        .blocks
                        .keys()
                        .any(|c| dist(self.local_to_world([c[0] as f32, c[1] as f32]), packet.pos) <= consts::COLLECT_RANGE);
                    if !in_range {
                        continue;
                    }
                    // Fill the lightest matching blocks first, raising them to a common
                    // level, so the mass evens out over all blocks of that material.
                    let max_mass = consts::props(packet.material).max_mass;
                    let mut masses: Vec<f32> =
                        self.blocks.values().filter(|b| b.material == packet.material).map(|b| b.mass).collect();
                    masses.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    if let Some(&lightest) = masses.first() {
                        let mut level = lightest;
                        for i in 0..masses.len() {
                            let target = masses.get(i + 1).copied().unwrap_or(max_mass).min(max_mass);
                            let needed = (target - level).max(0.0) * (i + 1) as f32;
                            if packet.mass <= needed {
                                level += packet.mass / (i + 1) as f32;
                                packet.mass = 0.0;
                                break;
                            }
                            packet.mass -= needed;
                            level = level.max(target);
                        }
                        for b in self.blocks.values_mut().filter(|b| b.material == packet.material && b.mass < level) {
                            b.mass = level;
                        }
                    }
                    if packet.mass < MIN_BLOCK_MASS {
                        continue;
                    }
                    let com_dist2 = |c: &Cell| (c[0] as f32 - self.com[0]).powi(2) + (c[1] as f32 - self.com[1]).powi(2);
                    let free_cell = self
                        .blocks
                        .keys()
                        .flat_map(|c| [[c[0] + 1, c[1]], [c[0] - 1, c[1]], [c[0], c[1] + 1], [c[0], c[1] - 1]])
                        .filter(|c| !self.blocks.contains_key(c))
                        .min_by(|a, b| com_dist2(a).partial_cmp(&com_dist2(b)).unwrap().then(a.cmp(b)));
                    if let Some(cell) = free_cell {
                        let take = consts::props(packet.material).max_mass.min(packet.mass);
                        self.blocks.insert(cell, Block::new(packet.material, take, 0.0));
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
                    self.blocks.insert(cell, Block::new(target_material, collected, 0.0));
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
        ship.blocks.insert([0, 0], Block::new(Material::Iron, 100.0, 50.0));
        ship.recompute_com_inertia();

        ship.apply_move_mass([0, 0], [1, 0], 100.0).unwrap();

        assert!(!ship.blocks.contains_key(&[0, 0]), "source block should be removed once empty");
        let to = ship.blocks.get(&[1, 0]).expect("target block created");
        approx(to.mass, 100.0, 1e-4);
        approx(to.energy, 50.0, 1e-4);
    }

    #[test]
    fn move_mass_partial_preserves_total_mass_and_takes_energy_share_along() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Copper, 200.0, 40.0));
        ship.blocks.insert([1, 0], Block::new(Material::Copper, 50.0, 10.0));
        ship.recompute_com_inertia();
        let total_before: f32 = ship.blocks.values().map(|b| b.mass).sum();

        ship.apply_move_mass([0, 0], [1, 0], 30.0).unwrap();

        let total_after: f32 = ship.blocks.values().map(|b| b.mass).sum();
        approx(total_after, total_before, 1e-3);
        approx(ship.blocks[&[0, 0]].mass, 170.0, 1e-4);
        approx(ship.blocks[&[1, 0]].mass, 80.0, 1e-4);
        // 30 of 200 kg moved, so 15% of the source's energy went along
        approx(ship.blocks[&[0, 0]].energy, 34.0, 1e-4);
        approx(ship.blocks[&[1, 0]].energy, 16.0, 1e-4);
    }

    #[test]
    fn energy_capacity_scales_with_mass() {
        let per_kg = consts::props(Material::Iron).energy_per_kg;
        approx(Block::new(Material::Iron, 100.0, 0.0).energy_capacity(), 100.0 * per_kg, 1e-2);
        approx(Block::new(Material::Iron, 200.0, 0.0).energy_capacity(), 200.0 * per_kg, 1e-2);
    }

    #[test]
    fn move_mass_rejects_mismatched_material_and_non_adjacent_target() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Iron, 100.0, 0.0));
        ship.blocks.insert([5, 5], Block::new(Material::Copper, 100.0, 0.0));
        ship.recompute_com_inertia();

        assert!(ship.apply_move_mass([0, 0], [5, 5], 10.0).is_err());
        assert!(ship.apply_move_mass([0, 0], [9, 9], 10.0).is_err());
    }

    #[test]
    fn collect_pulls_matching_packets_into_blocks() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Iron, 10.0, 0.0));
        ship.recompute_com_inertia();
        let mut packets = vec![Packet { pos: [0.0, 0.0], vel: [0.0, 0.0], material: Material::Iron, mass: 5.0, age: 0.0 }];

        ship.apply_collect(&mut packets, None, None).unwrap();

        approx(ship.blocks[&[0, 0]].mass, 15.0, 1e-4);
        assert!(packets.is_empty(), "packet should be fully consumed");
    }

    #[test]
    fn collect_builds_a_new_block_for_a_material_the_ship_lacks() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Iron, 10.0, 0.0));
        ship.recompute_com_inertia();
        let packet = |x: f32| Packet { pos: [x, 0.0], vel: [0.0, 0.0], material: Material::Copper, mass: 5.0, age: 0.0 };
        let mut packets = vec![packet(3.0), packet(4.0), packet(500.0)];

        ship.apply_collect(&mut packets, None, None).unwrap();

        // One new copper block takes both packets in range; the far one stays.
        let copper: Vec<&Block> = ship.blocks.values().filter(|b| b.material == Material::Copper).collect();
        assert_eq!(copper.len(), 1);
        approx(copper[0].mass, 10.0, 1e-4);
        assert_eq!(packets.len(), 1);
    }

    #[test]
    fn collect_fills_a_matching_block_that_is_itself_out_of_range() {
        // The copper block is 20 m from the packet, the iron block right next to it.
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Iron, 10.0, 0.0));
        ship.blocks.insert([20, 0], Block::new(Material::Copper, 10.0, 0.0));
        ship.recompute_com_inertia();
        let pos = ship.local_to_world([-2.0, 0.0]);
        let mut packets = vec![Packet { pos, vel: [0.0, 0.0], material: Material::Copper, mass: 5.0, age: 0.0 }];

        ship.apply_collect(&mut packets, None, None).unwrap();

        assert_eq!(ship.blocks.len(), 2, "no new block while a matching one has room");
        approx(ship.blocks[&[20, 0]].mass, 15.0, 1e-4);
    }

    #[test]
    fn collect_evens_out_the_masses_of_matching_blocks() {
        let lead = |masses: &[f32], collected: f32| {
            let mut ship = Ship::new(1, None);
            for (i, &m) in masses.iter().enumerate() {
                ship.blocks.insert([i as i32, 0], Block::new(Material::Lead, m, 0.0));
            }
            ship.recompute_com_inertia();
            let pos = ship.local_to_world([0.0, 2.0]);
            // Two packets, to check that the result does not depend on how the mass is split.
            let packet = Packet { pos, vel: [0.0, 0.0], material: Material::Lead, mass: collected / 2.0, age: 0.0 };
            let mut packets = vec![packet, packet];
            ship.apply_collect(&mut packets, None, None).unwrap();
            assert!(packets.is_empty());
            (0..masses.len()).map(|i| ship.blocks[&[i as i32, 0]].mass).collect::<Vec<f32>>()
        };
        let after = lead(&[50.0, 100.0], 150.0);
        approx(after[0], 150.0, 1e-3);
        approx(after[1], 150.0, 1e-3);
        // Too little to reach the heavier block: only the lighter one grows.
        let after = lead(&[50.0, 100.0, 400.0], 30.0);
        approx(after[0], 80.0, 1e-3);
        approx(after[1], 100.0, 1e-3);
        approx(after[2], 400.0, 1e-3);
    }

    #[test]
    fn collect_overflows_full_blocks_into_a_new_one() {
        let max = consts::props(Material::Lead).max_mass;
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Lead, max - 10.0, 0.0));
        ship.recompute_com_inertia();
        let mut packets = vec![Packet { pos: [0.0, 2.0], vel: [0.0, 0.0], material: Material::Lead, mass: 60.0, age: 0.0 }];

        ship.apply_collect(&mut packets, None, None).unwrap();

        approx(ship.blocks[&[0, 0]].mass, max, 1e-2);
        assert_eq!(ship.blocks.len(), 2);
        let total: f32 = ship.blocks.values().map(|b| b.mass).sum();
        approx(total, max + 50.0, 1e-2);
    }

    #[test]
    fn point_velocity_includes_rotation_component() {
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Iron, 100.0, 0.0));
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
