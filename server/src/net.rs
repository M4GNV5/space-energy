//! Websocket networking: connections, players, login, command validation,
//! and per-tick state snapshots.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::protocol::{
    BlockView, ClientMsg, PacketView, RayView, ServerMsg, ShipId, ShipView, StateMsg, SunView,
};
use crate::sim::consts::MAX_VIEW_RADIUS;
use crate::sim::ship::{dist, Ship};
use crate::sim::world::{Command, World};

struct Conn {
    name: Option<String>,
    view: (f32, f32, f32),
    tx: mpsc::UnboundedSender<ServerMsg>,
}

/// Everything behind one lock: the simulation plus connection/player bookkeeping.
pub struct Server {
    pub world: World,
    pub(crate) players: std::collections::HashSet<String>,
    /// Ships of offline players. They leave the world when their owner's last
    /// connection closes and come back, unchanged, at the next login.
    pub parked: Vec<Ship>,
    connections: HashMap<u64, Conn>,
    next_conn_id: u64,
}

impl Server {
    pub fn new(seed: u64) -> Self {
        Server { world: World::new(seed), players: std::collections::HashSet::new(), parked: Vec::new(), connections: HashMap::new(), next_conn_id: 1 }
    }

    /// Drops a connection. If it was the player's last one, their ships are parked.
    fn disconnect(&mut self, conn_id: u64) {
        let Some(name) = self.connections.remove(&conn_id).and_then(|c| c.name) else { return };
        if self.connections.values().any(|c| c.name.as_deref() == Some(name.as_str())) {
            return;
        }
        let ids: Vec<ShipId> =
            self.world.ships.iter().filter(|(_, sh)| sh.owner.as_deref() == Some(name.as_str())).map(|(id, _)| *id).collect();
        for id in ids {
            self.parked.extend(self.world.ships.remove(&id));
        }
    }

    /// Puts the parked ships of `name` back into the world.
    fn unpark(&mut self, name: &str) {
        let (theirs, rest) = std::mem::take(&mut self.parked).into_iter().partition(|sh| sh.owner.as_deref() == Some(name));
        self.parked = rest;
        for ship in theirs {
            self.world.ships.insert(ship.id, ship);
        }
    }
}

pub type SharedServer = Arc<Mutex<Server>>;

pub async fn ws_handler(ws: WebSocketUpgrade, State(server): State<SharedServer>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, server))
}

async fn handle_socket(socket: WebSocket, server: SharedServer) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMsg>();

    let conn_id = {
        let mut s = server.lock().unwrap();
        let id = s.next_conn_id;
        s.next_conn_id += 1;
        s.connections.insert(id, Conn { name: None, view: (0.0, 0.0, 1000.0), tx: tx.clone() });
        id
    };

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = serde_json::to_string(&msg).unwrap_or_default();
            if ws_tx.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => handle_client_msg(&server, conn_id, &text),
            Message::Close(_) => break,
            _ => {}
        }
    }

    server.lock().unwrap().disconnect(conn_id);
    writer.abort();
}

fn send_to(server: &SharedServer, conn_id: u64, msg: ServerMsg) {
    let s = server.lock().unwrap();
    if let Some(conn) = s.connections.get(&conn_id) {
        let _ = conn.tx.send(msg);
    }
}

fn owns_ship(s: &Server, ship: ShipId, player: &str) -> bool {
    s.world.ships.get(&ship).and_then(|sh| sh.owner.as_deref()).map(|o| o == player).unwrap_or(false)
}

fn handle_client_msg(server: &SharedServer, conn_id: u64, text: &str) {
    let msg: ClientMsg = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(_) => {
            send_to(server, conn_id, ServerMsg::Error { msg: "invalid json".into() });
            return;
        }
    };

    let mut s = server.lock().unwrap();
    let current_name = s.connections.get(&conn_id).and_then(|c| c.name.clone());

    match msg {
        ClientMsg::Login { name } => {
            if current_name.is_some() {
                drop(s);
                send_to(server, conn_id, ServerMsg::Error { msg: "already logged in".into() });
                return;
            }
            let first_time = !s.players.contains(&name);
            s.players.insert(name.clone());
            s.unpark(&name);
            let mut ships: Vec<ShipId> =
                s.world.ships.iter().filter(|(_, sh)| sh.owner.as_deref() == Some(name.as_str())).map(|(id, _)| *id).collect();
            if first_time {
                let id = s.world.spawn_starter_ship(name.clone());
                ships.push(id);
            }
            let center = ships.first().and_then(|id| s.world.ships.get(id)).map(|sh| (sh.pos[0], sh.pos[1])).unwrap_or((0.0, 0.0));
            if let Some(conn) = s.connections.get_mut(&conn_id) {
                conn.name = Some(name.clone());
                conn.view = (center.0, center.1, 1000.0);
            }
            drop(s);
            send_to(server, conn_id, ServerMsg::Welcome { player: name, ships });
        }
        ClientMsg::View { x, y, r } => {
            let Some(_name) = current_name else {
                drop(s);
                send_to(server, conn_id, ServerMsg::Error { msg: "must login first".into() });
                return;
            };
            if let Some(conn) = s.connections.get_mut(&conn_id) {
                conn.view = (x, y, r.clamp(0.0, MAX_VIEW_RADIUS));
            }
        }
        ClientMsg::MoveMass { ship, from, to, kg } => {
            let Some(name) = current_name else {
                drop(s);
                send_to(server, conn_id, ServerMsg::Error { msg: "must login first".into() });
                return;
            };
            if !owns_ship(&s, ship, &name) {
                drop(s);
                send_to(server, conn_id, ServerMsg::Error { msg: "not your ship".into() });
                return;
            }
            s.world.queue_command(name, Command::MoveMass { ship, from, to, kg });
        }
        ClientMsg::Emit { ship, block, dir, energy, mass } => {
            let Some(name) = current_name else {
                drop(s);
                send_to(server, conn_id, ServerMsg::Error { msg: "must login first".into() });
                return;
            };
            if !owns_ship(&s, ship, &name) {
                drop(s);
                send_to(server, conn_id, ServerMsg::Error { msg: "not your ship".into() });
                return;
            }
            s.world.queue_command(name, Command::Emit { ship, block, dir, energy, mass });
        }
        ClientMsg::Collect { ship, block, material } => {
            let Some(name) = current_name else {
                drop(s);
                send_to(server, conn_id, ServerMsg::Error { msg: "must login first".into() });
                return;
            };
            if !owns_ship(&s, ship, &name) {
                drop(s);
                send_to(server, conn_id, ServerMsg::Error { msg: "not your ship".into() });
                return;
            }
            s.world.queue_command(name, Command::Collect { ship, block, material });
        }
    }
}

fn ship_view(s: &Ship) -> ShipView {
    ShipView {
        id: s.id,
        owner: s.owner.clone(),
        x: s.pos[0],
        y: s.pos[1],
        rot: s.rot,
        vx: s.vel[0],
        vy: s.vel[1],
        omega: s.omega,
        com: s.com,
        blocks: s
            .blocks
            .iter()
            .map(|(&p, b)| BlockView { p, m: b.material, mass: b.mass, energy: b.energy, dir: b.dir })
            .collect(),
    }
}

/// Runs the fixed-rate game loop forever: steps the simulation, dispatches
/// queued command errors, and sends each logged-in client its filtered state.
pub async fn run_game_loop(server: SharedServer) {
    let mut interval = tokio::time::interval(Duration::from_millis(40));
    loop {
        interval.tick().await;
        let mut s = server.lock().unwrap();
        s.world.step();

        let errors = std::mem::take(&mut s.world.errors);
        for (player, msg) in errors {
            for conn in s.connections.values() {
                if conn.name.as_deref() == Some(player.as_str()) {
                    let _ = conn.tx.send(ServerMsg::Error { msg: msg.clone() });
                }
            }
        }

        type ConnSnapshot = (u64, Option<String>, (f32, f32, f32));
        let conns: Vec<ConnSnapshot> = s.connections.iter().map(|(id, c)| (*id, c.name.clone(), c.view)).collect();
        let tick = s.world.tick;
        let mut dead = Vec::new();
        for (id, name, (cx, cy, r)) in conns {
            if name.is_none() {
                continue;
            }
            let center = [cx, cy];
            let suns: Vec<SunView> = s.world.suns_in_view(center, r).into_iter().map(|sun| SunView { x: sun.x, y: sun.y, radius: sun.radius }).collect();
            let ships: Vec<ShipView> =
                s.world.ships.values().filter(|sh| dist(sh.pos, center) <= r + sh.bounding_radius()).map(ship_view).collect();
            let packets: Vec<PacketView> = s
                .world
                .packets
                .iter()
                .filter(|p| dist(p.pos, center) <= r)
                .map(|p| PacketView { x: p.pos[0], y: p.pos[1], vx: p.vel[0], vy: p.vel[1], m: p.material, mass: p.mass })
                .collect();
            let rays: Vec<RayView> = s
                .world
                .rays
                .iter()
                .filter(|ray| dist([ray.x1, ray.y1], center) <= r || dist([ray.x2, ray.y2], center) <= r)
                .cloned()
                .collect();
            let state = ServerMsg::State(StateMsg { tick, ships, suns, packets, rays });
            match s.connections.get(&id) {
                Some(conn) if conn.tx.send(state).is_ok() => {}
                _ => dead.push(id),
            }
        }
        for id in dead {
            s.disconnect(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connect(s: &mut Server, conn_id: u64, name: &str) {
        let (tx, _rx) = mpsc::unbounded_channel();
        s.connections.insert(conn_id, Conn { name: Some(name.into()), view: (0.0, 0.0, 1000.0), tx });
        s.unpark(name);
    }

    #[test]
    fn ships_are_parked_while_their_owner_is_offline() {
        let mut s = Server::new(7);
        let a = s.world.spawn_starter_ship("a".into());
        let b = s.world.spawn_starter_ship("b".into());
        connect(&mut s, 1, "a");
        connect(&mut s, 2, "a");
        connect(&mut s, 3, "b");

        s.disconnect(1);
        assert!(s.world.ships.contains_key(&a), "still online through the second connection");
        s.disconnect(2);
        assert!(!s.world.ships.contains_key(&a) && s.world.ships.contains_key(&b));
        assert_eq!(s.parked.len(), 1);

        connect(&mut s, 4, "a");
        assert!(s.world.ships.contains_key(&a));
        assert!(s.parked.is_empty());
    }
}
