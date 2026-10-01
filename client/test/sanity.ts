// Pure-function sanity checks for camera transforms and extrapolation.
// No server, no DOM. Run with: npx tsx client/test/sanity.ts
// (tsx is used ad hoc, not added as a project dependency.)

import assert from "node:assert/strict";
import {
  clampZoom,
  localToCell,
  localToWorld,
  rotate,
  screenToWorld,
  worldToLocal,
  worldToScreen,
  type Camera,
} from "../src/camera.ts";
import { TICK_S } from "../src/protocol.ts";
import { GameState, MAX_EXTRAPOLATION_S, extrapolate } from "../src/state.ts";

let passed = 0;
function check(name: string, fn: () => void): void {
  fn();
  passed++;
  console.log(`ok - ${name}`);
}

// world -> screen -> world round trip
check("worldToScreen/screenToWorld round trip", () => {
  const cam: Camera = { x: 12.5, y: -7, zoom: 15 };
  const cx = 400;
  const cy = 300;
  for (const [wx, wy] of [
    [0, 0],
    [12.5, -7],
    [100, 50],
    [-30, 200],
  ]) {
    const [sx, sy] = worldToScreen(cam, cx, cy, wx, wy);
    const [wx2, wy2] = screenToWorld(cam, cx, cy, sx, sy);
    assert.ok(Math.abs(wx2 - wx) < 1e-9, `x round trip: ${wx2} != ${wx}`);
    assert.ok(Math.abs(wy2 - wy) < 1e-9, `y round trip: ${wy2} != ${wy}`);
  }
});

// canvas y flips relative to world y (world up == screen up == smaller sy)
check("worldToScreen flips y", () => {
  const cam: Camera = { x: 0, y: 0, zoom: 10 };
  const [, syUp] = worldToScreen(cam, 0, 0, 0, 10); // world "up" (+y)
  const [, syDown] = worldToScreen(cam, 0, 0, 0, -10); // world "down" (-y)
  assert.ok(syUp < syDown, "positive world y should map to a smaller screen y");
});

// rotate(): 90 degree CCW turns +x into +y (world convention)
check("rotate 90deg CCW", () => {
  const [x, y] = rotate(1, 0, Math.PI / 2);
  assert.ok(Math.abs(x) < 1e-9, `x should be ~0, got ${x}`);
  assert.ok(Math.abs(y - 1) < 1e-9, `y should be ~1, got ${y}`);
});

// localToWorld / worldToLocal round trip through a rotated, offset ship
check("localToWorld/worldToLocal round trip", () => {
  const shipX = 5;
  const shipY = -3;
  const rot = 0.7;
  const com: [number, number] = [0.2, -0.1];
  for (const p of [
    [0, 0],
    [1, 1],
    [-1, 2],
    [3, -2],
  ] as [number, number][]) {
    const [wx, wy] = localToWorld(shipX, shipY, rot, com, p);
    const [lx, ly] = worldToLocal(shipX, shipY, rot, com, wx, wy);
    assert.ok(Math.abs(lx - p[0]) < 1e-9, `local x round trip: ${lx} != ${p[0]}`);
    assert.ok(Math.abs(ly - p[1]) < 1e-9, `local y round trip: ${ly} != ${p[1]}`);
  }
});

// localToCell rounds to the nearest integer cell
check("localToCell rounds to nearest cell", () => {
  // `+0` normalizes -0 (e.g. Math.round(-0.49) === -0) for deepEqual.
  const norm = (c: [number, number]): [number, number] => [c[0] + 0, c[1] + 0];
  assert.deepEqual(norm(localToCell([0.49, -0.49])), [0, 0]);
  assert.deepEqual(norm(localToCell([0.51, -0.51])), [1, -1]);
  assert.deepEqual(norm(localToCell([2.4, 2.6])), [2, 3]);
});

// A world point exactly on a rotated ship's block centre maps back to that
// block's cell after rotate+round.
check("hover cell survives ship rotation", () => {
  const shipX = 10;
  const shipY = 10;
  const rot = 1.234;
  const com: [number, number] = [0, 0];
  const cell: [number, number] = [1, -1];
  const [wx, wy] = localToWorld(shipX, shipY, rot, com, cell);
  const local = worldToLocal(shipX, shipY, rot, com, wx, wy);
  assert.deepEqual(localToCell(local), cell);
});

// zoom clamps to the documented wide range
check("clampZoom clamps to [0.02, 200]", () => {
  assert.equal(clampZoom(0.0001), 0.02);
  assert.equal(clampZoom(999999), 200);
  assert.equal(clampZoom(5), 5);
});

// extrapolation moves the ship forward but caps at MAX_EXTRAPOLATION_S even for a larger dt
check("extrapolate caps at MAX_EXTRAPOLATION_S", () => {
  const ship = { x: 0, y: 0, rot: 0, vx: 10, vy: 0, omega: 1 };
  const at50ms = extrapolate(ship, 0.05);
  assert.ok(Math.abs(at50ms.x - 0.5) < 1e-9, `expected x=0.5 at 50ms, got ${at50ms.x}`);

  const atCap = extrapolate(ship, MAX_EXTRAPOLATION_S);
  const at1s = extrapolate(ship, 1.0); // way beyond the cap
  assert.equal(atCap.x, at1s.x, "extrapolation beyond the cap should not move further");
  assert.ok(Math.abs(at1s.x - 10 * MAX_EXTRAPOLATION_S) < 1e-9, `capped x should be 10 m/s * the cap, got ${at1s.x}`);
});

check("state deltas: ships and packets coast between updates, blocks are patched", () => {
  const game = new GameState();
  game.apply(
    {
      tick: 10,
      reset: true,
      ships: [
        { id: 1, new: true, owner: "p", pose: [0, 0, 0, 10, 0, 1], com: [0.5, 0], blocks: [[0, 0, "iron", 100, 5], [1, 0, "silicon", 50, 0, "n"]] },
        { id: 2, new: true, pose: [50, 50, 0, 0, 0, 0], com: [0, 0], blocks: [[0, 0, "rock", 80, 0]] },
      ],
      suns: [{ x: 0, y: 500, radius: 100 }],
      packets: [[7, 0, 0, 0, 25, "lead", 2]],
    },
    0,
  );
  assert.deepEqual(game.find(1)!.blocks[1], { p: [1, 0], m: "silicon", mass: 50, energy: 0, dir: "n" });
  assert.equal(game.find(2)!.owner, null);

  // Three ticks later (two were skipped), with nothing but a block update and a packet.
  game.apply({ tick: 13, ships: [{ id: 1, blocks: [[0, 0, "iron", 100, 900]], del: [[1, 0]] }], rays: [[0, 0, 5, 5, 10, 20, 1]] }, 0);
  const ship = game.find(1)!;
  assert.ok(Math.abs(ship.x - 30 * TICK_S) < 1e-9 && Math.abs(ship.rot - 3 * TICK_S) < 1e-9, "ship coasts by its velocity");
  assert.deepEqual(ship.blocks, [{ p: [0, 0], m: "iron", mass: 100, energy: 900 }]);
  assert.ok(Math.abs(game.packets[0]!.y - 75 * TICK_S) < 1e-9, "packet coasts by its velocity");
  assert.equal(game.suns.length, 1, "suns stay until they are sent again");
  assert.deepEqual(game.snapshot().rays, [{ x1: 0, y1: 0, x2: 5, y2: 5, energy: 10, emitted: 20, beam: true }]);

  // A new pose replaces the dead reckoning; things that left the view are dropped.
  game.apply({ tick: 14, ships: [{ id: 1, pose: [2, 3, 0.5, 0, 0, 0] }], gone: [2], pgone: [7] }, 0);
  assert.deepEqual([ship.x, ship.y, ship.rot, ship.vx], [2, 3, 0.5, 0]);
  assert.deepEqual([game.ships.map((s) => s.id), game.packets, game.snapshot().rays], [[1], [], []]);

  // A reconnect starts from scratch.
  game.apply({ tick: 1, reset: true, suns: [] }, 0);
  assert.deepEqual([game.ships, game.suns], [[], []]);
});

console.log(`\n${passed} checks passed`);
