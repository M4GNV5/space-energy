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
import { extrapolate } from "../src/state.ts";

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

// extrapolation moves the ship forward but caps at 100ms even for a larger dt
check("extrapolate caps at 100ms", () => {
  const ship = { x: 0, y: 0, rot: 0, vx: 10, vy: 0, omega: 1 };
  const at50ms = extrapolate(ship, 0.05);
  assert.ok(Math.abs(at50ms.x - 0.5) < 1e-9, `expected x=0.5 at 50ms, got ${at50ms.x}`);

  const at100ms = extrapolate(ship, 0.1);
  const at1s = extrapolate(ship, 1.0); // way beyond the cap
  assert.equal(at100ms.x, at1s.x, "extrapolation beyond the cap should not move further");
  assert.ok(Math.abs(at1s.x - 1.0) < 1e-9, `capped x should be 1.0 (10 m/s * 0.1s), got ${at1s.x}`);
});

console.log(`\n${passed} checks passed`);
