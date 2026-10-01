// One script per ship, each in its own worker. Feeds them every server state,
// forwards the commands they issue and kills scripts that hang.
// Sources are persisted to localStorage per player and ship.

import { DEFAULT_SCRIPT } from "./defaultScript";
import type { Cell, ClientMsg, ShipId, StateMsg } from "./protocol";
import type { ScriptButton, ScriptResult } from "./scriptRuntime";
import type { WorkerRequest } from "./scriptWorker";

/** A script that does not answer for this long is killed. */
const WATCHDOG_MS = 1000;
const MAX_LOG_LINES = 200;

export interface ScriptStatus {
  state: "stopped" | "running" | "error";
  error: string | null;
  buttons: ScriptButton[];
  logs: string[];
}

interface Stored {
  src: string;
  /** Start the script automatically when the ship shows up. */
  enabled: boolean;
}

interface Entry extends ScriptStatus {
  worker: Worker | null;
  /** Requests the worker has not answered yet. */
  pending: number;
  pendingSince: number;
}

export class ScriptManager {
  private entries = new Map<ShipId, Entry>();
  private lastState: StateMsg | null = null;

  constructor(
    private player: string,
    private send: (msg: ClientMsg) => void,
    /** Called whenever a ship's status, menu or log changed. */
    private onChange: (ship: ShipId) => void,
    /** Show a popup a script asked for. */
    private toast: (ship: ShipId, text: string) => void,
    /** The cell of `ship` the player has clicked, if any. */
    private selected: (ship: ShipId) => Cell | null,
  ) {}

  source(ship: ShipId): string {
    return this.load(ship)?.src ?? DEFAULT_SCRIPT;
  }

  /** Persist an edited source without (re)starting it. */
  saveDraft(ship: ShipId, src: string): void {
    this.store(ship, { src, enabled: this.load(ship)?.enabled ?? false });
  }

  status(ship: ShipId): ScriptStatus {
    return this.entry(ship);
  }

  /** Save `src` and (re)start the ship's script with it. */
  run(ship: ShipId, src: string): void {
    this.store(ship, { src, enabled: true });
    this.kill(ship);
    const e = this.entry(ship);
    e.logs = [];
    if (!this.lastState?.ships.some((s) => s.id === ship)) {
      this.fail(ship, "ship is not in view");
      return;
    }
    const worker = new Worker(new URL("./scriptWorker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (ev: MessageEvent<ScriptResult>) => {
      if (e.worker === worker) this.onResult(ship, ev.data);
    };
    worker.onerror = (ev) => {
      if (e.worker === worker) this.fail(ship, ev.message || "worker failed");
    };
    e.worker = worker;
    e.state = "running";
    e.error = null;
    this.post(e, { t: "start", ship, src, state: this.lastState, selected: this.selected(ship) });
    this.onChange(ship);
  }

  stop(ship: ShipId): void {
    const stored = this.load(ship);
    if (stored) this.store(ship, { ...stored, enabled: false });
    this.kill(ship);
    const e = this.entry(ship);
    e.state = "stopped";
    e.error = null;
    this.onChange(ship);
  }

  click(ship: ShipId, button: number): void {
    const e = this.entries.get(ship);
    if (e?.worker) this.post(e, { t: "click", id: button, selected: this.selected(ship) });
  }

  /** Call for every server state: runs `loop()` of all scripts and auto-starts saved ones. */
  onState(state: StateMsg): void {
    this.lastState = state;
    const now = performance.now();
    for (const ship of state.ships) {
      if (ship.owner !== this.player) continue;
      const e = this.entries.get(ship.id);
      if (!e) {
        const stored = this.load(ship.id);
        if (stored?.enabled) this.run(ship.id, stored.src);
        else this.entry(ship.id);
        continue;
      }
      if (!e.worker) continue;
      if (e.pending === 0) this.post(e, { t: "tick", state, selected: this.selected(ship.id) });
      else if (now - e.pendingSince > WATCHDOG_MS) this.fail(ship.id, "script took longer than 1 s (endless loop?) and was stopped");
      // else: still busy with the previous tick, skip this one.
    }
  }

  private onResult(ship: ShipId, res: ScriptResult): void {
    const e = this.entry(ship);
    e.pending = Math.max(0, e.pending - 1);
    e.pendingSince = performance.now();
    for (const cmd of res.cmds) this.send({ ...cmd, ship });
    for (const text of res.toasts) this.toast(ship, text);
    if (res.buttons) e.buttons = res.buttons;
    if (res.logs.length > 0) e.logs = [...e.logs, ...res.logs].slice(-MAX_LOG_LINES);
    if (res.error != null) this.fail(ship, res.error);
    else if (res.buttons || res.logs.length > 0) this.onChange(ship);
  }

  private post(e: Entry, req: WorkerRequest): void {
    if (e.pending === 0) e.pendingSince = performance.now();
    e.pending++;
    e.worker!.postMessage(req);
  }

  private fail(ship: ShipId, error: string): void {
    this.kill(ship);
    const e = this.entry(ship);
    e.state = "error";
    e.error = error;
    this.onChange(ship);
  }

  private kill(ship: ShipId): void {
    const e = this.entries.get(ship);
    if (!e) return;
    e.worker?.terminate();
    e.worker = null;
    e.pending = 0;
    e.buttons = [];
  }

  private entry(ship: ShipId): Entry {
    let e = this.entries.get(ship);
    if (!e) {
      e = { state: "stopped", error: null, buttons: [], logs: [], worker: null, pending: 0, pendingSince: 0 };
      this.entries.set(ship, e);
    }
    return e;
  }

  private key(ship: ShipId): string {
    return `space-energy-script-v1:${this.player}:${ship}`;
  }

  private load(ship: ShipId): Stored | null {
    try {
      const raw = localStorage.getItem(this.key(ship));
      if (!raw) return null;
      const parsed = JSON.parse(raw) as Partial<Stored>;
      return typeof parsed.src === "string" ? { src: parsed.src, enabled: parsed.enabled === true } : null;
    } catch {
      return null;
    }
  }

  private store(ship: ShipId, stored: Stored): void {
    try {
      localStorage.setItem(this.key(ship), JSON.stringify(stored));
    } catch {
      // localStorage unavailable (private mode, quota, ...) - scripts just won't persist.
    }
  }
}
