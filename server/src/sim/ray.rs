//! Raycasting: bounding-circle broadphase, then grid DDA in ship-local space.

use std::collections::HashMap;

use crate::protocol::{Cell, ShipId};
use crate::sim::ship::Ship;
use crate::sim::world::Sun;

#[derive(Debug, Clone, Copy)]
pub enum HitKind {
    Block(ShipId, Cell),
    Sun,
}

#[derive(Debug, Clone, Copy)]
pub struct RayHit {
    pub t: f32,
    pub point: [f32; 2],
    pub kind: HitKind,
}

/// Casts a ray from `origin` in direction `dir` (must be a unit vector) up to
/// `max_dist`, returning the nearest hit across all ships and suns.
/// `ignore_block` skips a specific ship's cell (the emitting block).
/// `ignore_sun` skips a specific sun by index (a sun emitting its own ray).
pub fn raycast(
    ships: &HashMap<ShipId, Ship>,
    suns: &[Sun],
    origin: [f32; 2],
    dir: [f32; 2],
    max_dist: f32,
    ignore_block: Option<(ShipId, Cell)>,
    ignore_sun: Option<usize>,
) -> Option<RayHit> {
    let mut best: Option<RayHit> = None;

    for (&id, ship) in ships.iter() {
        let radius = ship.bounding_radius();
        if radius <= 0.0 {
            continue;
        }
        let Some(entry) = ray_circle_entry(origin, dir, ship.pos, radius) else { continue };
        let limit = best.map(|b| b.t).unwrap_or(max_dist);
        if entry > limit {
            continue;
        }
        let local_origin = ship.world_to_local(origin);
        let local_dir = ship.world_dir_to_local(dir);
        let skip = ignore_block.and_then(|(sid, cell)| (sid == id).then_some(cell));
        if let Some((t, cell)) = dda(&ship.blocks, local_origin, local_dir, limit, skip) {
            if best.map(|b| t < b.t).unwrap_or(true) {
                best = Some(RayHit { t, point: [origin[0] + dir[0] * t, origin[1] + dir[1] * t], kind: HitKind::Block(id, cell) });
            }
        }
    }

    for (i, sun) in suns.iter().enumerate() {
        if ignore_sun == Some(i) {
            continue;
        }
        let limit = best.map(|b| b.t).unwrap_or(max_dist);
        if let Some(t) = ray_circle_entry(origin, dir, [sun.x, sun.y], sun.radius) {
            if t <= limit && best.map(|b| t < b.t).unwrap_or(true) {
                best = Some(RayHit { t, point: [origin[0] + dir[0] * t, origin[1] + dir[1] * t], kind: HitKind::Sun });
            }
        }
    }

    best
}

/// Nearest entry distance (t >= 0) where the ray hits the circle, or None.
fn ray_circle_entry(origin: [f32; 2], dir: [f32; 2], center: [f32; 2], radius: f32) -> Option<f32> {
    let oc = [origin[0] - center[0], origin[1] - center[1]];
    let b = oc[0] * dir[0] + oc[1] * dir[1];
    let c = oc[0] * oc[0] + oc[1] * oc[1] - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let sd = disc.sqrt();
    let t0 = -b - sd;
    let t1 = -b + sd;
    if t0 >= 0.0 {
        Some(t0)
    } else if t1 >= 0.0 {
        Some(0.0)
    } else {
        None
    }
}

/// Amanatides & Woo grid traversal over a ship's block grid (cell (i,j) spans
/// `[i-0.5, i+0.5] x [j-0.5, j+0.5]`). Returns the first occupied cell hit.
fn dda(
    blocks: &HashMap<Cell, crate::sim::ship::Block>,
    origin: [f32; 2],
    dir: [f32; 2],
    max_dist: f32,
    skip: Option<Cell>,
) -> Option<(f32, Cell)> {
    let ox = origin[0] + 0.5;
    let oy = origin[1] + 0.5;
    let mut ix = ox.floor() as i32;
    let mut iy = oy.floor() as i32;

    let step_x: i32 = if dir[0] > 0.0 { 1 } else if dir[0] < 0.0 { -1 } else { 0 };
    let step_y: i32 = if dir[1] > 0.0 { 1 } else if dir[1] < 0.0 { -1 } else { 0 };
    let t_delta_x = if dir[0] != 0.0 { (1.0 / dir[0]).abs() } else { f32::INFINITY };
    let t_delta_y = if dir[1] != 0.0 { (1.0 / dir[1]).abs() } else { f32::INFINITY };
    let mut t_max_x = if dir[0] != 0.0 {
        let bound = if step_x > 0 { (ix + 1) as f32 } else { ix as f32 };
        (bound - ox) / dir[0]
    } else {
        f32::INFINITY
    };
    let mut t_max_y = if dir[1] != 0.0 {
        let bound = if step_y > 0 { (iy + 1) as f32 } else { iy as f32 };
        (bound - oy) / dir[1]
    } else {
        f32::INFINITY
    };

    let mut t = 0.0f32;
    let max_iters = (max_dist as usize).saturating_mul(2) + 16;
    for _ in 0..max_iters {
        if t > max_dist {
            return None;
        }
        let cell = [ix, iy];
        if Some(cell) != skip && blocks.contains_key(&cell) {
            return Some((t.max(0.0), cell));
        }
        if t_max_x < t_max_y {
            t = t_max_x;
            ix += step_x;
            t_max_x += t_delta_x;
        } else {
            t = t_max_y;
            iy += step_y;
            t_max_y += t_delta_y;
        }
    }
    None
}
