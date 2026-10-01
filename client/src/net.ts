// Websocket transport + message dispatch. `mock.ts` implements the same
// `NetLike` interface so the app can run without a server.

import type { ClientMsg, ServerMsg, ShipId, StateMsg } from "./protocol";

export interface NetHandlers {
  onWelcome: (player: string, ships: ShipId[]) => void;
  onState: (msg: StateMsg) => void;
  onError: (msg: string) => void;
  onClose: () => void;
  onOpen?: () => void;
}

export interface NetLike {
  connect(name: string): void;
  send(msg: ClientMsg): void;
  close(): void;
}

export class Net implements NetLike {
  private ws: WebSocket | null = null;

  constructor(private handlers: NetHandlers) {}

  connect(name: string): void {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${proto}://${location.host}/ws`);
    this.ws = ws;
    ws.onopen = () => {
      this.send({ t: "login", name });
      this.handlers.onOpen?.();
    };
    ws.onmessage = (e) => {
      let msg: ServerMsg;
      try {
        msg = JSON.parse(e.data as string) as ServerMsg;
      } catch {
        return;
      }
      switch (msg.t) {
        case "welcome":
          this.handlers.onWelcome(msg.player, msg.ships);
          break;
        case "state":
          this.handlers.onState(msg);
          break;
        case "error":
          this.handlers.onError(msg.msg);
          break;
      }
    };
    ws.onclose = () => this.handlers.onClose();
  }

  send(msg: ClientMsg): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    }
  }

  close(): void {
    this.ws?.close();
    this.ws = null;
  }
}
