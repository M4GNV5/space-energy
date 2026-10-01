// Plain-DOM UI for ship scripts: the script menu (buttons the script created)
// in the HUD column and the script editor panel.

import { API_REFERENCE, DEFAULT_SCRIPT } from "./defaultScript";
import type { ShipId } from "./protocol";
import type { ScriptManager } from "./scripts";

export class ScriptPanel {
  private menu: HTMLDivElement;
  private panel: HTMLDivElement;
  private title: HTMLDivElement;
  private editor: HTMLTextAreaElement;
  private status: HTMLDivElement;
  private log: HTMLPreElement;

  private ship: ShipId | null = null;
  private dirty = true;

  constructor(
    parent: HTMLElement,
    hudRoot: HTMLElement,
    private scripts: ScriptManager,
  ) {
    this.menu = document.createElement("div");
    this.menu.className = "hud-panel script-menu";
    hudRoot.appendChild(this.menu);

    this.panel = document.createElement("div");
    this.panel.className = "script-panel hidden";
    parent.appendChild(this.panel);

    this.title = document.createElement("div");
    this.title.className = "bind-title";
    this.panel.appendChild(this.title);

    this.editor = document.createElement("textarea");
    this.editor.className = "script-editor";
    this.editor.spellcheck = false;
    this.editor.addEventListener("input", () => {
      if (this.ship != null) this.scripts.saveDraft(this.ship, this.editor.value);
    });
    this.editor.addEventListener("keydown", (e) => this.onEditorKey(e));
    this.panel.appendChild(this.editor);

    const buttons = document.createElement("div");
    buttons.className = "script-buttons";
    buttons.appendChild(button("run (Ctrl+Enter)", () => this.run()));
    buttons.appendChild(
      button("stop", () => {
        if (this.ship != null) this.scripts.stop(this.ship);
      }),
    );
    buttons.appendChild(
      button("load example", () => {
        this.editor.value = DEFAULT_SCRIPT;
        if (this.ship != null) this.scripts.saveDraft(this.ship, DEFAULT_SCRIPT);
      }),
    );
    this.panel.appendChild(buttons);

    this.status = document.createElement("div");
    this.status.className = "script-status";
    this.panel.appendChild(this.status);

    this.log = document.createElement("pre");
    this.log.className = "script-log";
    this.panel.appendChild(this.log);

    const api = document.createElement("details");
    api.innerHTML = `<summary>API</summary>`;
    const apiText = document.createElement("pre");
    apiText.className = "script-api";
    apiText.textContent = API_REFERENCE;
    api.appendChild(apiText);
    this.panel.appendChild(api);
  }

  toggle(): void {
    this.panel.classList.toggle("hidden");
    if (this.isOpen()) this.editor.focus();
  }

  close(): void {
    this.panel.classList.add("hidden");
  }

  isOpen(): boolean {
    return !this.panel.classList.contains("hidden");
  }

  /** Tell the panel that `ship`'s script status, menu or log changed. */
  markDirty(ship: ShipId): void {
    if (ship === this.ship) this.dirty = true;
  }

  /** Call once per frame with the controlled ship. */
  update(ship: ShipId | null): void {
    if (ship !== this.ship) {
      this.ship = ship;
      this.editor.value = ship != null ? this.scripts.source(ship) : "";
      this.dirty = true;
    }
    if (!this.dirty) return;
    this.dirty = false;
    this.render();
  }

  private run(): void {
    if (this.ship != null) this.scripts.run(this.ship, this.editor.value);
  }

  private onEditorKey(e: KeyboardEvent): void {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      this.run();
    } else if (e.key === "Tab") {
      e.preventDefault();
      this.editor.setRangeText("  ", this.editor.selectionStart, this.editor.selectionEnd, "end");
      if (this.ship != null) this.scripts.saveDraft(this.ship, this.editor.value);
    } else if (e.key === "Escape") {
      this.editor.blur();
    }
  }

  private render(): void {
    const ship = this.ship;
    this.title.textContent = ship != null ? `Script for ship #${ship} (R to close)` : "Script (no ship)";
    this.editor.disabled = ship == null;
    this.menu.innerHTML = "";
    if (ship == null) {
      this.status.textContent = "";
      this.log.textContent = "";
      this.menu.classList.add("hidden");
      return;
    }
    this.menu.classList.remove("hidden");
    const st = this.scripts.status(ship);

    this.status.className = `script-status ${st.state}`;
    this.status.textContent = st.state === "error" ? `error: ${st.error}` : st.state;
    const atBottom = this.log.scrollTop + this.log.clientHeight >= this.log.scrollHeight - 4;
    this.log.textContent = st.logs.join("\n");
    if (atBottom) this.log.scrollTop = this.log.scrollHeight;

    const head = document.createElement("div");
    head.className = `script-status ${st.state}`;
    head.textContent = `script: ${st.state} (R)`;
    if (st.error) head.title = st.error;
    this.menu.appendChild(head);
    for (const b of st.buttons) {
      const el = button(b.label, () => this.scripts.click(ship, b.id));
      if (b.active) el.classList.add("active");
      this.menu.appendChild(el);
    }
  }
}

function button(text: string, onClick: () => void): HTMLButtonElement {
  const el = document.createElement("button");
  el.textContent = text;
  el.addEventListener("click", () => {
    onClick();
    // Keep keyboard focus off the button so space/enter key binds don't click it again.
    el.blur();
  });
  return el;
}
