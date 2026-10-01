// Web worker hosting one ship's script, so a script that hangs can be killed
// without freezing the game. Every request gets exactly one `ScriptResult` back.

import type { Cell, ShipId, StateMsg } from "./protocol";
import { ScriptRuntime, type ScriptResult } from "./scriptRuntime";

export type WorkerRequest =
  | { t: "start"; ship: ShipId; src: string; state: StateMsg; selected: Cell | null }
  | { t: "tick"; state: StateMsg; selected: Cell | null }
  | { t: "click"; id: number; selected: Cell | null };

const scope = self as unknown as {
  onmessage: ((e: MessageEvent<WorkerRequest>) => void) | null;
  postMessage(msg: ScriptResult): void;
};

let runtime: ScriptRuntime | null = null;

scope.onmessage = (e) => {
  const req = e.data;
  if (req.t === "start") {
    runtime = new ScriptRuntime(req.ship);
    scope.postMessage(runtime.start(req.src, req.state, req.selected));
  } else if (runtime) {
    scope.postMessage(req.t === "tick" ? runtime.tick(req.state, req.selected) : runtime.click(req.id, req.selected));
  }
};
