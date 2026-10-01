//! Persistence: player ships and the player list are written to a JSON file
//! every `SAVE_INTERVAL` and read back at server start.
//!
//! Asteroids, mass packets and the rng state are not saved: suns come from the
//! seed, and asteroids respawn around players on their own.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::net::{Server, SharedServer};
use crate::protocol::{Cell, Dir, Material, ShipId};
use crate::sim::ship::{Block, Ship};

pub const SAVE_INTERVAL: Duration = Duration::from_secs(180);

#[derive(Serialize, Deserialize)]
struct SavedBlock {
    p: Cell,
    m: Material,
    mass: f32,
    energy: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dir: Option<Dir>,
}

#[derive(Serialize, Deserialize)]
struct SavedShip {
    id: ShipId,
    owner: String,
    pos: [f32; 2],
    rot: f32,
    vel: [f32; 2],
    omega: f32,
    blocks: Vec<SavedBlock>,
}

#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    seed: u64,
    tick: u64,
    next_ship_id: ShipId,
    /// All known players, also those without ships (spectators).
    players: Vec<String>,
    ships: Vec<SavedShip>,
}

impl Server {
    /// Snapshot of everything that is persisted.
    pub fn to_save(&self) -> SaveFile {
        let mut players: Vec<String> = self.players.iter().cloned().collect();
        players.sort();
        let mut ships: Vec<SavedShip> = self
            .world
            .ships
            .values()
            .filter_map(|s| {
                let mut blocks: Vec<SavedBlock> = s
                    .blocks
                    .iter()
                    .map(|(&p, b)| SavedBlock { p, m: b.material, mass: b.mass, energy: b.energy, dir: b.dir })
                    .collect();
                blocks.sort_by_key(|b| b.p);
                Some(SavedShip { id: s.id, owner: s.owner.clone()?, pos: s.pos, rot: s.rot, vel: s.vel, omega: s.omega, blocks })
            })
            .collect();
        ships.sort_by_key(|s| s.id);
        SaveFile { seed: self.world.seed, tick: self.world.tick, next_ship_id: self.world.next_ship_id, players, ships }
    }

    /// Rebuilds a server from a snapshot. The world keeps the saved seed, so
    /// the suns are where they were.
    pub fn from_save(save: SaveFile) -> Self {
        let mut server = Server::new(save.seed);
        server.world.tick = save.tick;
        server.world.next_ship_id = save.next_ship_id;
        server.players = save.players.into_iter().collect();
        for saved in save.ships {
            let mut ship = Ship::new(saved.id, Some(saved.owner));
            for b in saved.blocks {
                ship.blocks.insert(b.p, Block { material: b.m, mass: b.mass, energy: b.energy, dir: b.dir });
            }
            // Recomputing shifts `pos` along with the centre of mass, so set it afterwards.
            ship.recompute_com_inertia();
            ship.pos = saved.pos;
            ship.rot = saved.rot;
            ship.vel = saved.vel;
            ship.omega = saved.omega;
            server.world.next_ship_id = server.world.next_ship_id.max(ship.id + 1);
            server.world.ships.insert(ship.id, ship);
        }
        server
    }
}

/// Reads a save file. `Ok(None)` if there is none yet.
pub fn load(path: &Path) -> Result<Option<Server>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let save: SaveFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(Some(Server::from_save(save)))
}

/// Writes the current state to `path`. The lock is only held for the snapshot;
/// the file is replaced by a rename, so a crash never leaves half a save.
pub fn save(server: &SharedServer, path: &Path) -> std::io::Result<()> {
    let snapshot = server.lock().unwrap().to_save();
    let text = serde_json::to_string(&snapshot)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Saves every `SAVE_INTERVAL`, forever.
pub async fn run_save_loop(server: SharedServer, path: PathBuf) {
    let mut interval = tokio::time::interval(SAVE_INTERVAL);
    interval.tick().await; // the first tick fires at once
    loop {
        interval.tick().await;
        let (server, path) = (server.clone(), path.clone());
        match tokio::task::spawn_blocking(move || save(&server, &path)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!("save failed: {e}"),
            Err(e) => eprintln!("save failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_roundtrip_keeps_player_ships_and_drops_asteroids() {
        let mut server = Server::new(7);
        let id = server.world.spawn_starter_ship("jakob".into());
        server.players.insert("jakob".into());
        server.players.insert("spectator".into());
        for _ in 0..50 {
            server.world.step();
        }
        assert!(server.world.ships.values().any(|s| s.owner.is_none()), "asteroids spawned");

        let text = serde_json::to_string(&server.to_save()).unwrap();
        let loaded = Server::from_save(serde_json::from_str(&text).unwrap());

        assert_eq!(loaded.world.seed, 7);
        assert_eq!(loaded.world.tick, server.world.tick);
        assert_eq!(loaded.world.next_ship_id, server.world.next_ship_id);
        assert_eq!(loaded.players, server.players);
        assert_eq!(loaded.world.ships.len(), 1);
        let (a, b) = (&server.world.ships[&id], &loaded.world.ships[&id]);
        assert_eq!(a.owner, b.owner);
        assert_eq!(a.pos, b.pos);
        assert_eq!(a.rot, b.rot);
        assert_eq!(a.vel, b.vel);
        assert_eq!(a.blocks, b.blocks);
        assert!((a.com[0] - b.com[0]).abs() < 1e-3 && (a.com[1] - b.com[1]).abs() < 1e-3);
        assert!((a.total_mass - b.total_mass).abs() / a.total_mass < 1e-4);
    }
}
