// Parallax sky behind the map: tileable textures from tools/gen_sky.py, drawn
// as repeating patterns. Far layers move and zoom much less than the world.

import { type Camera, ZOOM_MIN } from "./camera";

interface Layer {
  src: string;
  /** Texture px moved per world metre the camera moves. */
  parallax: number;
  /** Texture scale at minimum zoom. */
  scale: number;
  /** How strongly the layer follows the camera zoom (0 = not at all, 1 = like the world). */
  zoomExp: number;
  alpha: number;
  image: HTMLImageElement;
  pattern: CanvasPattern | null;
}

function layer(src: string, parallax: number, scale: number, zoomExp: number, alpha: number): Layer {
  const image = new Image();
  image.src = src;
  return { src, parallax, scale, zoomExp, alpha, image, pattern: null };
}

// Back to front. The star texture is used twice, at different depths.
const LAYERS: Layer[] = [
  layer("/sky/nebula.jpg", 0.01, 1, 0.075, 1),
  layer("/sky/stars.png", 0.02, 0.7, 0.085, 0.7),
  layer("/sky/stars.png", 0.05, 1.15, 0.1, 1),
];

export function drawSky(ctx: CanvasRenderingContext2D, cam: Camera, width: number, height: number): void {
  const cx = width / 2;
  const cy = height / 2;
  for (const l of LAYERS) {
    if (!l.image.complete || l.image.naturalWidth === 0) continue;
    l.pattern ??= ctx.createPattern(l.image, "repeat");
    if (!l.pattern) continue;

    const s = l.scale * Math.pow(cam.zoom / ZOOM_MIN, l.zoomExp);
    // Wrap in texture space, so the offset stays small however far the camera is.
    const tile = l.image.naturalWidth;
    const ox = (((cam.x * l.parallax) % tile) + tile) % tile;
    const oy = (((-cam.y * l.parallax) % tile) + tile) % tile;
    // Zooms about the screen centre.
    l.pattern.setTransform(new DOMMatrix([s, 0, 0, s, cx - ox * s, cy - oy * s]));

    ctx.globalAlpha = l.alpha;
    ctx.fillStyle = l.pattern;
    ctx.fillRect(0, 0, width, height);
  }
  ctx.globalAlpha = 1;
}
