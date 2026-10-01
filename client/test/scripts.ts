// Sanity checks for the script runtime, driven without a worker or a server.
// Run with: npx tsx client/test/scripts.ts

import assert from "node:assert/strict";
import { DEFAULT_SCRIPT } from "../src/defaultScript.ts";
import type { ShipView, StateMsg } from "../src/protocol.ts";
import { MAX_CMDS_PER_TICK, ScriptRuntime } from "../src/scriptRuntime.ts";

let passed = 0;
function check(name: string, fn: () => void): void {
  fn();
  passed++;
  console.log(`ok - ${name}`);
}

function state(over: Partial<ShipView> = {}, tick = 1): StateMsg {
  const ship: ShipView = {
    id: 7,
    owner: "p",
    x: 0,
    y: 0,
    rot: 0,
    vx: 0,
    vy: 0,
    omega: 0,
    com: [0, 0],
    blocks: [{ p: [0, 0], m: "lead", mass: 4000, energy: 500000 }],
    ...over,
  };
  const rock: ShipView = { ...ship, id: 8, owner: null, x: 50, blocks: [{ p: [0, 0], m: "iron", mass: 100, energy: 0 }] };
  return { tick, ships: [ship, rock], suns: [{ x: 0, y: 500, radius: 100 }], packets: [], rays: [] };
}

check("setup runs once, loop every tick, commands carry the ship id", () => {
  const rt = new ScriptRuntime(7);
  const src = `
    let n = 0;
    function setup() { log("hi", ship.id, world.ships.length, world.suns[0].radius); }
    function loop() { n++; ship.emit([0, 0], "s", 10 * n, 1); ship.collect(); ship.moveMass([0, 0], [0, 1], 5); }
  `;
  const started = rt.start(src, state());
  assert.equal(started.error, null);
  assert.deepEqual(started.logs, ["hi 7 1 100"]);
  assert.deepEqual(started.cmds, []);
  rt.tick(state({}, 2));
  const res = rt.tick(state({}, 3));
  assert.deepEqual(res.cmds, [
    { t: "emit", ship: 7, block: [0, 0], dir: "s", energy: 20, mass: 1 },
    { t: "collect", ship: 7 },
    { t: "move_mass", ship: 7, from: [0, 0], to: [0, 1], kg: 5 },
  ]);
});

check("blocks get capacity and fill, ship gets mass", () => {
  const rt = new ScriptRuntime(7);
  const res = rt.start(
    `function setup() { const b = ship.block(0, 0); log(b.capacity, b.fill, ship.mass, ship.block(5, 5)); } function loop() {}`,
    state(),
  );
  assert.deepEqual(res.logs, ["1000000 0.500 4000 undefined"]);
});

check("missing functions, syntax errors and exceptions are reported", () => {
  assert.match(new ScriptRuntime(7).start(`function setup() {}`, state()).error!, /setup\(\).*loop\(\)/);
  assert.match(new ScriptRuntime(7).start(`function setup( {`, state()).error!, /SyntaxError/);
  const rt = new ScriptRuntime(7);
  assert.equal(rt.start(`function setup() {} function loop() { ship.emit([0, 0], "up"); }`, state()).error, null);
  assert.match(rt.tick(state()).error!, /loop: TypeError: dir must be/);
  // Dead afterwards.
  assert.deepEqual(rt.tick(state()), { cmds: [], buttons: null, logs: [], toasts: [], error: null });
});

check("menu buttons: created, clicked, updated, removed", () => {
  const rt = new ScriptRuntime(7);
  const started = rt.start(
    `let on = false, b;
     function setup() {
       b = menu.button("stabilize", () => { on = !on; b.setActive(on); ship.collect(); });
       const gone = menu.button("gone", () => {});
       gone.remove();
     }
     function loop() {}`,
    state(),
  );
  assert.deepEqual(started.buttons, [{ id: 1, label: "stabilize", active: false }]);
  assert.equal(rt.tick(state()).buttons, null, "unchanged menu is not resent");
  const clicked = rt.click(1);
  assert.deepEqual(clicked.buttons, [{ id: 1, label: "stabilize", active: true }]);
  assert.deepEqual(clicked.cmds, [{ t: "collect", ship: 7 }]);
  assert.deepEqual(rt.click(99).cmds, []);
});

check("loop is skipped while the ship is out of view", () => {
  const rt = new ScriptRuntime(7);
  rt.start(`function setup() {} function loop() { ship.collect(); }`, state());
  const s = state();
  s.ships = s.ships.filter((sh) => sh.id !== 7);
  assert.deepEqual(rt.tick(s).cmds, []);
  assert.equal(rt.tick(state()).cmds.length, 1);
});

check("commands per tick are capped", () => {
  const rt = new ScriptRuntime(7);
  rt.start(`function setup() {} function loop() { for (let i = 0; i < 1000; i++) ship.collect(); }`, state());
  assert.equal(rt.tick(state()).cmds.length, MAX_CMDS_PER_TICK);
});

check("selectedBlock follows the player's selection, toast is returned", () => {
  const rt = new ScriptRuntime(7);
  const src = `function setup() { menu.button("sel", () => toast("clicked", ship.selectedBlock?.m)); }
    function loop() { const b = ship.selectedBlock; toast(b ? b.m + " " + b.fill : "none"); }`;
  assert.deepEqual(rt.start(src, state()).toasts, []);
  assert.deepEqual(rt.tick(state()).toasts, ["none"]);
  assert.deepEqual(rt.tick(state(), [0, 0]).toasts, ["lead 0.5"]);
  assert.deepEqual(rt.tick(state(), [3, 3]).toasts, ["none"], "selected cell without a block");
  assert.deepEqual(rt.click(1, [0, 0]).toasts, ["clicked lead"]);
});

check("default script: vent selected", () => {
  const rt = new ScriptRuntime(7);
  rt.start(DEFAULT_SCRIPT, state());
  const none = rt.click(3);
  assert.deepEqual([none.cmds, none.toasts], [[], ["click a block of this ship first"]]);
  assert.deepEqual(rt.click(3, [0, 0]).cmds, [{ t: "emit", ship: 7, block: [0, 0], dir: "all", energy: 500000, mass: 0 }]);
});

check("default script: auto collect only fires with a packet in reach", () => {
  const rt = new ScriptRuntime(7);
  rt.start(DEFAULT_SCRIPT, state());
  const withPacket = (x: number): StateMsg => ({ ...state(), packets: [{ x, y: 0, vx: 0, vy: 0, m: "iron", mass: 5 }] });
  assert.deepEqual(rt.tick(withPacket(5)).cmds, [], "off by default");
  assert.deepEqual(rt.click(2).buttons![1], { id: 2, label: "auto collect: on", active: true });
  assert.deepEqual(rt.tick(withPacket(5)).cmds, [{ t: "collect", ship: 7 }]);
  assert.deepEqual(rt.tick(withPacket(11)).cmds, [], "out of range");
  assert.deepEqual(rt.tick(state()).cmds, [], "no packets");
  assert.equal(rt.click(2).buttons![1]!.label, "auto collect: off");
});

check("default script: stabilize thrusts against motion and spin", () => {
  const rt = new ScriptRuntime(7);
  const started = rt.start(DEFAULT_SCRIPT, state());
  assert.equal(started.error, null);
  assert.deepEqual(started.buttons!.map((b) => b.label), ["stabilize", "auto collect: off", "vent selected"]);
  const moving = { vx: -3, vy: 0, rot: Math.PI / 2, omega: 0.1 }; // nose points -x: flying forward, turning left
  assert.deepEqual(rt.tick(state(moving)).cmds, [], "idle until the button is clicked");
  assert.equal(rt.click(1).buttons![0]!.active, true);
  const res = rt.tick(state(moving));
  assert.equal(res.error, null);
  const emits = res.cmds.map((c) => (c.t === "emit" ? `${c.block}:${c.dir}` : c.t));
  assert.deepEqual(emits, ["-2,4:n", "2,4:n", "-2,4:w", "2,-3:e"]);
  assert.deepEqual(rt.tick(state()).cmds, [], "nothing to do at rest");
});

console.log(`\n${passed} checks passed`);
