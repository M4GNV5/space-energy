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
- A block's energy capacity scales with its mass: `capacity = material J/kg × mass`. A heavier block stores more; a block that loses mass (thrust, uranium decay) can hold less.
- Energy level = fill ratio (`energy / energy capacity`). *(MVP choice.)*
- Moved mass takes its share of the block's energy along, so moving mass does not change the source block's fill ratio.
- When a block's energy exceeds its capacity, it **bursts**: the block is removed, and all of its mass (as packets) and all of its energy (as rays) are emitted in all directions at once. Chain reactions are possible.

### Block Types
Each material has a maximum mass, an energy capacity per kg, a conductance (energy flow to adjacent blocks), an emit rate (max energy/mass emitted per tick), and possibly extra effects.

| Material | Role |
|---|---|
| iron | high emit rate (good emitter / laser) |
| copper | high conductance (wiring) |
| lead | highest mass capacity (fuel tank, mass storage) |
| plastic | very low conductance (energy isolator) |
| tungsten | highest energy capacity per kg and per block (battery, armor that is hard to burst) |
| uranium | each tick converts mass into energy at a configurable rate. No control, no meltdown. Inert while part of an asteroid |
| silicon | switchable one-way conductor: energy flows in from the block behind it and out to the block ahead, nothing to the sides. A perfect isolator until pointed |
| gold | reflects incoming energy rays instead of absorbing them (mirror). *Post-MVP* |
| sun | only used by suns. Infinite, indestructible, continuously emits energy in all directions, absorbs incoming energy. Cannot be built. *MVP: a sun is a single disk (radius 100–300 m; its power scales with the radius), not a grid of blocks* |

Control signals: there is no separate "switch" command. An emit command that requests at least `SIGNAL_MIN_ENERGY` (1 J) from a silicon block also controls it. The request counts even if the block has no energy to emit.
- Silicon points at the emit direction and stays that way. Emitting in all directions switches it off again.
- New silicon blocks start switched off.
- Switched-off silicon still absorbs rays and can still burst.

### Obtaining Energy
- Solar: energy emitted by nearby suns is absorbed by the blocks it hits. Asteroids absorb only a small fraction of sunlight (`ASTEROID_SUN_ABSORPTION`), so they do not burst from the sun alone; lasers hit them at full strength.
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
- A temporary helper, until players can script flight control: an automatic brake. Every ship decelerates at a constant rate (`LINEAR_BRAKE`), the same at any speed, until it stops. The brake is off in any tick where the ship's own thrust increases its speed, so there is no maximum speed, and retro thrust adds to the brake. Spin loses a fraction per tick (`ANGULAR_DAMPING`).
- For now, no collisions between ships and no ship splitting (a ship stays one body even if it becomes disconnected).
- A ship with no blocks left is removed.

### World
- The world is infinite and is generated from a seed given at server start. Nothing is persisted.
- Suns are deterministic per chunk (`CHUNK_SIZE`) based on the seed. They are static, infinite and have no gravity (for now).
- Asteroids (unowned ships of 40–300 blocks, with materials in veins) spawn randomly around active players all the time, and despawn when no player is near.

### Players
- Login by username only (no password for now). Typing an existing name gives control of that player's ships.
- A new player gets a starting ship near a sun: an iron hull (86 blocks) around a plastic-insulated uranium reactor. Two silicon outlets feed a copper spine (tungsten battery and laser at the nose) and a copper bus (lead engines at the rear). Plastic keeps both circuits apart from the hull.
- There is no death or respawn. A player who has lost all ships becomes a spectator (camera only) and can create a new account.
- No goal for now. PvP comes later.

### Multiplayer
The main game server (written in Rust) hosts a websocket server allowing multiple browsers to connect.
The game server is authoritative. It is responsible for:
- tracking all ships, their position, rotation and blocks, as well as mass and energy levels
- automatically moving energy between adjacent blocks of a ship. Energy flows from higher fill ratio to lower. The better conductor of a pair sets the rate (lead next to copper exchanges energy at copper's rate), except that a pair with an isolator (plastic) uses the lower conductance
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
- Gold, signals between ships
- PvP goals, proper authentication, persistence
