//! World state and the per-tick simulation step.

use std::collections::{HashMap, HashSet};

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::protocol::{Cell, Dir, Material, RayView, ShipId};
use crate::sim::consts;
use crate::sim::ray::{self, HitKind};
use crate::sim::ship::{self, Ship};
use crate::sim::worldgen;

#[derive(Debug, Clone, Copy)]
pub struct Packet {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub material: Material,
    pub mass: f32,
    pub age: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Sun {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

#[derive(Debug, Clone)]
pub enum Command {
    MoveMass { ship: ShipId, from: Cell, to: Cell, kg: f32 },
    Emit { ship: ShipId, block: Cell, dir: Dir, energy: f32, mass: f32 },
    Collect { ship: ShipId, block: Option<Cell>, material: Option<Material> },
}

pub struct World {
    pub tick: u64,
    pub seed: u64,
    pub rng: ChaCha8Rng,
    pub ships: HashMap<ShipId, Ship>,
    pub next_ship_id: ShipId,
    pub packets: Vec<Packet>,
    /// Rays emitted this tick, for the client to draw.
    pub rays: Vec<RayView>,
    pub sun_cache: HashMap<(i32, i32), Option<Sun>>,
    /// Commands queued by the network layer, applied at the next tick.
    pub pending: Vec<(String, Command)>,
    emit_queue: Vec<(ShipId, Cell, Dir, f32, f32)>,
    /// (player, message) pairs produced by failed commands this tick.
    pub errors: Vec<(String, String)>,
}

/// Max distance a ray can travel before falloff drops below `MIN_FALLOFF`.
pub fn max_ray_range() -> f32 {
    consts::FALLOFF_DISTANCE * (1.0 / consts::MIN_FALLOFF - 1.0).sqrt()
}

fn rotate(rot: f32, v: [f32; 2]) -> [f32; 2] {
    let (s, c) = rot.sin_cos();
    [c * v[0] - s * v[1], s * v[0] + c * v[1]]
}

fn local_to_world_raw(pos: [f32; 2], rot: f32, com: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    let rel = [p[0] - com[0], p[1] - com[1]];
    let (s, c) = rot.sin_cos();
    [pos[0] + c * rel[0] - s * rel[1], pos[1] + s * rel[0] + c * rel[1]]
}

fn point_velocity_raw(pos: [f32; 2], rot: f32, com: [f32; 2], vel: [f32; 2], omega: f32, p: [f32; 2]) -> [f32; 2] {
    let rel = [p[0] - com[0], p[1] - com[1]];
    let (s, c) = rot.sin_cos();
    let rx = c * rel[0] - s * rel[1];
    let ry = s * rel[0] + c * rel[1];
    let _ = pos; // pos not needed for velocity, kept for symmetry with local_to_world_raw
    [vel[0] - omega * ry, vel[1] + omega * rx]
}

fn face_offset(cell: Cell, dir: Dir) -> Option<[f32; 2]> {
    let (i, j) = (cell[0] as f32, cell[1] as f32);
    match dir {
        Dir::N => Some([i, j + 0.5]),
        Dir::E => Some([i + 0.5, j]),
        Dir::S => Some([i, j - 0.5]),
        Dir::W => Some([i - 0.5, j]),
        Dir::All => None,
    }
}

fn dir_vec(dir: Dir) -> Option<[f32; 2]> {
    match dir {
        Dir::N => Some([0.0, 1.0]),
        Dir::E => Some([1.0, 0.0]),
        Dir::S => Some([0.0, -1.0]),
        Dir::W => Some([-1.0, 0.0]),
        Dir::All => None,
    }
}

fn chunk_range(center: [f32; 2], radius: f32) -> Vec<(i32, i32)> {
    let min_cx = ((center[0] - radius) / consts::CHUNK_SIZE).floor() as i32;
    let max_cx = ((center[0] + radius) / consts::CHUNK_SIZE).floor() as i32;
    let min_cy = ((center[1] - radius) / consts::CHUNK_SIZE).floor() as i32;
    let max_cy = ((center[1] + radius) / consts::CHUNK_SIZE).floor() as i32;
    let mut v = Vec::new();
    for cx in min_cx..=max_cx {
        for cy in min_cy..=max_cy {
            v.push((cx, cy));
        }
    }
    v
}

impl World {
    pub fn new(seed: u64) -> Self {
        World {
            tick: 0,
            seed,
            rng: ChaCha8Rng::seed_from_u64(seed ^ 0xA5A5_A5A5_A5A5_A5A5),
            ships: HashMap::new(),
            next_ship_id: 1,
            packets: Vec::new(),
            rays: Vec::new(),
            sun_cache: HashMap::new(),
            pending: Vec::new(),
            emit_queue: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn queue_command(&mut self, player: String, cmd: Command) {
        self.pending.push((player, cmd));
    }

    pub fn spawn_starter_ship(&mut self, owner: String) -> ShipId {
        worldgen::make_starter_ship(self, owner)
    }

    fn get_or_gen_sun(&mut self, cx: i32, cy: i32) -> Option<Sun> {
        if let Some(cached) = self.sun_cache.get(&(cx, cy)) {
            return *cached;
        }
        let sun = worldgen::sun_in_chunk(self.seed, cx, cy);
        self.sun_cache.insert((cx, cy), sun);
        sun
    }

    /// Suns within `radius` of `center`, generating/caching chunks as needed.
    pub fn suns_in_view(&mut self, center: [f32; 2], radius: f32) -> Vec<Sun> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        for (cx, cy) in chunk_range(center, radius + consts::SUN_RADIUS_MAX) {
            if visited.insert((cx, cy)) {
                if let Some(sun) = self.get_or_gen_sun(cx, cy) {
                    if ship::dist([sun.x, sun.y], center) <= radius + sun.radius {
                        result.push(sun);
                    }
                }
            }
        }
        result
    }

    fn compute_active_suns(&mut self) -> Vec<Sun> {
        let range = max_ray_range() + consts::SUN_RADIUS_MAX;
        let positions: Vec<[f32; 2]> = self.ships.values().map(|s| s.pos).collect();
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        for pos in positions {
            for (cx, cy) in chunk_range(pos, range) {
                if visited.insert((cx, cy)) {
                    if let Some(sun) = self.get_or_gen_sun(cx, cy) {
                        result.push(sun);
                    }
                }
            }
        }
        result
    }

    fn omni_dirs(&mut self, n: usize) -> Vec<[f32; 2]> {
        let offset: f32 = self.rng.random_range(0.0..std::f32::consts::TAU);
        (0..n)
            .map(|k| {
                let a = offset + (k as f32) * std::f32::consts::TAU / (n as f32);
                [a.cos(), a.sin()]
            })
            .collect()
    }

    fn cast_and_apply_ray(
        &mut self,
        suns: &[Sun],
        origin: [f32; 2],
        dir: [f32; 2],
        energy: f32,
        ignore_block: Option<(ShipId, Cell)>,
        ignore_sun: Option<usize>,
        beam: bool,
    ) {
        let max_dist = max_ray_range();
        let hit = ray::raycast(&self.ships, suns, origin, dir, max_dist, ignore_block, ignore_sun);
        match hit {
            Some(h) => {
                let factor = 1.0 / (1.0 + (h.t / consts::FALLOFF_DISTANCE).powi(2));
                let delivered = if factor < consts::MIN_FALLOFF { 0.0 } else { energy * factor };
                if delivered > 0.0 {
                    if let HitKind::Block(sid, cell) = h.kind {
                        if let Some(ship) = self.ships.get_mut(&sid) {
                            if let Some(b) = ship.blocks.get_mut(&cell) {
                                b.energy += delivered;
                            }
                        }
                    }
                }
                self.rays.push(RayView { x1: origin[0], y1: origin[1], x2: h.point[0], y2: h.point[1], energy: delivered, emitted: energy, beam });
            }
            None => {
                let end = [origin[0] + dir[0] * max_dist, origin[1] + dir[1] * max_dist];
                self.rays.push(RayView { x1: origin[0], y1: origin[1], x2: end[0], y2: end[1], energy: 0.0, emitted: energy, beam });
            }
        }
    }

    fn apply_pending(&mut self) {
        self.errors.clear();
        let cmds = std::mem::take(&mut self.pending);
        for (player, cmd) in cmds {
            match cmd {
                Command::MoveMass { ship, from, to, kg } => {
                    if let Some(s) = self.ships.get_mut(&ship) {
                        if let Err(e) = s.apply_move_mass(from, to, kg) {
                            self.errors.push((player, e));
                        }
                    }
                }
                Command::Collect { ship, block, material } => {
                    if let Some(s) = self.ships.get_mut(&ship) {
                        if let Err(e) = s.apply_collect(&mut self.packets, block, material) {
                            self.errors.push((player, e));
                        }
                    }
                }
                Command::Emit { ship, block, dir, energy, mass } => {
                    self.emit_queue.push((ship, block, dir, energy, mass));
                }
            }
        }
    }

    fn apply_uranium(&mut self) {
        for ship in self.ships.values_mut() {
            let mut changed = false;
            for block in ship.blocks.values_mut() {
                if block.material == Material::Uranium {
                    let consume = consts::URANIUM_KG_PER_TICK.min((block.mass - consts::MIN_BLOCK_MASS).max(0.0));
                    if consume > 0.0 {
                        block.mass -= consume;
                        block.energy += consume * consts::URANIUM_J_PER_KG;
                        changed = true;
                    }
                }
            }
            if changed {
                ship.recompute_com_inertia();
            }
        }
    }

    fn process_emit(&mut self, suns: &[Sun], ship_id: ShipId, cell: Cell, dir: Dir, req_energy: f32, req_mass: f32) {
        let Some((mass0, energy0, material, ship_pos, ship_rot, ship_com, ship_vel, ship_omega)) =
            self.ships.get(&ship_id).and_then(|s| {
                s.blocks.get(&cell).map(|b| (b.mass, b.energy, b.material, s.pos, s.rot, s.com, s.vel, s.omega))
            })
        else {
            return;
        };
        if req_energy >= consts::SIGNAL_MIN_ENERGY {
            if let Some(b) = self.ships.get_mut(&ship_id).and_then(|s| s.blocks.get_mut(&cell)) {
                if material == Material::Silicon {
                    b.dir = (dir != Dir::All).then_some(dir);
                }
            }
        }
        let props = consts::props(material);
        let energy_amt = req_energy.max(0.0).min(energy0).min(props.energy_emit_rate);
        let remaining_energy = (energy0 - energy_amt).max(0.0);
        let max_mass_by_energy =
            if consts::EMIT_ENERGY_PER_KG > 0.0 { remaining_energy / consts::EMIT_ENERGY_PER_KG } else { f32::INFINITY };
        let mass_amt = req_mass.max(0.0).min(props.mass_emit_rate).min(mass0).min(max_mass_by_energy).max(0.0);
        let energy_cost_mass = mass_amt * consts::EMIT_ENERGY_PER_KG;

        if let Some(ship) = self.ships.get_mut(&ship_id) {
            if let Some(b) = ship.blocks.get_mut(&cell) {
                b.energy -= energy_amt + energy_cost_mass;
                b.mass -= mass_amt;
            }
            let dead = ship.blocks.get(&cell).map(|b| b.mass <= 1e-6).unwrap_or(false);
            if dead {
                ship.blocks.remove(&cell);
            }
            ship.recompute_com_inertia();
        }

        if energy_amt > 0.0 {
            let center_local = [cell[0] as f32, cell[1] as f32];
            match dir {
                Dir::All => {
                    let dirs = self.omni_dirs(consts::OMNI_RAYS);
                    let per = energy_amt / consts::OMNI_RAYS as f32;
                    let world_origin = local_to_world_raw(ship_pos, ship_rot, ship_com, center_local);
                    for d in dirs {
                        let world_dir = rotate(ship_rot, d);
                        self.cast_and_apply_ray(suns, world_origin, world_dir, per, Some((ship_id, cell)), None, true);
                    }
                }
                _ => {
                    let local_dir = dir_vec(dir).unwrap();
                    let local_origin = face_offset(cell, dir).unwrap();
                    let world_origin = local_to_world_raw(ship_pos, ship_rot, ship_com, local_origin);
                    let world_dir = rotate(ship_rot, local_dir);
                    self.cast_and_apply_ray(suns, world_origin, world_dir, energy_amt, Some((ship_id, cell)), None, true);
                }
            }
        }

        if mass_amt > 0.0 {
            let center_local = [cell[0] as f32, cell[1] as f32];
            let point_vel = point_velocity_raw(ship_pos, ship_rot, ship_com, ship_vel, ship_omega, center_local);
            let world_point = local_to_world_raw(ship_pos, ship_rot, ship_com, center_local);
            match dir {
                Dir::All => {
                    let dirs = self.omni_dirs(consts::BURST_PACKETS);
                    let per = mass_amt / consts::BURST_PACKETS as f32;
                    for d in dirs {
                        let world_dir = rotate(ship_rot, d);
                        let vel = [
                            point_vel[0] + world_dir[0] * consts::EXHAUST_SPEED,
                            point_vel[1] + world_dir[1] * consts::EXHAUST_SPEED,
                        ];
                        self.packets.push(Packet { pos: world_point, vel, material, mass: per, age: 0.0 });
                    }
                }
                _ => {
                    let local_dir = dir_vec(dir).unwrap();
                    let world_dir = rotate(ship_rot, local_dir);
                    let vel = [
                        point_vel[0] + world_dir[0] * consts::EXHAUST_SPEED,
                        point_vel[1] + world_dir[1] * consts::EXHAUST_SPEED,
                    ];
                    self.packets.push(Packet { pos: world_point, vel, material, mass: mass_amt, age: 0.0 });
                    let impulse = [-mass_amt * consts::EXHAUST_SPEED * world_dir[0], -mass_amt * consts::EXHAUST_SPEED * world_dir[1]];
                    if let Some(ship) = self.ships.get_mut(&ship_id) {
                        ship.apply_impulse(world_point, impulse);
                    }
                }
            }
        }
    }

    fn apply_sun_emissions(&mut self, suns: &[Sun]) {
        let per = consts::SUN_POWER / consts::SUN_RAYS as f32;
        for (i, sun) in suns.iter().enumerate() {
            for _ in 0..consts::SUN_RAYS {
                let angle: f32 = self.rng.random_range(0.0..std::f32::consts::TAU);
                let (s, c) = angle.sin_cos();
                let origin = [sun.x + sun.radius * c, sun.y + sun.radius * s];
                self.cast_and_apply_ray(suns, origin, [c, s], per, None, Some(i), false);
            }
        }
    }

    fn apply_conduction(&mut self) {
        for ship in self.ships.values_mut() {
            let scale = if ship.owner.is_none() { consts::ASTEROID_CONDUCTION } else { 1.0 };
            let mut deltas: HashMap<Cell, f32> = HashMap::new();
            for (&cell, a) in &ship.blocks {
                let cap_a = consts::props(a.material).energy_capacity;
                let cond_a = consts::props(a.material).conductance;
                let fill_a = a.energy / cap_a;
                for (dx, dy, pair_dir) in [(1, 0, Dir::E), (0, 1, Dir::N)] {
                    let ncell = [cell[0] + dx, cell[1] + dy];
                    if let Some(nb) = ship.blocks.get(&ncell) {
                        let cap_b = consts::props(nb.material).energy_capacity;
                        let cond_b = consts::props(nb.material).conductance;
                        let fill_b = nb.energy / cap_b;
                        let k = cond_a.min(cond_b) * scale;
                        let flow = k * (fill_a - fill_b) * cap_a.min(cap_b) / 2.0;
                        // Silicon only lets energy flow along its direction.
                        let (a_fwd, a_back) = a.conduction_gate(pair_dir);
                        let (b_fwd, b_back) = nb.conduction_gate(pair_dir);
                        let allowed = if flow > 0.0 { a_fwd && b_fwd } else { a_back && b_back };
                        if !allowed {
                            continue;
                        }
                        *deltas.entry(cell).or_insert(0.0) -= flow;
                        *deltas.entry(ncell).or_insert(0.0) += flow;
                    }
                }
            }
            for (cell, d) in deltas {
                if let Some(b) = ship.blocks.get_mut(&cell) {
                    b.energy = (b.energy + d).max(0.0);
                }
            }
        }
    }

    fn apply_asteroid_cooling(&mut self) {
        for ship in self.ships.values_mut().filter(|s| s.owner.is_none()) {
            for block in ship.blocks.values_mut() {
                block.energy = (block.energy - consts::ASTEROID_COOLING).max(0.0);
            }
        }
    }

    fn process_bursts(&mut self, suns: &[Sun]) {
        struct BurstEvent {
            ship_id: ShipId,
            cell: Cell,
            material: Material,
            mass: f32,
            energy: f32,
            world_point: [f32; 2],
            ship_vel: [f32; 2],
        }
        let mut events = Vec::new();
        for (&id, ship) in self.ships.iter() {
            for (&cell, block) in ship.blocks.iter() {
                let cap = consts::props(block.material).energy_capacity;
                if block.energy > cap {
                    events.push(BurstEvent {
                        ship_id: id,
                        cell,
                        material: block.material,
                        mass: block.mass,
                        energy: block.energy,
                        world_point: ship.local_to_world([cell[0] as f32, cell[1] as f32]),
                        ship_vel: ship.vel,
                    });
                }
            }
        }
        if events.is_empty() {
            return;
        }
        let mut affected: HashSet<ShipId> = HashSet::new();
        for e in &events {
            if let Some(ship) = self.ships.get_mut(&e.ship_id) {
                ship.blocks.remove(&e.cell);
                affected.insert(e.ship_id);
            }
        }
        for id in &affected {
            if let Some(ship) = self.ships.get_mut(id) {
                ship.recompute_com_inertia();
            }
        }
        for e in events {
            let dirs = self.omni_dirs(consts::BURST_PACKETS);
            let per_mass = e.mass / consts::BURST_PACKETS as f32;
            for d in &dirs {
                self.packets.push(Packet {
                    pos: e.world_point,
                    vel: [e.ship_vel[0] + d[0] * consts::BURST_SPEED, e.ship_vel[1] + d[1] * consts::BURST_SPEED],
                    material: e.material,
                    mass: per_mass,
                    age: 0.0,
                });
            }
            let edirs = self.omni_dirs(consts::OMNI_RAYS);
            let e_per = e.energy / consts::OMNI_RAYS as f32;
            for d in &edirs {
                self.cast_and_apply_ray(suns, e.world_point, *d, e_per, None, None, false);
            }
        }
    }

    fn integrate(&mut self) {
        for ship in self.ships.values_mut() {
            ship.pos[0] += ship.vel[0] * consts::DT;
            ship.pos[1] += ship.vel[1] * consts::DT;
            ship.rot += ship.omega * consts::DT;
            ship.vel[0] *= 1.0 - consts::LINEAR_DAMPING;
            ship.vel[1] *= 1.0 - consts::LINEAR_DAMPING;
            ship.omega *= 1.0 - consts::ANGULAR_DAMPING;
        }
    }

    fn step_packets(&mut self) {
        for p in self.packets.iter_mut() {
            p.pos[0] += p.vel[0] * consts::DT;
            p.pos[1] += p.vel[1] * consts::DT;
            p.age += consts::DT;
        }
        self.packets.retain(|p| p.age < consts::PACKET_TTL);
    }

    fn update_asteroids(&mut self) {
        let player_positions: Vec<[f32; 2]> = self.ships.values().filter(|s| s.owner.is_some()).map(|s| s.pos).collect();
        if player_positions.is_empty() {
            return;
        }
        let to_remove: Vec<ShipId> = self
            .ships
            .iter()
            .filter(|(_, s)| s.owner.is_none() && player_positions.iter().all(|p| ship::dist(*p, s.pos) > consts::ASTEROID_DESPAWN))
            .map(|(id, _)| *id)
            .collect();
        for id in to_remove {
            self.ships.remove(&id);
        }
        for ppos in player_positions {
            let count = self
                .ships
                .values()
                .filter(|s| s.owner.is_none() && ship::dist(ppos, s.pos) <= consts::ASTEROID_SPAWN_MAX)
                .count();
            if count < consts::ASTEROIDS_PER_PLAYER {
                self.spawn_asteroid_near(ppos);
            }
        }
    }

    fn spawn_asteroid_near(&mut self, near: [f32; 2]) {
        for _ in 0..10 {
            let angle: f32 = self.rng.random_range(0.0..std::f32::consts::TAU);
            let d: f32 = self.rng.random_range(consts::ASTEROID_SPAWN_MIN..consts::ASTEROID_SPAWN_MAX);
            let pos = [near[0] + angle.cos() * d, near[1] + angle.sin() * d];
            let suns = self.suns_in_view(pos, consts::SUN_RADIUS_MAX + 20.0);
            if suns.iter().any(|s| ship::dist([s.x, s.y], pos) < s.radius + 20.0) {
                continue;
            }
            let id = self.next_ship_id;
            self.next_ship_id += 1;
            let ship = worldgen::build_asteroid(&mut self.rng, id, pos);
            self.ships.insert(id, ship);
            return;
        }
    }

    pub fn step(&mut self) {
        self.tick += 1;
        self.rays.clear();
        self.apply_pending();
        self.apply_uranium();
        let suns = self.compute_active_suns();
        let emit_queue = std::mem::take(&mut self.emit_queue);
        for (ship, block, dir, energy, mass) in emit_queue {
            self.process_emit(&suns, ship, block, dir, energy, mass);
        }
        self.apply_sun_emissions(&suns);
        self.apply_conduction();
        self.apply_asteroid_cooling();
        self.process_bursts(&suns);
        self.integrate();
        self.step_packets();
        self.update_asteroids();
        self.ships.retain(|_, s| !s.blocks.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::ship::Block;

    fn approx(a: f32, b: f32, eps: f32) {
        assert!((a - b).abs() < eps, "expected {b}, got {a}");
    }

    /// A single-block ship placed at `pos`, block centred at local (0,0).
    fn single_block_ship(w: &mut World, id: ShipId, pos: [f32; 2], material: Material, mass: f32, energy: f32) {
        let mut ship = Ship::new(id, None);
        ship.blocks.insert([0, 0], Block::new(material, mass, energy));
        ship.recompute_com_inertia();
        ship.pos = pos;
        w.ships.insert(id, ship);
    }

    #[test]
    fn conduction_conserves_energy_and_flows_high_to_low() {
        let mut w = World::new(1);
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Copper, 100.0, 1_000_000.0));
        ship.blocks.insert([1, 0], Block::new(Material::Copper, 100.0, 0.0));
        ship.recompute_com_inertia();
        w.ships.insert(1, ship);
        let total_before: f32 = w.ships[&1].blocks.values().map(|b| b.energy).sum();

        w.apply_conduction();

        let ship = &w.ships[&1];
        let total_after: f32 = ship.blocks.values().map(|b| b.energy).sum();
        approx(total_after, total_before, 1.0);
        assert!(ship.blocks[&[0, 0]].energy < 1_000_000.0, "high-fill block should lose energy");
        assert!(ship.blocks[&[1, 0]].energy > 0.0, "low-fill block should gain energy");
    }

    #[test]
    fn plastic_conducts_much_slower_than_copper() {
        let mut w = World::new(1);
        let mut copper = Ship::new(1, None);
        copper.blocks.insert([0, 0], Block::new(Material::Copper, 100.0, 1_000_000.0));
        copper.blocks.insert([1, 0], Block::new(Material::Copper, 100.0, 0.0));
        copper.recompute_com_inertia();
        w.ships.insert(1, copper);

        let mut plastic = Ship::new(2, None);
        plastic.blocks.insert([0, 0], Block::new(Material::Plastic, 100.0, 400_000.0));
        plastic.blocks.insert([1, 0], Block::new(Material::Plastic, 100.0, 0.0));
        plastic.recompute_com_inertia();
        w.ships.insert(2, plastic);

        w.apply_conduction();

        let copper_flow = w.ships[&1].blocks[&[1, 0]].energy;
        let plastic_flow = w.ships[&2].blocks[&[1, 0]].energy;
        assert!(copper_flow > plastic_flow * 20.0, "copper ({copper_flow}) should conduct much faster than plastic ({plastic_flow})");
    }

    #[test]
    fn uranium_produces_energy_and_loses_mass() {
        let mut w = World::new(1);
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Uranium, 1000.0, 0.0));
        ship.recompute_com_inertia();
        w.ships.insert(1, ship);

        w.apply_uranium();

        let b = w.ships[&1].blocks[&[0, 0]];
        approx(b.mass, 1000.0 - consts::URANIUM_KG_PER_TICK, 1e-6);
        approx(b.energy, consts::URANIUM_KG_PER_TICK * consts::URANIUM_J_PER_KG, 1e-3);
    }

    #[test]
    fn directed_laser_delivers_falloff_correct_energy_at_distance() {
        let mut w = World::new(1);
        single_block_ship(&mut w, 1, [0.0, 0.0], Material::Iron, 1000.0, 1_000_000.0);
        single_block_ship(&mut w, 2, [60.0, 0.0], Material::Iron, 1000.0, 0.0);

        w.process_emit(&[], 1, [0, 0], Dir::E, 100_000.0, 0.0);

        let ray = w.rays.last().expect("a ray was recorded");
        let t = ship::dist([ray.x1, ray.y1], [ray.x2, ray.y2]);
        assert!((55.0..65.0).contains(&t), "expected to hit near ship B, t={t}");
        let expected = 100_000.0 / (1.0 + (t / consts::FALLOFF_DISTANCE).powi(2));
        approx(ray.energy, expected, 1.0);
        approx(w.ships[&2].blocks[&[0, 0]].energy, expected, 1.0);
    }

    #[test]
    fn ray_beyond_max_range_delivers_nothing() {
        let mut w = World::new(1);
        let far = max_ray_range() + 100.0;
        single_block_ship(&mut w, 1, [0.0, 0.0], Material::Iron, 1000.0, 1_000_000.0);
        single_block_ship(&mut w, 2, [far, 0.0], Material::Iron, 1000.0, 0.0);

        w.process_emit(&[], 1, [0, 0], Dir::E, 100_000.0, 0.0);

        let ray = w.rays.last().expect("a ray was recorded");
        approx(ray.energy, 0.0, 1e-6);
        approx(w.ships[&2].blocks[&[0, 0]].energy, 0.0, 1e-6);
    }

    #[test]
    fn close_range_laser_eventually_bursts_plastic_and_makes_packets() {
        let mut w = World::new(1);
        single_block_ship(&mut w, 1, [0.0, 0.0], Material::Iron, 1000.0, 1_000_000.0);
        single_block_ship(&mut w, 2, [2.0, 0.0], Material::Plastic, 100.0, 0.0);

        let mut burst = false;
        for _ in 0..50 {
            if let Some(ship) = w.ships.get_mut(&1) {
                ship.blocks.get_mut(&[0, 0]).unwrap().energy = 1_000_000.0;
            }
            w.process_emit(&[], 1, [0, 0], Dir::E, 100_000.0, 0.0);
            w.process_bursts(&[]);
            if !w.ships.get(&2).map(|s| s.blocks.contains_key(&[0, 0])).unwrap_or(false) {
                burst = true;
                break;
            }
        }
        assert!(burst, "plastic block should have burst");
        assert!(!w.packets.is_empty(), "burst should produce mass packets");
        assert!(w.packets.iter().any(|p| p.material == Material::Plastic));
    }

    #[test]
    fn directed_mass_emission_gives_thrust_and_costs_energy() {
        let mut w = World::new(1);
        single_block_ship(&mut w, 1, [0.0, 0.0], Material::Iron, 1000.0, 1_000_000.0);

        w.process_emit(&[], 1, [0, 0], Dir::E, 0.0, 1.0);

        let ship = &w.ships[&1];
        assert!(ship.vel[0] < -0.5, "thrust east should push the ship west, vel={:?}", ship.vel);
        approx(ship.vel[1], 0.0, 1e-3);
        let energy_left = ship.blocks[&[0, 0]].energy;
        approx(1_000_000.0 - energy_left, consts::EMIT_ENERGY_PER_KG, 50.0);
        assert_eq!(w.packets.len(), 1);
        assert!(w.packets[0].vel[0] > 500.0, "packet should move fast in +x");
    }

    #[test]
    fn off_centre_thrust_produces_rotation() {
        let mut w = World::new(1);
        let mut ship = Ship::new(1, None);
        ship.blocks.insert([0, 0], Block::new(Material::Lead, 1000.0, 0.0));
        ship.blocks.insert([1, 0], Block::new(Material::Lead, 1000.0, 1_000_000.0));
        ship.recompute_com_inertia();
        w.ships.insert(1, ship);

        w.process_emit(&[], 1, [1, 0], Dir::N, 0.0, 1.0);

        assert_ne!(w.ships[&1].omega, 0.0, "off-centre thrust should spin the ship");
    }

    #[test]
    fn asteroids_cool_passively_but_player_ships_do_not() {
        let mut w = World::new(1);
        single_block_ship(&mut w, 1, [0.0, 0.0], Material::Iron, 1000.0, 1_000_000.0);
        single_block_ship(&mut w, 2, [50.0, 0.0], Material::Iron, 1000.0, 1_000_000.0);
        w.ships.get_mut(&2).unwrap().owner = Some("p".into());

        w.apply_asteroid_cooling();

        approx(w.ships[&1].blocks[&[0, 0]].energy, 1_000_000.0 - consts::ASTEROID_COOLING, 1.0);
        approx(w.ships[&2].blocks[&[0, 0]].energy, 1_000_000.0, 1e-3);
    }

    #[test]
    fn missed_player_beam_is_still_reported() {
        let mut w = World::new(1);
        single_block_ship(&mut w, 1, [0.0, 0.0], Material::Iron, 1000.0, 1_000_000.0);

        w.process_emit(&[], 1, [0, 0], Dir::E, 100_000.0, 0.0);

        let ray = w.rays.last().expect("a ray was recorded");
        assert!(ray.beam);
        approx(ray.emitted, 100_000.0, 1.0);
        approx(ray.energy, 0.0, 1e-6);
    }

    /// Ship 1: a full copper block at (0,0), `middle` at (1,0), an empty copper block at (2,0).
    fn three_block_row(w: &mut World, middle: Material) {
        let mut ship = Ship::new(1, Some("p".into()));
        ship.blocks.insert([0, 0], Block::new(Material::Copper, 100.0, 1_000_000.0));
        ship.blocks.insert([1, 0], Block::new(middle, 100.0, 0.0));
        ship.blocks.insert([2, 0], Block::new(Material::Copper, 100.0, 0.0));
        ship.recompute_com_inertia();
        w.ships.insert(1, ship);
    }

    fn energy_at(w: &World, cell: Cell) -> f32 {
        w.ships[&1].blocks[&cell].energy
    }

    #[test]
    fn weak_emit_request_is_not_a_signal() {
        let mut w = World::new(1);
        three_block_row(&mut w, Material::Silicon);

        w.process_emit(&[], 1, [1, 0], Dir::E, 0.5, 0.0);

        assert_eq!(w.ships[&1].blocks[&[1, 0]].dir, None);
    }

    #[test]
    fn silicon_conducts_one_way_along_its_direction() {
        // Unoriented: isolator.
        let mut w = World::new(1);
        three_block_row(&mut w, Material::Silicon);
        w.apply_conduction();
        approx(energy_at(&w, [1, 0]), 0.0, 1e-6);

        // Pointing east: takes from the west block, passes to the east block.
        w.process_emit(&[], 1, [1, 0], Dir::E, 1.0, 0.0);
        w.apply_conduction();
        w.apply_conduction();
        assert!(energy_at(&w, [2, 0]) > 0.0, "energy should flow along the silicon direction");

        // Pointing west: the full block is now "ahead", so nothing flows out of it.
        let mut w = World::new(1);
        three_block_row(&mut w, Material::Silicon);
        w.process_emit(&[], 1, [1, 0], Dir::W, 1.0, 0.0);
        w.apply_conduction();
        approx(energy_at(&w, [0, 0]), 1_000_000.0, 1e-3);

        // Pointing north: both neighbours are to the side.
        let mut w = World::new(1);
        three_block_row(&mut w, Material::Silicon);
        w.process_emit(&[], 1, [1, 0], Dir::N, 1.0, 0.0);
        w.apply_conduction();
        approx(energy_at(&w, [0, 0]), 1_000_000.0, 1e-3);

        // `all` switches it off again.
        w.process_emit(&[], 1, [1, 0], Dir::All, 1.0, 0.0);
        assert_eq!(w.ships[&1].blocks[&[1, 0]].dir, None);
    }

    #[test]
    fn damping_slows_ships() {
        let mut w = World::new(1);
        single_block_ship(&mut w, 1, [0.0, 0.0], Material::Iron, 1000.0, 0.0);
        w.ships.get_mut(&1).unwrap().vel = [10.0, 0.0];
        w.ships.get_mut(&1).unwrap().omega = 1.0;

        w.integrate();

        let ship = &w.ships[&1];
        approx(ship.vel[0], 10.0 * (1.0 - consts::LINEAR_DAMPING), 1e-4);
        approx(ship.omega, 1.0 * (1.0 - consts::ANGULAR_DAMPING), 1e-4);
    }
}
