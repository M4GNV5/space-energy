// Mouse (pan/zoom/build/hover) and keyboard (binds, reserved keys) handling.

import { type Camera, clampZoom, screenToWorld, worldToScreen } from "./camera";
import type { Bind } from "./binds";
import { escapeHtml, formatEnergy, formatMass } from "./format";
import type { Hud } from "./hud";
import type { NetLike } from "./net";
import type { ScriptPanel } from "./scriptPanel";
import { MATERIALS, energyCapacity, type Cell, type ShipId } from "./protocol";
import type { GameState } from "./state";
import { type BuildSelection, type PickResult, localCellForShip, pickShipAt } from "./ships";
import { computePoses } from "./render";

export interface InputContext {
  getGame: () => GameState;
  getMyName: () => string;
  getControlledShip: () => ShipId | null;
  cycleControlledShip: () => void;
  getBinds: () => Bind[];
  scriptPanel: ScriptPanel;
}

const EMIT_INTERVAL_MS = 40;
const VIEW_THROTTLE_MS = 200;

export class InputController {
  follow = true;

  private hover: PickResult | null = null;
  private source: BuildSelection | null = null;
  private buildTargetCell: Cell | null = null;

  private dragging: { button: number; lastX: number; lastY: number; moved: boolean } | null = null;
  private activeKeys = new Map<string, number>();
  private lastSentView: { x: number; y: number; r: number } | null = null;
  private lastViewSentAt = 0;
  private tooltipEl: HTMLDivElement;
  private lastMouse: { x: number; y: number } | null = null;

  constructor(
    private canvas: HTMLCanvasElement,
    private cam: Camera,
    private net: NetLike,
    private hud: Hud,
    private ctx: InputContext,
  ) {
    this.tooltipEl = document.createElement("div");
    this.tooltipEl.className = "tooltip hidden";
    document.body.appendChild(this.tooltipEl);
    this.attach();
  }

  getHover(): PickResult | null {
    return this.hover;
  }
  getSource(): BuildSelection | null {
    return this.source;
  }
  getBuildTargetCell(): Cell | null {
    return this.buildTargetCell;
  }

  /** Call once per frame: re-picks under the cursor so hover and tooltip track the latest state. */
  refreshHover(): void {
    if (this.lastMouse) this.updateHover(this.lastMouse.x, this.lastMouse.y);
  }

  /** Call once per frame; throttles `view` messages to ~5/s. */
  maybeSendView(now: number): void {
    if (now - this.lastViewSentAt < VIEW_THROTTLE_MS) return;
    const w = this.canvas.clientWidth;
    const h = this.canvas.clientHeight;
    const r = Math.hypot(w, h) / 2 / this.cam.zoom;
    const view = { x: this.cam.x, y: this.cam.y, r };
    const last = this.lastSentView;
    if (last && last.x === view.x && last.y === view.y && Math.abs(last.r - view.r) < 1e-9) return;
    this.lastSentView = view;
    this.lastViewSentAt = now;
    this.net.send({ t: "view", x: view.x, y: view.y, r: view.r });
  }

  private attach(): void {
    this.canvas.addEventListener("wheel", (e) => this.onWheel(e), { passive: false });
    this.canvas.addEventListener("mousedown", (e) => this.onMouseDown(e));
    this.canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    window.addEventListener("mousemove", (e) => this.onMouseMove(e));
    window.addEventListener("mouseup", (e) => this.onMouseUp(e));
    window.addEventListener("keydown", (e) => this.onKeyDown(e));
    window.addEventListener("keyup", (e) => this.onKeyUp(e));
    window.addEventListener("blur", () => this.stopAllKeys());
  }

  private screenCenter(): [number, number] {
    return [this.canvas.clientWidth / 2, this.canvas.clientHeight / 2];
  }

  private toWorld(clientX: number, clientY: number): [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    const [cx, cy] = this.screenCenter();
    return screenToWorld(this.cam, cx, cy, clientX - rect.left, clientY - rect.top);
  }

  private onWheel(e: WheelEvent): void {
    e.preventDefault();
    const rect = this.canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const [cx, cy] = this.screenCenter();
    const [wx, wy] = screenToWorld(this.cam, cx, cy, sx, sy);
    const factor = Math.exp(-e.deltaY * 0.001);
    this.cam.zoom = clampZoom(this.cam.zoom * factor);
    const [nsx, nsy] = worldToScreen(this.cam, cx, cy, wx, wy);
    this.cam.x += (sx - nsx) / this.cam.zoom;
    this.cam.y -= (sy - nsy) / this.cam.zoom;
  }

  private onMouseDown(e: MouseEvent): void {
    if (e.button === 1 || e.button === 2) {
      this.dragging = { button: e.button, lastX: e.clientX, lastY: e.clientY, moved: false };
      e.preventDefault();
    } else if (e.button === 0) {
      this.handleLeftClick(e);
    }
  }

  private onMouseMove(e: MouseEvent): void {
    if (this.dragging) {
      const dx = e.clientX - this.dragging.lastX;
      const dy = e.clientY - this.dragging.lastY;
      if (Math.abs(dx) > 2 || Math.abs(dy) > 2) this.dragging.moved = true;
      this.dragging.lastX = e.clientX;
      this.dragging.lastY = e.clientY;
      this.cam.x -= dx / this.cam.zoom;
      this.cam.y += dy / this.cam.zoom;
      this.follow = false;
    }
    this.lastMouse = { x: e.clientX, y: e.clientY };
    this.updateHover(e.clientX, e.clientY);
  }

  private onMouseUp(e: MouseEvent): void {
    if (this.dragging && this.dragging.button === e.button) {
      const wasRightNoDrag = this.dragging.button === 2 && !this.dragging.moved;
      this.dragging = null;
      if (wasRightNoDrag) this.clearSource();
    }
  }

  private handleLeftClick(e: MouseEvent): void {
    const [wx, wy] = this.toWorld(e.clientX, e.clientY);
    const game = this.ctx.getGame();
    const myName = this.ctx.getMyName();
    const poses = computePoses(game, performance.now());

    if (this.hud.isPickingBlock()) {
      const pick = pickShipAt(game.ships, poses, wx, wy);
      if (pick && pick.block && pick.ship.id === this.ctx.getControlledShip()) {
        this.hud.finishBlockPick(pick.cell);
      } else {
        this.hud.toast("click a block of your controlled ship (Esc to cancel)");
      }
      return;
    }

    if (this.source) {
      const ship = game.find(this.source.ship);
      const pose = ship && poses.get(this.source.ship);
      if (ship && pose) {
        const cell = localCellForShip(ship, pose, wx, wy);
        const kg = this.hud.getMoveKg();
        this.net.send({ t: "move_mass", ship: ship.id, from: this.source.cell, to: cell, kg });
      }
      return;
    }

    const pick = pickShipAt(game.ships, poses, wx, wy);
    if (pick && pick.block && pick.ship.owner === myName) {
      this.source = { ship: pick.ship.id, cell: pick.cell };
    }
  }

  private clearSource(): void {
    this.source = null;
    this.buildTargetCell = null;
  }

  private updateHover(clientX: number, clientY: number): void {
    const [wx, wy] = this.toWorld(clientX, clientY);
    const game = this.ctx.getGame();
    const poses = computePoses(game, performance.now());
    const pick = pickShipAt(game.ships, poses, wx, wy);
    this.hover = pick && pick.block ? pick : null;

    if (this.source) {
      const ship = game.find(this.source.ship);
      const pose = ship && poses.get(this.source.ship);
      this.buildTargetCell = ship && pose ? localCellForShip(ship, pose, wx, wy) : null;
    } else {
      this.buildTargetCell = null;
    }

    this.updateTooltip(clientX, clientY);
  }

  private updateTooltip(clientX: number, clientY: number): void {
    if (!this.hover || !this.hover.block) {
      this.tooltipEl.classList.add("hidden");
      return;
    }
    const b = this.hover.block;
    const mat = MATERIALS[b.m];
    const cap = energyCapacity(b);
    const fillPct = cap > 0 ? Math.round((b.energy / cap) * 100) : 0;
    const owner = this.hover.ship.owner ?? "asteroid";
    const control = b.m === "silicon" ? (b.dir ? ` — points ${b.dir}` : " — off") : "";
    this.tooltipEl.innerHTML =
      `ship #${this.hover.ship.id} — ${escapeHtml(owner)}<br/>` +
      `${b.m}${control}<br/>` +
      `mass ${formatMass(b.mass)}/${mat.maxMass} kg<br/>` +
      `energy ${formatEnergy(b.energy)}/${formatEnergy(cap)} (${fillPct}%)`;
    this.tooltipEl.classList.remove("hidden");
    this.tooltipEl.style.left = `${clientX + 14}px`;
    this.tooltipEl.style.top = `${clientY + 14}px`;
  }

  private onKeyDown(e: KeyboardEvent): void {
    if (isTypingTarget(e.target)) return;
    if (this.hud.isCapturingKey()) {
      if (this.hud.handleCaptureKeydown(e.key)) e.preventDefault();
      return;
    }
    const key = normalizeKey(e.key);

    if (key === "tab") {
      e.preventDefault();
      this.ctx.cycleControlledShip();
      return;
    }
    if (key === "f") {
      if (!e.repeat) this.follow = !this.follow;
      return;
    }
    if (key === "c") {
      if (!e.repeat) {
        const ship = this.ctx.getControlledShip();
        if (ship != null) this.net.send({ t: "collect", ship });
      }
      return;
    }
    if (key === "b") {
      if (!e.repeat) {
        this.ctx.scriptPanel.close();
        this.hud.toggleBindEditor();
      }
      return;
    }
    if (key === "r") {
      if (!e.repeat) {
        if (this.hud.isBindEditorOpen()) this.hud.toggleBindEditor();
        this.ctx.scriptPanel.toggle();
      }
      // Otherwise the key press would also be typed into the editor that just got focus.
      e.preventDefault();
      return;
    }
    if (key === "escape") {
      this.hud.cancelBlockPick();
      this.clearSource();
      return;
    }
    if (e.repeat) return;
    if (this.activeKeys.has(key)) return;

    const bind = this.ctx.getBinds().find((b) => b.key === key);
    if (!bind) return;

    const fire = () => {
      const shipId = this.ctx.getControlledShip();
      if (shipId == null) return;
      for (const action of bind.actions) {
        this.net.send({
          t: "emit",
          ship: shipId,
          block: action.block,
          dir: action.dir,
          energy: action.energy,
          mass: action.mass,
        });
      }
    };
    fire();
    const id = window.setInterval(fire, EMIT_INTERVAL_MS);
    this.activeKeys.set(key, id);
  }

  private onKeyUp(e: KeyboardEvent): void {
    this.stopKey(normalizeKey(e.key));
  }

  private stopKey(key: string): void {
    const id = this.activeKeys.get(key);
    if (id != null) {
      clearInterval(id);
      this.activeKeys.delete(key);
    }
  }

  private stopAllKeys(): void {
    for (const id of this.activeKeys.values()) clearInterval(id);
    this.activeKeys.clear();
  }
}

function normalizeKey(key: string): string {
  return key === " " ? " " : key.toLowerCase();
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}
