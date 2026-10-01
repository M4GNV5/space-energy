// Client-side world state: the server's deltas put back together, plus
// extrapolation and short-lived ray fade-out.

import { TICK_S, type BlockView, type PacketView, type RayView, type ShipId, type ShipView, type StateDelta, type StateMsg, type SunView } from "./protocol";

export interface RayInstance {
  ray: RayView;
  bornAt: number; // performance.now() ms
}

/** Long enough to coast through the ticks a slow connection skips. */
export const MAX_EXTRAPOLATION_S = 0.25;
export const RAY_FADE_MS = 150;

export class GameState {
  tick = 0;
  ships: ShipView[] = [];
  suns: SunView[] = [];
  packets: PacketView[] = [];
  rays: RayInstance[] = [];
  /** performance.now() timestamp of the last `state` message. */
  receivedAt = 0;

  private shipsById = new Map<ShipId, ShipView>();
  /** Per ship: "i,j" -> block. */
  private blocksByShip = new Map<ShipId, Map<string, BlockView>>();
  private packetsById = new Map<number, PacketView>();
  private tickRays: RayView[] = [];

  apply(msg: StateDelta, now: number): void {
    if (msg.reset) {
      this.shipsById.clear();
      this.blocksByShip.clear();
      this.packetsById.clear();
      this.suns = [];
    } else {
      // Everything keeps moving through the ticks since the last message.
      // Mirrors `predicted` in server/src/delta.rs.
      const dt = (msg.tick - this.tick) * TICK_S;
      for (const s of this.shipsById.values()) {
        s.x += s.vx * dt;
        s.y += s.vy * dt;
        s.rot += s.omega * dt;
      }
      for (const p of this.packetsById.values()) {
        p.x += p.vx * dt;
        p.y += p.vy * dt;
      }
    }
    this.tick = msg.tick;
    this.receivedAt = now;

    for (const d of msg.ships ?? []) {
      let ship = this.shipsById.get(d.id);
      if (d.new || !ship) {
        ship = { id: d.id, owner: d.owner ?? null, x: 0, y: 0, rot: 0, vx: 0, vy: 0, omega: 0, com: [0, 0], blocks: [] };
        this.shipsById.set(d.id, ship);
        this.blocksByShip.set(d.id, new Map());
      }
      if (d.pose) [ship.x, ship.y, ship.rot, ship.vx, ship.vy, ship.omega] = d.pose;
      if (d.com) ship.com = d.com;
      const blocks = this.blocksByShip.get(d.id)!;
      for (const [i, j, m, mass, energy, dir] of d.blocks ?? []) {
        const block: BlockView = { p: [i, j], m, mass, energy };
        if (dir) block.dir = dir;
        blocks.set(`${i},${j}`, block);
      }
      for (const [i, j] of d.del ?? []) blocks.delete(`${i},${j}`);
      if (d.blocks || d.del) ship.blocks = [...blocks.values()];
    }
    for (const id of msg.gone ?? []) {
      this.shipsById.delete(id);
      this.blocksByShip.delete(id);
    }
    this.ships = [...this.shipsById.values()];

    if (msg.suns) this.suns = msg.suns;

    for (const [id, x, y, vx, vy, m, mass] of msg.packets ?? []) this.packetsById.set(id, { id, x, y, vx, vy, m, mass });
    for (const id of msg.pgone ?? []) this.packetsById.delete(id);
    this.packets = [...this.packetsById.values()];

    this.tickRays = (msg.rays ?? []).map(([x1, y1, x2, y2, energy, emitted, beam]) => ({ x1, y1, x2, y2, energy, emitted, beam: beam === 1 }));
    for (const r of this.tickRays) this.rays.push({ ray: r, bornAt: now });
  }

  /** The current view as one message, for scripts. */
  snapshot(): StateMsg {
    return { tick: this.tick, ships: this.ships, suns: this.suns, packets: this.packets, rays: this.tickRays };
  }

  pruneRays(now: number): void {
    if (this.rays.length === 0) return;
    this.rays = this.rays.filter((r) => now - r.bornAt < RAY_FADE_MS);
  }

  ownedShipIds(player: string): number[] {
    return this.ships
      .filter((s) => s.owner === player)
      .map((s) => s.id)
      .sort((a, b) => a - b);
  }

  find(id: number): ShipView | undefined {
    return this.shipsById.get(id);
  }
}

export interface Pose {
  x: number;
  y: number;
  rot: number;
}

/** Extrapolate a ship's pose forward by `dtSeconds`, capped at MAX_EXTRAPOLATION_S. */
export function extrapolate(
  ship: { x: number; y: number; rot: number; vx: number; vy: number; omega: number },
  dtSeconds: number,
): Pose {
  const dt = Math.min(Math.max(dtSeconds, 0), MAX_EXTRAPOLATION_S);
  return { x: ship.x + ship.vx * dt, y: ship.y + ship.vy * dt, rot: ship.rot + ship.omega * dt };
}
