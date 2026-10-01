// Canvas 2D rendering. Pure drawing code, no game logic beyond extrapolation.

import { type Camera, localToWorld, worldToScreen } from "./camera";
import { MATERIALS, energyCapacity, type BlockView, type Cell, type ShipId, type ShipView } from "./protocol";
import { extrapolate, type GameState, type Pose } from "./state";
import { type BuildSelection, type PickResult } from "./ships";

export interface RenderOptions {
  ownerName: string;
  controlledShip: ShipId | null;
  hover: PickResult | null;
  source: BuildSelection | null;
  /** Ship-local cell currently hovered on the source ship's grid, while a source is selected. */
  buildTargetCell: Cell | null;
}

const BLOCK_COLLAPSE_PX = 2;
/** Server tick length in seconds. */
const TICK_S = 0.04;

export function computePoses(game: GameState, now: number): Map<ShipId, Pose> {
  const dt = (now - game.receivedAt) / 1000;
  const poses = new Map<ShipId, Pose>();
  for (const ship of game.ships) poses.set(ship.id, extrapolate(ship, dt));
  return poses;
}

export function render(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  cam: Camera,
  game: GameState,
  poses: Map<ShipId, Pose>,
  now: number,
  opts: RenderOptions,
): void {
  ctx.fillStyle = "#05060a";
  ctx.fillRect(0, 0, width, height);

  const cx = width / 2;
  const cy = height / 2;

  drawGrid(ctx, cam, width, height);

  for (const sun of game.suns) drawSun(ctx, cam, cx, cy, sun);

  for (const packet of game.packets) {
    const [sx, sy] = worldToScreen(cam, cx, cy, packet.x, packet.y);
    const r = Math.max(1.5, Math.min(4, cam.zoom * 0.15));
    // Streak back over the distance travelled in one tick, so the packets of a
    // continuous emission join up into a line.
    const [tx, ty] = worldToScreen(cam, cx, cy, packet.x - packet.vx * TICK_S, packet.y - packet.vy * TICK_S);
    if (Math.hypot(tx - sx, ty - sy) > 2 * r) {
      ctx.strokeStyle = MATERIALS[packet.m].color;
      ctx.lineWidth = r;
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(tx, ty);
      ctx.lineTo(sx, sy);
      ctx.stroke();
      ctx.lineCap = "butt";
      continue;
    }
    ctx.fillStyle = MATERIALS[packet.m].color;
    ctx.beginPath();
    ctx.arc(sx, sy, r, 0, Math.PI * 2);
    ctx.fill();
  }

  for (const ship of game.ships) {
    const pose = poses.get(ship.id);
    if (!pose) continue;
    const isOwn = ship.owner === opts.ownerName;
    const isControlled = ship.id === opts.controlledShip;
    drawShip(ctx, cam, cx, cy, ship, pose, isOwn, isControlled, now, opts.hover, opts.source);
  }

  if (opts.source && opts.buildTargetCell) {
    drawGhost(ctx, cam, cx, cy, game, poses, opts.source, opts.buildTargetCell);
  }

  for (const inst of game.rays) {
    const age = now - inst.bornAt;
    const lifeFrac = 1 - age / 150;
    if (lifeFrac <= 0) continue;
    // Player beams are drawn by emitted energy, so they show even on a miss.
    const energy = inst.ray.beam ? inst.ray.emitted : inst.ray.energy;
    const intensity = Math.max(0, Math.log10(energy + 1));
    const alpha = Math.min(1, intensity / 6) * lifeFrac;
    if (alpha <= 0) continue;
    const width_ = Math.max(0.5, Math.min(4, intensity * 0.6));
    const [x1, y1] = worldToScreen(cam, cx, cy, inst.ray.x1, inst.ray.y1);
    const [x2, y2] = worldToScreen(cam, cx, cy, inst.ray.x2, inst.ray.y2);
    ctx.strokeStyle = `rgba(180,240,255,${alpha})`;
    ctx.lineWidth = width_;
    ctx.beginPath();
    ctx.moveTo(x1, y1);
    ctx.lineTo(x2, y2);
    ctx.stroke();
  }
}

function niceGridSpacing(zoom: number): number {
  const targetPx = 80;
  const rawWorld = targetPx / zoom;
  const exp = Math.floor(Math.log10(rawWorld));
  const base = Math.pow(10, exp);
  const options = [1, 2, 5, 10].map((m) => m * base);
  let best = options[0]!;
  let bestDiff = Infinity;
  for (const o of options) {
    const diff = Math.abs(o * zoom - targetPx);
    if (diff < bestDiff) {
      bestDiff = diff;
      best = o;
    }
  }
  return best;
}

function drawGrid(ctx: CanvasRenderingContext2D, cam: Camera, width: number, height: number): void {
  const spacing = niceGridSpacing(cam.zoom);
  const cx = width / 2;
  const cy = height / 2;
  const left = cam.x - cx / cam.zoom;
  const right = cam.x + cx / cam.zoom;
  const top = cam.y + cy / cam.zoom;
  const bottom = cam.y - cy / cam.zoom;

  ctx.strokeStyle = "rgba(255,255,255,0.06)";
  ctx.lineWidth = 1;

  const startX = Math.floor(left / spacing) * spacing;
  for (let wx = startX; wx <= right; wx += spacing) {
    const [sx] = worldToScreen(cam, cx, cy, wx, 0);
    ctx.beginPath();
    ctx.moveTo(sx, 0);
    ctx.lineTo(sx, height);
    ctx.stroke();
  }
  const startY = Math.floor(bottom / spacing) * spacing;
  for (let wy = startY; wy <= top; wy += spacing) {
    const [, sy] = worldToScreen(cam, cx, cy, 0, wy);
    ctx.beginPath();
    ctx.moveTo(0, sy);
    ctx.lineTo(width, sy);
    ctx.stroke();
  }
}

function drawSun(
  ctx: CanvasRenderingContext2D,
  cam: Camera,
  cx: number,
  cy: number,
  sun: { x: number; y: number; radius: number },
): void {
  const [sx, sy] = worldToScreen(cam, cx, cy, sun.x, sun.y);
  const r = Math.max(2, sun.radius * cam.zoom);
  const glowR = r * 3;
  const grad = ctx.createRadialGradient(sx, sy, 0, sx, sy, glowR);
  grad.addColorStop(0, "rgba(255,250,200,0.9)");
  grad.addColorStop(0.3, "rgba(255,220,120,0.35)");
  grad.addColorStop(1, "rgba(255,220,120,0)");
  ctx.fillStyle = grad;
  ctx.beginPath();
  ctx.arc(sx, sy, glowR, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#fff8dd";
  ctx.beginPath();
  ctx.arc(sx, sy, r, 0, Math.PI * 2);
  ctx.fill();
}

function drawShip(
  ctx: CanvasRenderingContext2D,
  cam: Camera,
  cx: number,
  cy: number,
  ship: ShipView,
  pose: Pose,
  isOwn: boolean,
  isControlled: boolean,
  now: number,
  hover: PickResult | null,
  source: BuildSelection | null,
): void {
  const blockPx = cam.zoom; // blocks are 1m x 1m
  const [sx, sy] = worldToScreen(cam, cx, cy, pose.x, pose.y);

  if (blockPx < BLOCK_COLLAPSE_PX) {
    const dotColor = isOwn ? "#4ddfff" : ship.owner ? "#ff9a4d" : "#888";
    ctx.fillStyle = dotColor;
    ctx.beginPath();
    ctx.arc(sx, sy, 3, 0, Math.PI * 2);
    ctx.fill();
  } else {
    for (const b of ship.blocks) {
      const [wx, wy] = localToWorld(pose.x, pose.y, pose.rot, ship.com, b.p);
      const [bsx, bsy] = worldToScreen(cam, cx, cy, wx, wy);
      ctx.save();
      ctx.translate(bsx, bsy);
      ctx.rotate(-pose.rot);
      const half = blockPx / 2;

      ctx.fillStyle = MATERIALS[b.m].color;
      ctx.fillRect(-half, -half, blockPx, blockPx);

      const cap = energyCapacity(b);
      const fill = cap > 0 ? Math.min(1, b.energy / cap) : 0;
      let overlayAlpha = 0.15 + 0.55 * fill;
      if (fill > 0.8) {
        const pulse = 0.5 + 0.5 * Math.sin(now / 150);
        overlayAlpha = 0.5 + 0.4 * pulse;
      }
      const g = Math.round(140 * (1 - fill));
      ctx.fillStyle = `rgba(255,${g},0,${overlayAlpha})`;
      ctx.fillRect(-half, -half, blockPx, blockPx);

      if (blockPx >= 6) drawControlState(ctx, b, half);

      const isSelectedSource = !!source && source.ship === ship.id && source.cell[0] === b.p[0] && source.cell[1] === b.p[1];
      const isHovered = !!hover && hover.ship.id === ship.id && hover.cell[0] === b.p[0] && hover.cell[1] === b.p[1];

      ctx.lineWidth = isOwn ? 1.5 : 0.75;
      ctx.strokeStyle = isSelectedSource ? "#8dff5a" : isHovered ? "#ffffff" : isOwn ? "#7fe8ff" : "#222";
      ctx.strokeRect(-half, -half, blockPx, blockPx);
      ctx.restore();
    }
  }

  if (isOwn) {
    ctx.fillStyle = isControlled ? "#8dff5a" : "#4ddfff";
    ctx.beginPath();
    ctx.moveTo(sx, sy - 10);
    ctx.lineTo(sx - 4, sy - 4);
    ctx.lineTo(sx + 4, sy - 4);
    ctx.closePath();
    ctx.fill();
    ctx.font = "11px system-ui, sans-serif";
    ctx.fillStyle = "#cdeeff";
    ctx.textAlign = "center";
    ctx.fillText(`#${ship.id}${isControlled ? " (controlled)" : ""}`, sx, sy - 14);
    ctx.textAlign = "left";
  }
}

/** Silicon: arrow along its flow direction, or a cross when off. Block-local, y down. */
function drawControlState(ctx: CanvasRenderingContext2D, b: BlockView, half: number): void {
  if (b.m === "silicon") {
    ctx.strokeStyle = b.dir ? "#ffffff" : "rgba(0,0,0,0.55)";
    ctx.lineWidth = Math.max(1, half * 0.2);
    ctx.beginPath();
    if (!b.dir || b.dir === "all") {
      // Not conducting: a cross.
      const d = half * 0.4;
      ctx.moveTo(-d, -d);
      ctx.lineTo(d, d);
      ctx.moveTo(d, -d);
      ctx.lineTo(-d, d);
    } else {
      const [dx, dy] = { n: [0, -1], e: [1, 0], s: [0, 1], w: [-1, 0] }[b.dir] as [number, number];
      const l = half * 0.6;
      const h = half * 0.35;
      ctx.moveTo(-dx * l, -dy * l);
      ctx.lineTo(dx * l, dy * l);
      ctx.moveTo(dx * l - dx * h - dy * h, dy * l - dy * h + dx * h);
      ctx.lineTo(dx * l, dy * l);
      ctx.lineTo(dx * l - dx * h + dy * h, dy * l - dy * h - dx * h);
    }
    ctx.stroke();
  }
}

function drawGhost(
  ctx: CanvasRenderingContext2D,
  cam: Camera,
  cx: number,
  cy: number,
  game: GameState,
  poses: Map<ShipId, Pose>,
  source: BuildSelection,
  cell: Cell,
): void {
  const ship = game.find(source.ship);
  const pose = ship && poses.get(source.ship);
  if (!ship || !pose) return;
  const [wx, wy] = localToWorld(pose.x, pose.y, pose.rot, ship.com, cell);
  const [sx, sy] = worldToScreen(cam, cx, cy, wx, wy);
  const blockPx = Math.max(cam.zoom, 4);
  const half = blockPx / 2;
  ctx.save();
  ctx.translate(sx, sy);
  ctx.rotate(-pose.rot);
  ctx.setLineDash([4, 3]);
  ctx.lineWidth = 1.5;
  ctx.strokeStyle = "rgba(141,255,90,0.9)";
  ctx.strokeRect(-half, -half, blockPx, blockPx);
  ctx.restore();
}
