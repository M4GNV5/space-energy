import "./style.css";

import type { Bind } from "./binds";
import { loadBinds } from "./binds";
import type { Camera } from "./camera";
import { Hud } from "./hud";
import { InputController } from "./input";
import { MockNet } from "./mock";
import { Net, type NetHandlers, type NetLike } from "./net";
import { MATERIALS, type ShipId, type StateMsg } from "./protocol";
import { computePoses, render } from "./render";
import { GameState } from "./state";

const app = document.querySelector<HTMLDivElement>("#app")!;

function showLogin(): void {
  app.innerHTML = "";
  const box = document.createElement("div");
  box.className = "login-box";
  box.innerHTML = `
    <h1>Space Energy</h1>
    <input type="text" id="login-name" placeholder="username" maxlength="32" autocomplete="off" />
    <button id="login-btn">Connect</button>
  `;
  app.appendChild(box);
  const nameInput = box.querySelector<HTMLInputElement>("#login-name")!;
  const btn = box.querySelector<HTMLButtonElement>("#login-btn")!;
  const submit = () => {
    const name = nameInput.value.trim();
    if (name.length === 0) return;
    startGame(name);
  };
  btn.addEventListener("click", submit);
  nameInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") submit();
  });
  nameInput.focus();
}

function startGame(name: string): void {
  app.innerHTML = "";

  const canvas = document.createElement("canvas");
  canvas.className = "game-canvas";
  app.appendChild(canvas);
  const ctx = canvas.getContext("2d")!;

  function resize(): void {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.floor(window.innerWidth * dpr);
    canvas.height = Math.floor(window.innerHeight * dpr);
    canvas.style.width = `${window.innerWidth}px`;
    canvas.style.height = `${window.innerHeight}px`;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }
  resize();
  window.addEventListener("resize", resize);

  const game = new GameState();
  const cam: Camera = { x: 0, y: 0, zoom: 10 };
  let controlledShip: ShipId | null = null;
  let binds: Bind[] = loadBinds();

  const hud: Hud = new Hud(
    app,
    binds,
    (b) => {
      binds = b;
    },
    () => input.getSource()?.cell ?? null,
  );

  const useMock = new URLSearchParams(location.search).get("mock") === "1";

  function ensureControlledShip(): void {
    const owned = game.ownedShipIds(name);
    if (controlledShip == null || !owned.includes(controlledShip)) {
      controlledShip = owned.length > 0 ? owned[0]! : null;
    }
  }

  function cycleControlledShip(): void {
    const owned = game.ownedShipIds(name);
    if (owned.length === 0) {
      controlledShip = null;
      return;
    }
    const idx = controlledShip == null ? -1 : owned.indexOf(controlledShip);
    controlledShip = owned[(idx + 1) % owned.length]!;
  }

  const netHandlers: NetHandlers = {
    onWelcome: (_player, _ships) => {
      hud.setConnectionLost(false, () => {});
    },
    onState: (msg: StateMsg) => {
      game.apply(msg, performance.now());
      ensureControlledShip();
    },
    onError: (msg) => hud.toast(msg),
    onClose: () => {
      hud.setConnectionLost(true, () => net.connect(name));
    },
  };

  const net: NetLike = useMock ? new MockNet(netHandlers) : new Net(netHandlers);

  const input: InputController = new InputController(canvas, cam, net, hud, {
    getGame: () => game,
    getMyName: () => name,
    getControlledShip: () => controlledShip,
    cycleControlledShip,
    getBinds: () => binds,
  });

  net.connect(name);

  function updateHud(): void {
    const owned = game.ownedShipIds(name);
    const follow = input.follow;
    if (owned.length === 0) {
      hud.setInfo({
        player: name,
        controlledShip: null,
        speed: 0,
        mass: 0,
        avgFill: 0,
        maxFill: 0,
        spectator: true,
        follow,
      });
      return;
    }
    const ship = controlledShip != null ? game.find(controlledShip) : undefined;
    if (!ship) {
      hud.setInfo({
        player: name,
        controlledShip: null,
        speed: 0,
        mass: 0,
        avgFill: 0,
        maxFill: 0,
        spectator: false,
        follow,
      });
      return;
    }
    const speed = Math.hypot(ship.vx, ship.vy);
    const mass = ship.blocks.reduce((s, b) => s + b.mass, 0);
    const fills = ship.blocks.map((b) => b.energy / MATERIALS[b.m].energyCapacity);
    const avgFill = fills.length > 0 ? fills.reduce((a, b) => a + b, 0) / fills.length : 0;
    const maxFill = fills.length > 0 ? Math.max(...fills) : 0;
    hud.setInfo({
      player: name,
      controlledShip: ship.id,
      speed,
      mass,
      avgFill,
      maxFill,
      spectator: false,
      follow,
    });
  }

  function frame(now: number): void {
    game.pruneRays(now);

    const poses = computePoses(game, now);
    if (input.follow && controlledShip != null) {
      const pose = poses.get(controlledShip);
      if (pose) {
        cam.x = pose.x;
        cam.y = pose.y;
      }
    }
    input.maybeSendView(now);

    render(ctx, canvas.clientWidth, canvas.clientHeight, cam, game, poses, now, {
      ownerName: name,
      controlledShip,
      hover: input.getHover(),
      source: input.getSource(),
      buildTargetCell: input.getBuildTargetCell(),
    });

    updateHud();
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

const autoName = new URLSearchParams(location.search).get("name")?.trim();
if (autoName) startGame(autoName);
else showLogin();
