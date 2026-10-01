// Example script shown in the editor for a ship without a saved script.
// Cells refer to the starter ship: `STARTER_LAYOUT` in server/src/sim/worldgen.rs.

export const DEFAULT_SCRIPT = `// setup() runs once when the script starts, loop() once per server tick (25/s).
// Globals: ship, world, menu, log, toast. See "API" below the editor.

// Thrusters of the starter ship: [block, face the mass leaves through].
// The ship is pushed the opposite way.
const FORWARD = [[-2, -3], [-1, -3], [0, -3], [1, -3], [2, -3]].map((b) => [b, "s"]);
const BACK = [[[-2, 4], "n"], [[2, 4], "n"]];
const LEFT = [[[2, 4], "e"], [[2, -3], "e"]];
const RIGHT = [[[-2, 4], "w"], [[-2, -3], "w"]];
const TURN_LEFT = [[[2, 4], "e"], [[-2, -3], "w"]];
const TURN_RIGHT = [[[-2, 4], "w"], [[2, -3], "e"]];

const ENERGY = 2000; // J per emit, sets the exhaust speed
const IMPULSE_PER_KG = ENERGY * 0.5; // kg m/s per kg of exhaust (server: EXHAUST_SPEED_PER_J)
const MAX_KG = 2; // lead emits at most 2 kg per tick

let stabilizing = false;
let autoCollect = false;

function setup() {
  const button = menu.button("stabilize", () => {
    stabilizing = !stabilizing;
    button.setActive(stabilizing);
  });
  const collect = menu.button("auto collect: off", () => {
    autoCollect = !autoCollect;
    collect.setActive(autoCollect);
    collect.setLabel("auto collect: " + (autoCollect ? "on" : "off"));
  });
  menu.button("vent selected", () => {
    const b = ship.selectedBlock;
    if (!b) return toast("click a block of this ship first");
    ship.emit(b.p, "all", b.energy);
  });
}

function loop() {
  if (stabilizing) stabilize();
  if (autoCollect && packetsInReach()) ship.collect();
}

// Packets can be collected within 10 m of any block (server: COLLECT_RANGE).
function packetsInReach() {
  return world.packets.some((p) => {
    const [x, y] = ship.toLocal(p.x - ship.x, p.y - ship.y);
    const px = x + ship.com[0], py = y + ship.com[1];
    return ship.blocks.some((b) => Math.hypot(b.p[0] - px, b.p[1] - py) <= 10);
  });
}

function fire(thrusters, kg) {
  for (const [block, dir] of thrusters) ship.emit(block, dir, ENERGY, Math.min(kg, MAX_KG));
}

// Thrust against the current motion and spin until the ship is at rest.
function stabilize() {
  // Velocity in the ship's own frame: +x = right, +y = towards the nose.
  const [vx, vy] = ship.toLocal(ship.vx, ship.vy);
  // Half of the exhaust needed to stop within one tick, to avoid overshooting.
  const kgFor = (v) => (0.5 * ship.mass * Math.abs(v)) / IMPULSE_PER_KG;

  if (vy > 0.02) fire(BACK, kgFor(vy) / BACK.length);
  if (vy < -0.02) fire(FORWARD, kgFor(vy) / FORWARD.length);
  if (vx > 0.02) fire(LEFT, kgFor(vx) / LEFT.length);
  if (vx < -0.02) fire(RIGHT, kgFor(vx) / RIGHT.length);

  // omega > 0 = turning counter-clockwise (left).
  const spinKg = 50 * Math.abs(ship.omega);
  if (ship.omega > 0.002) fire(TURN_RIGHT, spinKg);
  if (ship.omega < -0.002) fire(TURN_LEFT, spinKg);
}
`;

/** Shown below the editor. */
export const API_REFERENCE = `function setup()   runs once when the script starts
function loop()    runs once per server tick (world.dt seconds)

ship   (your ship; ship-local grid: +x = e, +y = n = nose)
  .id .x .y .rot .vx .vy .omega .com .mass     world frame, m / rad / kg
  .blocks        [{ p: [x, y], m, mass, energy, capacity, fill, dir? }]
  .block(x, y)   block at a cell, or undefined
  .selectedBlock the block of this ship the player clicked, or undefined
  .toLocal(x, y) / .toWorld(x, y)   rotate a vector between world and ship frame
  .emit(cell, dir, energy = 0, mass = 0)   dir: "n" "e" "s" "w" "all"
  .moveMass(fromCell, toCell, kg)
  .collect(cell?, material?)

world  (everything the camera sees)
  .tick .dt
  .ships         other ships and asteroids (owner null), same fields as ship
  .suns          [{ x, y, radius }]
  .packets       [{ x, y, vx, vy, m, mass }]
  .rays          [{ x1, y1, x2, y2, energy, emitted, beam }]

menu
  .button(label, onClick)   returns { setLabel(text), setActive(on), remove() }
  .clear()

log(...values)     prints to the panel below
toast(...values)   shows a popup in the top right`;
