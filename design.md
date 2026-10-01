# Space Energy

A 2D multiplayer space game where you receive, store and transmit energy.

Long-term goal: players write code that controls their ships (cooling, flight control, weapons). Until then, the game offers a few temporary helpers (e.g. speed damping) so ships are flyable by hand.

### Units and Tick
- Metric units everywhere: m, kg, s, J.
- Simulation runs at a fixed 25 ticks/s.
- Block size does not matter visually (everything is zoomable). For physics, one block is 1 m × 1 m.
- All balancing values (capacities, rates, falloff, damping, ...) are constants in the Rust code so they can be tuned later.

### Emitting and Receiving
All objects in space are built from blocks. Each block has a material, a mass and an energy level.
At every tick, blocks may transmit a portion of their mass and/or energy. They also receive energy emitted from other blocks. Energy and mass can be transmitted in a specific direction or in all directions equally.
Emitting can be used to transmit signals to other ships (later), destroy blocks (given enough energy), provide thrust (given enough mass and energy) or cool a ship by dumping energy into space.

Energy emission:
- Emission is instant within a tick: a ray is cast from the emitting block's edge.
- Directed: the ray goes out of one of the block's 4 faces (relative to the ship's rotation). *(MVP choice; arbitrary angles come with scripting.)*
- All directions: the energy is split across `OMNI_RAYS` rays with a random angular offset each tick, so that on average it spreads evenly.
- The first block hit absorbs the energy, including blocks of the emitting ship itself. This is not optional, and it is what makes lasers possible. Emit from exposed faces.
- The delivered energy weakens with distance (inverse square): `delivered = energy / (1 + (d / FALLOFF_DISTANCE)²)`. Below `MIN_FALLOFF` it becomes zero, which gives a maximum range.
- Energy lost to falloff, or rays that hit nothing, is gone. This is the energy sink of the universe, so emitting energy into empty space is how ships cool down.

Mass emission:
- Emitted mass becomes **mass packets**: point objects with a material, mass and velocity (`EXHAUST_SPEED` relative to the ship). They drift until collected or until `PACKET_TTL` expires.
- Directed mass emission gives the ship an equal and opposite impulse at the block's position (thrust plus torque).
- Emitting mass costs energy from the emitting block: `EMIT_ENERGY_PER_KG`.
- Mass packets do not hit blocks (no collisions for now).

### Blocks
- Every block has a mass between 1 kg and its material's maximum. Hovering shows e.g. `mass 5/9000 kg`, `energy 120/5000 J`.
- A block whose mass reaches 0 is removed.
- A new block is created by moving mass of material X into an empty cell adjacent to the ship. The new block has material X.
- Mass can only be moved into an existing block of the same material.
- Energy level = fill ratio (`energy / energy capacity`). *(MVP choice.)*
- When a block's energy exceeds its capacity, it **bursts**: the block is removed, and all of its mass (as packets) and all of its energy (as rays) are emitted in all directions at once. Chain reactions are possible.

### Block Types
Each material has a maximum mass, a maximum energy capacity, a conductance (energy flow to adjacent blocks), an emit rate (max energy/mass emitted per tick), and possibly extra effects.

| Material | Role |
|---|---|
| iron | high emit rate (good emitter / laser) |
| copper | high conductance (wiring) |
| lead | highest mass capacity (fuel tank, mass storage) |
| plastic | very low conductance (energy isolator) |
| tungsten | highest energy capacity (battery, armor that is hard to burst) |
| uranium | each tick converts mass into energy at a configurable rate. No control, no meltdown |
| silicon | conducts energy in one direction only (diode, orientation set on creation). *Post-MVP* |
| gold | reflects incoming energy rays instead of absorbing them (mirror). *Post-MVP* |
| sun | only used by suns. Infinite, indestructible, continuously emits energy in all directions, absorbs incoming energy. Cannot be built. *MVP: a sun is a single disk (radius 10–30 m), not a grid of blocks* |

### Obtaining Energy
- Solar: energy emitted by nearby suns is absorbed by the blocks it hits.
- Nuclear: uranium blocks turn their own mass into energy.
- Also: energy from other ships' beams and from bursting blocks.

### Obtaining Mass
When a block bursts, it releases all of its mass as packets.
Any ship can collect nearby packets (within `COLLECT_RANGE`) into any of its blocks with a matching material, or into an adjacent empty cell (which creates a new block of that material).
Mining = shooting asteroids with energy until their blocks burst, then collecting the mass.

### Ships and Physics
- A ship is a rigid body made of blocks on a ship-local integer grid.
- The server computes center of mass and moment of inertia from the block masses, and recomputes them whenever mass changes.
- Thrust comes only from mass emission. Energy beams carry no momentum (for now).
- A temporary helper, until players can script flight control: every tick, all ships lose a fraction of their linear and angular velocity (`LINEAR_DAMPING`, `ANGULAR_DAMPING`), which makes stopping easy.
- For now, no collisions between ships and no ship splitting (a ship stays one body even if it becomes disconnected).
- A ship with no blocks left is removed.

### World
- The world is infinite and is generated from a seed given at server start. Nothing is persisted.
- Suns are deterministic per chunk (`CHUNK_SIZE`) based on the seed. They are static, infinite and have no gravity (for now).
- Asteroids (unowned ships made of random materials with mass) spawn randomly around active players all the time, and despawn when no player is near.

### Players
- Login by username only (no password for now). Typing an existing name gives control of that player's ships.
- A new player gets a starting ship near a sun.
- There is no death or respawn. A player who has lost all ships becomes a spectator (camera only) and can create a new account.
- No goal for now. PvP comes later.

### Multiplayer
The main game server (written in Rust) hosts a websocket server allowing multiple browsers to connect.
The game server is authoritative. It is responsible for:
- tracking all ships, their position, rotation and blocks, as well as mass and energy levels
- automatically moving energy between adjacent blocks of a ship. Energy flows from higher fill ratio to lower, at a rate based on both blocks' conductance (copper = fast, plastic = slow)
- the basic 2D physics described above

Connected users can send commands for the ships they own:
- move mass between blocks, or into an empty adjacent cell (adds a block; a block that reaches 0 mass is removed)
- transmit energy and/or mass from a block (in a direction or in all directions)
- collect mass packets from nearby space

### Frontend
The browser-based frontend should:
- show a zoomable 2D space map, with the camera following your ship
- show ship/block information on hover (material, `mass 5/9000 kg`, `energy 120/5000 J`)
- allow building ships: click a mass source block first, then a target block or empty cell
- allow binding keys to commands (e.g. bind "WASD" to emitting mass from specific blocks)

### Later
- Player-written code controlling ships (flight control, cooling, automation)
- Collisions, ship splitting
- Silicon, gold, signals between ships
- PvP goals, proper authentication, persistence
