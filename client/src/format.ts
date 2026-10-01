// SI-prefix formatting for the hover tooltip / HUD.

const ENERGY_UNITS: Array<[number, string]> = [
  [1e9, "GJ"],
  [1e6, "MJ"],
  [1e3, "kJ"],
  [1, "J"],
];

export function formatEnergy(joules: number): string {
  const v = Math.max(0, joules);
  for (const [f, suffix] of ENERGY_UNITS) {
    if (v >= f) return `${roundSig(v / f, 3)} ${suffix}`;
  }
  return `0 J`;
}

export function formatMass(kg: number): string {
  return `${Math.round(kg)}`;
}

export function escapeHtml(s: string): string {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

function roundSig(v: number, sig: number): string {
  if (v === 0) return "0";
  const digits = sig - Math.ceil(Math.log10(Math.abs(v)));
  const factor = Math.pow(10, digits);
  const rounded = Math.round(v * factor) / factor;
  return rounded.toString();
}
