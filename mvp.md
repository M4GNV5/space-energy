# Space Energy – MVP Plan

Goal: the smallest playable version of [design.md](design.md).

**Done when:** two browsers connect with different usernames. Each gets a starting ship, flies with WASD, mines an asteroid (shoot → burst → collect), builds a new block, and shoots the other ship's blocks until they burst. A player with no ships left can still look around as a spectator.

## Scope

In:
- Materials: iron, copper, lead, plastic, tungsten, uranium, sun
- Energy conduction, uranium decay, bursting
- Energy emission (directed along 4 faces, and in all directions) with inverse-square falloff, instant raycast
- Mass emission → thrust plus mass packets; collecting packets
- Moving mass (build/remove blocks)
- Rigid-body movement with damping (custom code, no physics crate)
- Seeded suns, random asteroids, starting ship
- Username login, spectator when no ships are left
- Browser: pan/zoom map, hover info, build clicks, key binds

Out (later): chat, death/respawn, collisions, ship splitting, silicon, gold, signals, scripting, persistence, auth, goals/PvP scoring, client prediction, binary protocol.

## Tech

- `server/`: Rust, `tokio`, `axum` (websocket + serves the built client), `serde`/`serde_json`, `rand` + `rand_chacha` (seeded).
- `client/`: TypeScript + Vite, Canvas 2D, no framework.
- JSON messages over a single websocket.
- All tuning constants live in `server/src/sim/consts.rs`.

## Protocol (JSON)

Block positions are ship-local integer grid coordinates `[x, y]`.

Client → server:
```json
{"t":"login","name":"jakob"}
{"t":"view","x":0,"y":0,"r":500}
{"t":"move_mass","ship":1,"from":[0,0],"to":[1,0],"kg":10}
{"t":"emit","ship":1,"block":[0,1],"dir":"n|e|s|w|all","energy":100,"mass":0}
{"t":"collect","ship":1,"block":[0,-1],"material":"lead"}
{"t":"ack","tick":123}
```
- `emit` repeats every tick until a new `emit` for the same block and direction replaces it, until it is stopped (`energy` 0 and `mass` 0), or until 12 ticks pass (`EMIT_HOLD_TICKS`). The client resends it every 200 ms while the key is held and stops it on release. So thrust stays even when commands arrive late or in bursts.
- `ack` answers every `state`. The server lets at most 8 states go unanswered; after that it skips ticks for this client instead of queueing them, so a slow connection gets fewer but current states.
- `collect` needs `material` only when `block` is an empty cell.

Server → client:
```json
{"t":"welcome","player":"jakob","ships":[1]}
{"t":"state","tick":123,"ships":[{"id":1,"pose":[x,y,rot,vx,vy,omega],"blocks":[[0,1,"iron",300,5000]]}],"packets":[...],"rays":[...]}
{"t":"error","msg":"..."}
```
- `state` is sent every tick and covers the client's `view` radius, but only holds what changed since the last one (see `StateDelta` in `protocol.rs`). The first one has `reset: true` and is complete.
  - A ship's `pose` is left out while the ship just keeps moving by its last velocity. Blocks are sent when new, when their mass changes, or when their energy is off by 1 % of the capacity.
  - Packets fly straight: they are sent once, the client moves them.
- `rays` are only for drawing: all player beams, and one in four of the other rays that hit something.

## Starting Ship (tune in M7)

```
 y=1   L  I  L      L = lead (with fuel mass)
 y=0   C  U  C      I = iron (laser, fires north)
 y=-1  L  T  L      C = copper, U = uranium, T = tungsten
```
Default binds:
- W / S: bottom / top lead blocks emit mass south / north
- A / D: diagonal lead pairs emit sideways (rotation)
- Space: iron emits energy north (weapon + cooling)

## Milestones

### M1 – Skeleton
- [x] Cargo project `server/`, Vite project `client/`
- [x] axum serves `client/dist` and a `/ws` echo endpoint
- [x] Client connects and shows the echo

### M2 – Simulation core (headless, unit tested)
- [x] `consts.rs`: material table and all tuning constants
- [x] Ship: block grid, mass/energy per block, center of mass, inertia
- [x] Conduction between adjacent blocks (fill ratio, pairwise conductance)
- [x] Uranium: mass → energy per tick
- [x] Burst: remove the block, emit all mass and energy in all directions
- [x] Raycast: bounding-circle broadphase, then grid traversal (DDA) in ship-local space
- [x] Energy emission: directed and all directions, falloff, first hit absorbs
- [x] Mass emission: packets, impulse + torque, energy cost
- [x] Packets: drift, TTL, collect
- [x] `move_mass`: create/remove blocks, same-material rule, 1 kg minimum
- [x] Integration (position, rotation) with damping
- [x] Remove ships with no blocks

### M3 – World
- [x] Seed → per-chunk suns (deterministic)
- [x] Asteroid spawning around players, despawn when far away
- [x] Starting ship placed near a sun
- [x] 25 Hz tick loop

### M4 – Networking
- [x] Login (create or resume a player by name)
- [x] Command handling with an ownership check (commands are queued and applied at the next tick)
- [x] Per-client `state` snapshot filtered by the view radius
- [x] Spectator: a player with no ships can still send `view`

### M5 – Client rendering
- [x] Pan/zoom camera; camera follows your own ship (toggle)
- [x] Draw blocks by material color, with energy fill as brightness/overlay
- [x] Draw suns, packets and rays
- [x] Hover tooltip: material, `mass 5/9000 kg`, `energy 120/5000 J`

### M6 – Client controls
- [x] Build: click a source block, then a target block/empty cell, with an amount input
- [x] Collect button/key
- [x] Key-bind editor (key → emit command), stored in localStorage, with defaults for the starting ship

### M7 – Playtest and balance
- [x] Two-browser test against the "done when" criteria
- [x] Tune constants: the starting ship must be flyable and coolable by hand, and asteroids must be mineable in reasonable time
