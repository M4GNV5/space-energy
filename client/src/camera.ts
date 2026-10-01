// Camera + world<->screen<->ship-local transforms. Pure functions, no DOM —
// kept testable from client/test/*.

export interface Camera {
  x: number; // world metres, camera centre
  y: number;
  zoom: number; // px per metre
}

export const ZOOM_MIN = 0.02;
export const ZOOM_MAX = 200;

export function clampZoom(z: number): number {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z));
}

/** World -> screen pixel. `cx,cy` = screen centre (canvas half-size). Canvas y points down, world y points up. */
export function worldToScreen(cam: Camera, cx: number, cy: number, wx: number, wy: number): [number, number] {
  return [cx + (wx - cam.x) * cam.zoom, cy - (wy - cam.y) * cam.zoom];
}

export function screenToWorld(cam: Camera, cx: number, cy: number, sx: number, sy: number): [number, number] {
  return [cam.x + (sx - cx) / cam.zoom, cam.y - (sy - cy) / cam.zoom];
}

/** Rotate a vector by `angle` radians CCW (world convention: y up). */
export function rotate(x: number, y: number, angle: number): [number, number] {
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  return [x * c - y * s, x * s + y * c];
}

/** World position of ship-local point `p`, given the ship's pose. */
export function localToWorld(
  shipX: number,
  shipY: number,
  rot: number,
  com: [number, number],
  p: [number, number],
): [number, number] {
  const [rx, ry] = rotate(p[0] - com[0], p[1] - com[1], rot);
  return [shipX + rx, shipY + ry];
}

/** Inverse of `localToWorld`: ship-local point (not yet rounded to a cell) for a world position. */
export function worldToLocal(
  shipX: number,
  shipY: number,
  rot: number,
  com: [number, number],
  wx: number,
  wy: number,
): [number, number] {
  const [rx, ry] = rotate(wx - shipX, wy - shipY, -rot);
  return [rx + com[0], ry + com[1]];
}

/** Round a local point to the nearest integer cell. */
export function localToCell(p: [number, number]): [number, number] {
  return [Math.round(p[0]), Math.round(p[1])];
}
