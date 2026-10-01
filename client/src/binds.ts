// Key bindings: key -> list of emit actions. Persisted to localStorage.

import type { Cell, Dir } from "./protocol";

export interface BindAction {
  block: Cell;
  dir: Dir;
  energy: number;
  mass: number;
}

export interface Bind {
  key: string;
  actions: BindAction[];
}

/** Keys the bind editor must refuse: they have fixed meanings. */
export const RESERVED_KEYS = new Set(["f", "c", "b", "r", "tab", "escape"]);

// Cells refer to the starter ship: `STARTER_LAYOUT` in server/src/sim/worldgen.rs.
export const DEFAULT_BINDS: Bind[] = [
  {
    // Forward: the five main engines.
    key: "w",
    actions: [-2, -1, 0, 1, 2].map((x): BindAction => ({ block: [x, -3], dir: "s", energy: 2000, mass: 1.5 })),
  },
  {
    // Brake / reverse: the two front thrusters.
    key: "s",
    actions: [
      { block: [-2, 4], dir: "n", energy: 2000, mass: 2 },
      { block: [2, 4], dir: "n", energy: 2000, mass: 2 },
    ],
  },
  {
    // Turn left: front-right thruster pushes the nose left, rear-left engine pushes the tail right.
    key: "a",
    actions: [
      { block: [2, 4], dir: "e", energy: 2000, mass: 2 },
      { block: [-2, -3], dir: "w", energy: 2000, mass: 2 },
    ],
  },
  {
    key: "d",
    actions: [
      { block: [-2, 4], dir: "w", energy: 2000, mass: 2 },
      { block: [2, -3], dir: "e", energy: 2000, mass: 2 },
    ],
  },
  {
    // Strafe left: both right-hand thrusters push the ship sideways without turning it.
    key: "q",
    actions: [
      { block: [2, 4], dir: "e", energy: 2000, mass: 2 },
      { block: [2, -3], dir: "e", energy: 2000, mass: 2 },
    ],
  },
  {
    key: "e",
    actions: [
      { block: [-2, 4], dir: "w", energy: 2000, mass: 2 },
      { block: [-2, -3], dir: "w", energy: 2000, mass: 2 },
    ],
  },
  {
    // Laser at the nose tip.
    key: " ",
    actions: [{ block: [0, 10], dir: "n", energy: 100000, mass: 0 }],
  },
  {
    // Vent hull heat into space from the nose corners.
    key: "x",
    actions: [
      { block: [-2, 9], dir: "n", energy: 100000, mass: 0 },
      { block: [2, 9], dir: "n", energy: 100000, mass: 0 },
    ],
  },
  // Reactor outlets (silicon): 1/2 open/close the outlet to the battery and laser,
  // 3/4 open/close the outlet to the engines.
  { key: "1", actions: [{ block: [0, 2], dir: "n", energy: 1, mass: 0 }] },
  { key: "2", actions: [{ block: [0, 2], dir: "all", energy: 1, mass: 0 }] },
  { key: "3", actions: [{ block: [0, 0], dir: "s", energy: 1, mass: 0 }] },
  { key: "4", actions: [{ block: [0, 0], dir: "all", energy: 1, mass: 0 }] },
];

// Bump the version when the starter ship layout or the emit rules change, so stale binds are dropped.
const STORAGE_KEY = "space-energy-binds-v4";

function cloneDefaults(): Bind[] {
  return JSON.parse(JSON.stringify(DEFAULT_BINDS)) as Bind[];
}

export function loadBinds(): Bind[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return cloneDefaults();
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return cloneDefaults();
    return parsed as Bind[];
  } catch {
    return cloneDefaults();
  }
}

export function saveBinds(binds: Bind[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(binds));
  } catch {
    // localStorage unavailable (private mode, quota, ...) - binds just won't persist.
  }
}
