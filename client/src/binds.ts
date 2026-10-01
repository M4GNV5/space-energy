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
export const RESERVED_KEYS = new Set(["f", "c", "b", "tab", "escape"]);

export const DEFAULT_BINDS: Bind[] = [
  {
    key: "w",
    actions: [
      { block: [-1, -1], dir: "s", energy: 0, mass: 2 },
      { block: [1, -1], dir: "s", energy: 0, mass: 2 },
    ],
  },
  {
    key: "s",
    actions: [
      { block: [-1, 1], dir: "n", energy: 0, mass: 2 },
      { block: [1, 1], dir: "n", energy: 0, mass: 2 },
    ],
  },
  {
    key: "a",
    actions: [
      { block: [1, 1], dir: "e", energy: 0, mass: 0.5 },
      { block: [-1, -1], dir: "w", energy: 0, mass: 0.5 },
    ],
  },
  {
    key: "d",
    actions: [
      { block: [-1, 1], dir: "w", energy: 0, mass: 0.5 },
      { block: [1, -1], dir: "e", energy: 0, mass: 0.5 },
    ],
  },
  {
    key: " ",
    actions: [{ block: [0, 1], dir: "n", energy: 100000, mass: 0 }],
  },
  {
    key: "x",
    actions: [{ block: [0, -1], dir: "s", energy: 10000, mass: 0 }],
  },
];

const STORAGE_KEY = "space-energy-binds";

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
