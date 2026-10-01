// Dev-only fake server, enabled via `?mock=1`. Fabricates a state message on
// an interval so rendering/input logic can be sanity-checked without the Rust
// server running. Implements the same interface as `Net`.

import type { NetHandlers, NetLike } from "./net";
import { MATERIALS, type BlockView, type ClientMsg, type Material, type ShipView, type StateMsg } from "./protocol";

const START_BLOCKS: Array<{ p: [number, number]; m: Material; mass: number }> = [
  { p: [-1, 1], m: "lead", mass: 5000 },
  { p: [0, 1], m: "iron", mass: 4000 },
  { p: [1, 1], m: "lead", mass: 5000 },
  { p: [-1, 0], m: "copper", mass: 4000 },
  { p: [0, 0], m: "uranium", mass: 8000 },
  { p: [1, 0], m: "copper", mass: 4000 },
  { p: [-1, -1], m: "lead", mass: 5000 },
  { p: [0, -1], m: "tungsten", mass: 9000 },
  { p: [1, -1], m: "lead", mass: 5000 },
];

function energyFor(m: Material, t: number): number {
  const cap = MATERIALS[m].energyCapacity;
  if (m === "tungsten") return cap * (0.5 + 0.45 * Math.sin(t * 0.3)); // sweeps past the 80% burst-warning line
  if (m === "uranium") return cap * Math.min(1, 0.1 + t * 0.01);
  return cap * (0.2 + 0.1 * Math.sin(t + m.length));
}

export class MockNet implements NetLike {
  private timer: number | null = null;
  private t = 0;
  private name = "player";

  constructor(private handlers: NetHandlers) {}

  connect(name: string): void {
    this.name = name;
    setTimeout(() => {
      this.handlers.onOpen?.();
      this.handlers.onWelcome(name, [1]);
      this.timer = window.setInterval(() => this.tick(), 40);
    }, 50);
  }

  send(msg: ClientMsg): void {
    console.log("[mock] send", msg);
  }

  close(): void {
    if (this.timer != null) clearInterval(this.timer);
    this.timer = null;
  }

  private tick(): void {
    this.t += 0.04;

    const blocks: BlockView[] = START_BLOCKS.map((b) => ({
      p: b.p,
      m: b.m,
      mass: b.mass,
      energy: energyFor(b.m, this.t),
    }));

    const ship: ShipView = {
      id: 1,
      owner: this.name,
      x: Math.sin(this.t * 0.1) * 5,
      y: 20,
      rot: this.t * 0.05,
      vx: Math.cos(this.t * 0.1) * 0.5,
      vy: 0,
      omega: 0.05,
      com: [0, 0],
      blocks,
    };

    const asteroid: ShipView = {
      id: 2,
      owner: null,
      x: 30,
      y: -10,
      rot: 0.3,
      vx: 0,
      vy: 0,
      omega: 0,
      com: [0, 0],
      blocks: [
        { p: [0, 0], m: "iron", mass: 3000, energy: 100 },
        { p: [1, 0], m: "lead", mass: 6000, energy: 0 },
      ],
    };

    const msg: StateMsg = {
      tick: Math.round(this.t / 0.04),
      ships: [ship, asteroid],
      suns: [{ x: 0, y: 0, radius: 8 }],
      packets: [
        { x: 25 + Math.sin(this.t) * 2, y: -5, vx: 0, vy: 0, m: "lead", mass: 50 },
        { x: 27, y: -12, vx: 0, vy: 0, m: "iron", mass: 20 },
      ],
      rays: [{ x1: ship.x, y1: ship.y + 1.5, x2: ship.x, y2: ship.y + 40, energy: 50000, emitted: 50000, beam: true }],
    };
    this.handlers.onState(msg);
  }
}
