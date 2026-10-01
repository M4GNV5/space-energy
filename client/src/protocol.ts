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
  // Requesting >= 1 J also points a silicon block at `dir` ("all" = off).
  | { t: "emit"; ship: ShipId; block: Cell; dir: Dir; energy: number; mass: number }
  | { t: "collect"; ship: ShipId; block?: Cell; material?: Material };

export type ServerMsg =
  | { t: "welcome"; player: string; ships: ShipId[] }
  | ({ t: "state" } & StateMsg)
  | { t: "error"; msg: string };

export interface StateMsg {
  tick: number;
  ships: ShipView[];
  suns: SunView[];
  packets: PacketView[];
  /** Energy rays emitted this tick, for drawing only. */
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
