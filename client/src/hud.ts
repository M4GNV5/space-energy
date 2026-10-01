// Plain-DOM HUD: top-left info panel, toasts, connection-lost overlay and the
// key-bind editor panel. No canvas drawing here.

import { type Bind, type BindAction, DEFAULT_BINDS, RESERVED_KEYS, saveBinds } from "./binds";
import { escapeHtml } from "./format";
import type { Cell, Dir } from "./protocol";

export interface HudInfo {
  player: string;
  controlledShip: number | null;
  speed: number; // m/s
  mass: number; // kg
  avgFill: number; // 0..1
  maxFill: number; // 0..1
  spectator: boolean;
  follow: boolean;
}

const DIRS: Dir[] = ["n", "e", "s", "w", "all"];

export class Hud {
  readonly root: HTMLDivElement;
  private info: HTMLDivElement;
  private moveKgInput: HTMLInputElement;
  private toasts: HTMLDivElement;
  private connLost: HTMLDivElement;
  private bindPanel: HTMLDivElement;
  private bindList: HTMLDivElement;
  private capturingKey = false;
  /** Action waiting for the player to click its block on the ship. */
  private pickingFor: BindAction | null = null;

  private binds: Bind[];
  private onBindsChanged: (binds: Bind[]) => void;

  constructor(parent: HTMLElement, initialBinds: Bind[], onBindsChanged: (binds: Bind[]) => void) {
    this.binds = initialBinds;
    this.onBindsChanged = onBindsChanged;

    this.root = document.createElement("div");
    this.root.className = "hud";
    parent.appendChild(this.root);

    this.info = document.createElement("div");
    this.info.className = "hud-panel hud-info";
    this.root.appendChild(this.info);

    const controls = document.createElement("div");
    controls.className = "hud-panel hud-controls";
    controls.innerHTML = `<label>move kg <input type="number" min="0" step="1" value="100" id="move-kg"/></label>`;
    this.root.appendChild(controls);
    this.moveKgInput = controls.querySelector("#move-kg")!;

    this.toasts = document.createElement("div");
    this.toasts.className = "toasts";
    parent.appendChild(this.toasts);

    this.connLost = document.createElement("div");
    this.connLost.className = "overlay hidden";
    parent.appendChild(this.connLost);

    this.bindPanel = document.createElement("div");
    this.bindPanel.className = "bind-panel hidden";
    parent.appendChild(this.bindPanel);
    this.bindList = document.createElement("div");
    this.bindPanel.appendChild(this.bindList);
    this.renderBindEditor();
  }

  setInfo(info: HudInfo): void {
    if (info.spectator) {
      this.info.innerHTML = `<div><b>${escapeHtml(info.player)}</b></div>
        <div class="warn">SPECTATOR — no ships left, reload and log in with a new name to play again</div>
        ${this.helpLine()}`;
      return;
    }
    const pct = (v: number) => `${Math.round(v * 100)}%`;
    this.info.innerHTML = `
      <div><b>${escapeHtml(info.player)}</b> — ship #${info.controlledShip ?? "-"}</div>
      <div>speed ${info.speed.toFixed(1)} m/s — mass ${Math.round(info.mass)} kg</div>
      <div>fill avg ${pct(info.avgFill)} / max ${pct(info.maxFill)}</div>
      <div>follow: ${info.follow ? "on" : "off"} (F)</div>
      ${this.helpLine()}
    `;
  }

  private helpLine(): string {
    return `<div class="help">F follow · Tab switch ship · C collect · B binds · wheel zoom · right/middle drag pan</div>`;
  }

  getMoveKg(): number {
    const v = parseFloat(this.moveKgInput.value);
    return Number.isFinite(v) && v > 0 ? v : 100;
  }

  toast(msg: string): void {
    const el = document.createElement("div");
    el.className = "toast";
    el.textContent = msg;
    this.toasts.appendChild(el);
    setTimeout(() => el.remove(), 4000);
  }

  setConnectionLost(show: boolean, onReconnect: () => void): void {
    if (!show) {
      this.connLost.classList.add("hidden");
      this.connLost.innerHTML = "";
      return;
    }
    this.connLost.classList.remove("hidden");
    this.connLost.innerHTML = `<div class="overlay-box">
      <div>Connection lost.</div>
      <button id="reconnect-btn">Reconnect</button>
    </div>`;
    this.connLost.querySelector("#reconnect-btn")!.addEventListener("click", onReconnect);
  }

  toggleBindEditor(): void {
    this.bindPanel.classList.toggle("hidden");
    this.cancelBlockPick();
  }

  isPickingBlock(): boolean {
    return this.pickingFor != null;
  }

  /** Complete a "pick block" started in the bind editor with the clicked cell. */
  finishBlockPick(cell: Cell): void {
    if (!this.pickingFor) return;
    this.pickingFor.block = [cell[0], cell[1]];
    this.pickingFor = null;
    this.commit();
  }

  cancelBlockPick(): void {
    if (!this.pickingFor) return;
    this.pickingFor = null;
    this.renderBindEditor();
  }

  isBindEditorOpen(): boolean {
    return !this.bindPanel.classList.contains("hidden");
  }

  isCapturingKey(): boolean {
    return this.capturingKey;
  }

  /** Feed a keydown into the "add bind" key-capture flow. Returns true if consumed. */
  handleCaptureKeydown(key: string): boolean {
    if (!this.capturingKey) return false;
    this.capturingKey = false;
    const k = key.toLowerCase();
    if (RESERVED_KEYS.has(k) || this.binds.some((b) => b.key === k)) {
      this.renderBindEditor();
      return true;
    }
    this.binds = [...this.binds, { key: k, actions: [] }];
    this.commit();
    return true;
  }

  private commit(): void {
    saveBinds(this.binds);
    this.onBindsChanged(this.binds);
    this.renderBindEditor();
  }

  private renderBindEditor(): void {
    this.bindList.innerHTML = "";
    const title = document.createElement("div");
    title.className = "bind-title";
    title.textContent = "Key binds (B to close)";
    this.bindList.appendChild(title);

    for (const bind of this.binds) {
      this.bindList.appendChild(this.renderBindRow(bind));
    }

    const addBindBtn = document.createElement("button");
    addBindBtn.textContent = this.capturingKey ? "press a key..." : "+ add bind";
    addBindBtn.addEventListener("click", () => {
      this.capturingKey = true;
      this.renderBindEditor();
    });
    this.bindList.appendChild(addBindBtn);

    const resetBtn = document.createElement("button");
    resetBtn.textContent = "reset to defaults";
    resetBtn.addEventListener("click", () => {
      this.binds = JSON.parse(JSON.stringify(DEFAULT_BINDS)) as Bind[];
      this.pickingFor = null;
      this.commit();
    });
    this.bindList.appendChild(resetBtn);
  }

  private renderBindRow(bind: Bind): HTMLDivElement {
    const row = document.createElement("div");
    row.className = "bind-row";

    const header = document.createElement("div");
    header.className = "bind-row-header";
    header.innerHTML = `<b>${escapeHtml(bind.key === " " ? "space" : bind.key)}</b>`;
    const removeBindBtn = document.createElement("button");
    removeBindBtn.textContent = "remove bind";
    removeBindBtn.addEventListener("click", () => {
      this.binds = this.binds.filter((b) => b !== bind);
      this.commit();
    });
    header.appendChild(removeBindBtn);
    row.appendChild(header);

    for (const action of bind.actions) {
      row.appendChild(this.renderActionRow(bind, action));
    }

    const addActionBtn = document.createElement("button");
    addActionBtn.textContent = "+ action";
    addActionBtn.addEventListener("click", () => {
      bind.actions.push({ block: [0, 0], dir: "n", energy: 0, mass: 0 });
      this.commit();
    });
    row.appendChild(addActionBtn);
    return row;
  }

  private renderActionRow(bind: Bind, action: BindAction): HTMLDivElement {
    const row = document.createElement("div");
    row.className = "action-row";

    const picking = this.pickingFor === action;
    const blockLabel = document.createElement("span");
    blockLabel.textContent = `block [${action.block[0]}, ${action.block[1]}]`;
    row.appendChild(blockLabel);

    const pickBtn = document.createElement("button");
    pickBtn.textContent = picking ? "click a block... (Esc)" : "pick a block";
    pickBtn.addEventListener("click", () => {
      this.pickingFor = picking ? null : action;
      this.renderBindEditor();
    });
    row.appendChild(pickBtn);

    const dirSelect = document.createElement("select");
    for (const d of DIRS) {
      const opt = document.createElement("option");
      opt.value = d;
      opt.textContent = d;
      if (d === action.dir) opt.selected = true;
      dirSelect.appendChild(opt);
    }
    dirSelect.addEventListener("change", () => {
      action.dir = dirSelect.value as Dir;
      this.commit();
    });
    row.appendChild(labelWrap("dir", dirSelect));

    const energyInput = numberInput(action.energy, (v) => {
      action.energy = v;
      this.commit();
    });
    row.appendChild(labelWrap("energy", energyInput));

    const massInput = numberInput(action.mass, (v) => {
      action.mass = v;
      this.commit();
    });
    row.appendChild(labelWrap("mass", massInput));

    const removeBtn = document.createElement("button");
    removeBtn.textContent = "x";
    removeBtn.title = "remove action";
    removeBtn.addEventListener("click", () => {
      if (picking) this.pickingFor = null;
      bind.actions = bind.actions.filter((a) => a !== action);
      this.commit();
    });
    row.appendChild(removeBtn);

    return row;
  }
}

function numberInput(value: number, onChange: (v: number) => void): HTMLInputElement {
  const input = document.createElement("input");
  input.type = "number";
  input.value = String(value);
  input.step = "any";
  input.addEventListener("change", () => {
    const v = parseFloat(input.value);
    onChange(Number.isFinite(v) ? v : 0);
  });
  return input;
}

function labelWrap(text: string, el: HTMLElement): HTMLLabelElement {
  const label = document.createElement("label");
  label.textContent = text + " ";
  label.appendChild(el);
  return label;
}

