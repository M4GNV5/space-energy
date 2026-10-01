// Client-side world state: latest snapshot from the server plus extrapolation
// and short-lived ray fade-out.

import type { PacketView, RayView, ShipView, StateMsg, SunView } from "./protocol";

export interface RayInstance {
  ray: RayView;
  bornAt: number; // performance.now() ms
}

export const MAX_EXTRAPOLATION_S = 0.1;
export const RAY_FADE_MS = 150;

export class GameState {
  tick = 0;
  ships: ShipView[] = [];
  suns: SunView[] = [];
  packets: PacketView[] = [];
  rays: RayInstance[] = [];
  /** performance.now() timestamp of the last `state` message. */
  receivedAt = 0;

  apply(msg: StateMsg, now: number): void {
    this.tick = msg.tick;
    this.ships = msg.ships;
    this.suns = msg.suns;
    this.packets = msg.packets;
    this.receivedAt = now;
    for (const r of msg.rays) this.rays.push({ ray: r, bornAt: now });
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
    return this.ships.find((s) => s.id === id);
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
