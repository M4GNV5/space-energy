// Helpers to pick a ship/block under the cursor and to look up blocks by cell.

import { localToCell, worldToLocal } from "./camera";
import type { BlockView, Cell, ShipView } from "./protocol";
import type { Pose } from "./state";

/** Max distance from the ship's centre of mass to any block corner. Used as a cheap hit-test radius. */
export function shipBoundingRadius(ship: ShipView): number {
  let r = 0;
  for (const b of ship.blocks) {
    for (const dx of [-0.5, 0.5]) {
      for (const dy of [-0.5, 0.5]) {
        const cx = b.p[0] + dx - ship.com[0];
        const cy = b.p[1] + dy - ship.com[1];
        const d = Math.hypot(cx, cy);
        if (d > r) r = d;
      }
    }
  }
  return r;
}

export function findBlock(ship: ShipView, cell: Cell): BlockView | undefined {
  return ship.blocks.find((b) => b.p[0] === cell[0] && b.p[1] === cell[1]);
}

export interface PickResult {
  ship: ShipView;
  cell: Cell;
  block: BlockView | null;
}

/**
 * Find the nearest ship whose bounding circle contains the world point, and
 * the ship-local cell the point rounds to. Returns null if no ship is close.
 */
export function pickShipAt(
  ships: ShipView[],
  poses: Map<number, Pose>,
  wx: number,
  wy: number,
): PickResult | null {
  let best: PickResult | null = null;
  let bestDist = Infinity;
  for (const ship of ships) {
    const pose = poses.get(ship.id) ?? ship;
    const d = Math.hypot(wx - pose.x, wy - pose.y);
    const r = shipBoundingRadius(ship) + 0.75;
    if (d <= r && d < bestDist) {
      const local = worldToLocal(pose.x, pose.y, pose.rot, ship.com, wx, wy);
      const cell = localToCell(local);
      best = { ship, cell, block: findBlock(ship, cell) ?? null };
      bestDist = d;
    }
  }
  return best;
}

/** Ship-local cell for a world point, for one specific ship (ignores bounding radius). */
export function localCellForShip(ship: ShipView, pose: Pose, wx: number, wy: number): Cell {
  return localToCell(worldToLocal(pose.x, pose.y, pose.rot, ship.com, wx, wy));
}

/** A source block picked in build mode. */
export interface BuildSelection {
  ship: number;
  cell: Cell;
}
