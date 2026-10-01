// Wire protocol: JSON over a single websocket at `/ws`.
// Mirrors `server/src/protocol.rs` — keep both in sync.
//
// Conventions:
// - World space is metric (m), y points UP, angles in radians counter-clockwise.
//   (Canvas y points down: flip when rendering.)
// - A ship's blocks live on a ship-local integer grid. Block `p = [i, j]` is a
//   1 m × 1 m square centred at local point (i, j).
// - `x, y` of a ship is the world position of its centre of mass, `com` is the
//   centre of mass in local grid coordinates. World position of a block:
//   `[x, y] + rotate(rot) * (p - com)`.
// - Face directions are ship-local: n = +y, e = +x, s = -y, w = -x.
//
// `state` messages are deltas against what this connection was sent before
// (`StateDelta`); `GameState` puts them back together into a `StateMsg`. Each
// one is answered with `ack`, so the server can skip ticks for a slow link.

/** Server tick length in seconds. Keep in sync with `TICK_RATE` in server/src/sim/consts.rs. */
export const TICK_S = 1 / 25;

export type Material = "iron" | "copper" | "lead" | "plastic" | "tungsten" | "uranium" | "silicon" | "rock";
export type Dir = "n" | "e" | "s" | "w" | "all";
export type ShipId = number;
export type Cell = [number, number];

// Keep in sync with server/src/sim/consts.rs (used for hover "5/9000").
export const MATERIALS: Record<Material, { maxMass: number; energyPerKg: number; color: string }> = {
  iron: { maxMass: 7900, energyPerKg: 6500, color: "#8a8f98" },
  copper: { maxMass: 8900, energyPerKg: 5000, color: "#c8773a" },
  lead: { maxMass: 11300, energyPerKg: 250, color: "#5a5f7a" },
  plastic: { maxMass: 1200, energyPerKg: 5000, color: "#e8e2c8" },
  tungsten: { maxMass: 19300, energyPerKg: 13000, color: "#3e8a7e" },
  uranium: { maxMass: 19000, energyPerKg: 4000, color: "#6fcf3f" },
  silicon: { maxMass: 2330, energyPerKg: 5000, color: "#3b5bb5" },
  rock: { maxMass: 2700, energyPerKg: 300, color: "#4d443c" },
};

/** Energy (J) a block can hold before it bursts. Scales with its mass. */
export function energyCapacity(b: { m: Material; mass: number }): number {
  return MATERIALS[b.m].energyPerKg * b.mass;
}

export type ClientMsg =
  | { t: "login"; name: string }
  | { t: "view"; x: number; y: number; r: number }
  | { t: "move_mass"; ship: ShipId; from: Cell; to: Cell; kg: number }
  // Repeats every tick until an emit for the same block and dir replaces or
  // stops it (energy 0, mass 0), or until the server's hold time (~0.5 s) runs
  // out: resend it while it should last.
  // Requesting >= 1 J also points a silicon block at `dir` ("all" = off).
  | { t: "emit"; ship: ShipId; block: Cell; dir: Dir; energy: number; mass: number }
  | { t: "collect"; ship: ShipId; block?: Cell; material?: Material }
  | { t: "ack"; tick: number };

export type ServerMsg =
  | { t: "welcome"; player: string; ships: ShipId[] }
  | ({ t: "state" } & StateDelta)
  | { t: "error"; msg: string };

/** What changed in view since the last `state`. Empty lists are left out. */
export interface StateDelta {
  tick: number;
  /** First message of a connection: forget everything known so far. */
  reset?: boolean;
  /** New and changed ships. */
  ships?: ShipDelta[];
  /** Ships that left the view or no longer exist. */
  gone?: ShipId[];
  /** All suns in view, only when that set changed. */
  suns?: SunView[];
  /** New packets and packets whose mass changed: `[id, x, y, vx, vy, material, mass]`. */
  packets?: Array<[number, number, number, number, number, Material, number]>;
  pgone?: number[];
  /** `[x1, y1, x2, y2, energy, emitted, beam]`: all player beams, a sample of the other rays. */
  rays?: Array<[number, number, number, number, number, number, 0 | 1]>;
}

export interface ShipDelta {
  id: ShipId;
  /** A ship not known yet: `owner` (absent for asteroids), `pose`, `com` and all blocks follow. */
  new?: boolean;
  owner?: string;
  /** `[x, y, rot, vx, vy, omega]`. Absent while the ship just keeps moving by `vx, vy, omega`. */
  pose?: [number, number, number, number, number, number];
  com?: [number, number];
  /** New and changed blocks: `[i, j, material, mass, energy, dir?]`. */
  blocks?: Array<[number, number, Material, number, number, Dir?]>;
  /** Cells whose block is gone. */
  del?: Cell[];
}

/** The whole view at one tick, as kept by `GameState` and handed to scripts. */
export interface StateMsg {
  tick: number;
  ships: ShipView[];
  suns: SunView[];
  packets: PacketView[];
  /** Energy rays of this tick, for drawing only: player beams and some of the rest. */
  rays: RayView[];
}

export interface ShipView {
  id: ShipId;
  /** null for asteroids */
  owner: string | null;
  x: number;
  y: number;
  rot: number;
  vx: number;
  vy: number;
  omega: number;
  com: [number, number];
  blocks: BlockView[];
}

export interface BlockView {
  p: Cell;
  m: Material;
  /** kg */
  mass: number;
  /** J */
  energy: number;
  /** Silicon only: the face energy flows out of. Absent = not conducting. */
  dir?: Dir;
}

export interface SunView {
  x: number;
  y: number;
  radius: number;
}

export interface PacketView {
  id: number;
  x: number;
  y: number;
  /** Velocity (m/s), used to draw fast packets as streaks. */
  vx: number;
  vy: number;
  m: Material;
  mass: number;
}

export interface RayView {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  /** Energy delivered at the end point (J), for line intensity. */
  energy: number;
  /** Energy at the origin (J), before falloff. */
  emitted: number;
  /** True for rays from a player's emit command; drawn even when they hit nothing. */
  beam: boolean;
}
