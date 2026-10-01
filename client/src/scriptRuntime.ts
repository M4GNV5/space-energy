// Runs one ship's script: compiles the source, builds the `ship` / `world` /
// `menu` / `log` globals it sees and collects the commands it issues.
// No DOM and no worker globals in here, so client/test/* can drive it directly;
// `scriptWorker.ts` is the thin glue that runs it off the main thread.

import { rotate } from "./camera";
import {
  energyCapacity,
  type BlockView,
  type Cell,
  type ClientMsg,
  type Dir,
  type Material,
  MATERIALS,
  type ShipId,
  type ShipView,
  type StateMsg,
} from "./protocol";

/** Seconds per server tick. Keep in sync with `TICK_RATE` in server/src/sim/consts.rs. */
export const TICK_DT = 1 / 25;
/** Commands a script may issue per tick; the rest is dropped. */
export const MAX_CMDS_PER_TICK = 256;
const MAX_LOGS_PER_TICK = 50;
const MAX_TOASTS_PER_TICK = 5;

export type ScriptCmd = Extract<ClientMsg, { t: "emit" | "move_mass" | "collect" }>;

export interface ScriptButton {
  id: number;
  label: string;
  active: boolean;
}

export interface ScriptResult {
  cmds: ScriptCmd[];
  /** The whole menu, or null if it did not change. */
  buttons: ScriptButton[] | null;
  logs: string[];
  /** Popups to show the player. */
  toasts: string[];
  /** Set when the script threw or failed to compile. The script is dead afterwards. */
  error: string | null;
}

/** A block as scripts see it: the wire block plus derived values. */
export interface ScriptBlock extends BlockView {
  /** J the block can hold before it bursts. */
  capacity: number;
  /** energy / capacity, 0..1. */
  fill: number;
}

type ScriptShipView = Omit<ShipView, "blocks"> & { blocks: ScriptBlock[]; mass: number };

const DIRS: Dir[] = ["n", "e", "s", "w", "all"];

function decorate(ship: ShipView): ScriptShipView {
  let mass = 0;
  for (const b of ship.blocks as ScriptBlock[]) {
    b.capacity = energyCapacity(b);
    b.fill = b.capacity > 0 ? b.energy / b.capacity : 0;
    mass += b.mass;
  }
  return Object.assign(ship as ScriptShipView, { mass });
}

function cell(v: unknown, what: string): Cell {
  if (!Array.isArray(v) || v.length !== 2 || !Number.isInteger(v[0]) || !Number.isInteger(v[1])) {
    throw new TypeError(`${what} must be a cell [x, y], got ${fmt(v)}`);
  }
  return [v[0], v[1]];
}

function amount(v: unknown, what: string): number {
  if (typeof v !== "number" || !Number.isFinite(v) || v < 0) {
    throw new TypeError(`${what} must be a number >= 0, got ${fmt(v)}`);
  }
  return v;
}

function fmt(v: unknown): string {
  if (typeof v === "string") return v;
  if (typeof v === "number") return Number.isInteger(v) ? String(v) : v.toFixed(3);
  if (v instanceof Error) return String(v);
  try {
    return JSON.stringify(v) ?? String(v);
  } catch {
    return String(v);
  }
}

export class ScriptRuntime {
  private loopFn: (() => void) | null = null;
  private cmds: ScriptCmd[] = [];
  private logs: string[] = [];
  private toasts: string[] = [];
  private buttons: Array<ScriptButton & { onClick: () => void }> = [];
  private menuDirty = false;
  private nextButtonId = 1;
  private dead = false;

  // Stable objects, updated in place every tick, so scripts may keep references.
  private ship: Record<string, unknown>;
  private world: Record<string, unknown>;
  private menu: Record<string, unknown>;

  constructor(private shipId: ShipId) {
    const push = (cmd: ScriptCmd) => {
      if (this.cmds.length === MAX_CMDS_PER_TICK) this.log(`more than ${MAX_CMDS_PER_TICK} commands in one tick, dropping the rest`);
      if (this.cmds.length < MAX_CMDS_PER_TICK) this.cmds.push(cmd);
    };

    this.ship = {
      id: shipId,
      blocks: [],
      selectedBlock: undefined,
      block: (x: number, y: number) =>
        (this.ship.blocks as ScriptBlock[]).find((b) => b.p[0] === x && b.p[1] === y),
      toLocal: (x: number, y: number) => rotate(x, y, -(this.ship.rot as number)),
      toWorld: (x: number, y: number) => rotate(x, y, this.ship.rot as number),
      emit: (block: unknown, dir: unknown, energy: unknown = 0, mass: unknown = 0) => {
        if (!DIRS.includes(dir as Dir)) throw new TypeError(`dir must be one of ${DIRS.join(", ")}, got ${fmt(dir)}`);
        push({
          t: "emit",
          ship: shipId,
          block: cell(block, "block"),
          dir: dir as Dir,
          energy: amount(energy, "energy"),
          mass: amount(mass, "mass"),
        });
      },
      moveMass: (from: unknown, to: unknown, kg: unknown) => {
        push({ t: "move_mass", ship: shipId, from: cell(from, "from"), to: cell(to, "to"), kg: amount(kg, "kg") });
      },
      collect: (block?: unknown, material?: unknown) => {
        if (material != null && !(String(material) in MATERIALS)) throw new TypeError(`unknown material ${fmt(material)}`);
        const cmd: ScriptCmd = { t: "collect", ship: shipId };
        if (block != null) cmd.block = cell(block, "block");
        if (material != null) cmd.material = material as Material;
        push(cmd);
      },
    };

    this.world = { tick: 0, dt: TICK_DT, ships: [], suns: [], packets: [], rays: [] };

    this.menu = {
      button: (label: unknown, onClick: unknown) => {
        if (typeof onClick !== "function") throw new TypeError("menu.button(label, onClick): onClick must be a function");
        const button = { id: this.nextButtonId++, label: String(label), active: false, onClick: onClick as () => void };
        this.buttons.push(button);
        this.menuDirty = true;
        return {
          setLabel: (l: unknown) => {
            button.label = String(l);
            this.menuDirty = true;
          },
          setActive: (on: unknown) => {
            button.active = Boolean(on);
            this.menuDirty = true;
          },
          remove: () => {
            this.buttons = this.buttons.filter((b) => b !== button);
            this.menuDirty = true;
          },
        };
      },
      clear: () => {
        this.buttons = [];
        this.menuDirty = true;
      },
    };
  }

  /**
   * Compile `src` and run its `setup()`. `state` must contain the ship.
   * `selected` is the cell of this ship the player has clicked, if any.
   */
  start(src: string, state: StateMsg, selected: Cell | null = null): ScriptResult {
    if (!this.update(state, selected)) return this.fail("ship is not in view");
    let fns: { setup: unknown; loop: unknown };
    try {
      const compile = new Function(
        "ship",
        "world",
        "menu",
        "log",
        "toast",
        `"use strict";\n${src}\n;return { setup: typeof setup === "function" ? setup : null, loop: typeof loop === "function" ? loop : null };`,
      );
      fns = compile(
        this.ship,
        this.world,
        this.menu,
        (...args: unknown[]) => this.log(args.map(fmt).join(" ")),
        (...args: unknown[]) => {
          if (this.toasts.length < MAX_TOASTS_PER_TICK) this.toasts.push(args.map(fmt).join(" "));
        },
      );
    } catch (e) {
      return this.fail(String(e));
    }
    if (typeof fns.setup !== "function" || typeof fns.loop !== "function") {
      return this.fail("script must define `function setup()` and `function loop()`");
    }
    this.loopFn = fns.loop as () => void;
    return this.call("setup", fns.setup as () => void);
  }

  /** Run `loop()` for a new server state. Does nothing while the ship is out of view. */
  tick(state: StateMsg, selected: Cell | null = null): ScriptResult {
    if (this.dead || !this.loopFn || !this.update(state, selected)) return this.flush(null);
    return this.call("loop", this.loopFn);
  }

  click(id: number, selected: Cell | null = null): ScriptResult {
    this.select(selected);
    const button = this.buttons.find((b) => b.id === id);
    if (this.dead || !button) return this.flush(null);
    return this.call(`button "${button.label}"`, button.onClick);
  }

  private select(selected: Cell | null): void {
    const block = this.ship.block as (x: number, y: number) => ScriptBlock | undefined;
    this.ship.selectedBlock = selected ? block(selected[0], selected[1]) : undefined;
  }

  private update(state: StateMsg, selected: Cell | null): boolean {
    const own = state.ships.find((s) => s.id === this.shipId);
    if (!own) return false;
    Object.assign(this.ship, decorate(own));
    this.select(selected);
    Object.assign(this.world, {
      tick: state.tick,
      ships: state.ships.filter((s) => s !== own).map(decorate),
      suns: state.suns,
      packets: state.packets,
      rays: state.rays,
    });
    return true;
  }

  private call(what: string, fn: () => void): ScriptResult {
    try {
      fn();
    } catch (e) {
      return this.fail(`${what}: ${e}`);
    }
    return this.flush(null);
  }

  private log(line: string): void {
    if (this.logs.length < MAX_LOGS_PER_TICK) this.logs.push(line);
  }

  private fail(error: string): ScriptResult {
    this.dead = true;
    this.cmds = [];
    this.buttons = [];
    this.menuDirty = true;
    return this.flush(error);
  }

  private flush(error: string | null): ScriptResult {
    const res: ScriptResult = {
      cmds: this.cmds,
      buttons: this.menuDirty ? this.buttons.map(({ id, label, active }) => ({ id, label, active })) : null,
      logs: this.logs,
      toasts: this.toasts,
      error,
    };
    this.cmds = [];
    this.logs = [];
    this.toasts = [];
    this.menuDirty = false;
    return res;
  }
}
