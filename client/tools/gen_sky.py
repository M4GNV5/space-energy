#!/usr/bin/env python3
"""Generates the tileable background textures in client/public/sky/.

    python3 client/tools/gen_sky.py

All noise is synthesised in frequency space, so every texture wraps seamlessly.
Needs numpy, scipy and Pillow.
"""

from pathlib import Path

import numpy as np
from PIL import Image
from scipy.ndimage import map_coordinates

N = 2048
SEED = 7
OUT = Path(__file__).resolve().parent.parent / "public" / "sky"

rng = np.random.default_rng(SEED)


def fbm(beta: float, cutoff: float = 0.0) -> np.ndarray:
    """Periodic fractal noise with a 1/f^beta power spectrum, as a z-score field."""
    fy = np.fft.fftfreq(N)[:, None] * N
    fx = np.fft.rfftfreq(N)[None, :] * N
    f = np.hypot(fx, fy)
    f[0, 0] = 1.0
    amp = f ** (-beta / 2)
    amp[f < cutoff] = 0.0
    amp[0, 0] = 0.0
    spec = amp * (rng.normal(size=amp.shape) + 1j * rng.normal(size=amp.shape))
    field = np.fft.irfft2(spec, s=(N, N))
    return (field - field.mean()) / field.std()


def warp(field: np.ndarray, strength: float) -> np.ndarray:
    """Domain-warps a periodic field by two other periodic fields."""
    yy, xx = np.mgrid[0:N, 0:N].astype(np.float64)
    dx = fbm(3.4) * strength
    dy = fbm(3.4) * strength
    return map_coordinates(field, [yy + dy, xx + dx], order=1, mode="grid-wrap")


def smoothstep(lo: float, hi: float, x: np.ndarray) -> np.ndarray:
    t = np.clip((x - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def splat(img: np.ndarray, x: float, y: float, color: np.ndarray, sigma: float, spike: float = 0.0) -> None:
    """Adds a gaussian star (optionally with diffraction spikes) at a wrapped position."""
    r = int(max(3, sigma * 5, spike))
    ys = np.arange(-r, r + 1)
    xs = np.arange(-r, r + 1)
    dy = ys[:, None] - (y - np.floor(y))
    dx = xs[None, :] - (x - np.floor(x))
    d2 = dx * dx + dy * dy
    k = np.exp(-d2 / (2 * sigma * sigma))
    # Wide faint halo, so bright stars glow instead of looking like hard dots.
    k += 0.12 * np.exp(-d2 / (2 * (sigma * 3.5) ** 2))
    if spike > 0:
        fall = np.exp(-np.sqrt(d2) / (spike * 0.28))
        k += 0.5 * fall * (np.exp(-(dx * dx) / 0.5) + np.exp(-(dy * dy) / 0.5))
    iy = (int(np.floor(y)) + ys) % N
    ix = (int(np.floor(x)) + xs) % N
    img[np.ix_(iy, ix)] += k[:, :, None] * color[None, None, :]


def star_color() -> np.ndarray:
    """Rough black-body tint: mostly white, some blue and orange."""
    t = rng.random()
    if t < 0.25:
        return np.array([0.70, 0.82, 1.00])
    if t < 0.75:
        return np.array([1.00, 0.97, 0.92])
    if t < 0.93:
        return np.array([1.00, 0.84, 0.62])
    return np.array([1.00, 0.66, 0.48])


def nebula() -> np.ndarray:
    density = warp(fbm(3.3), 90.0)
    detail = warp(fbm(2.6, cutoff=6), 40.0)
    # Ridged noise gives the wispy filaments.
    wisps = 1.0 - np.abs(fbm(2.9, cutoff=3))
    wisps = np.clip(wisps, 0, 1) ** 3

    cloud = smoothstep(-0.2, 2.2, density + 0.25 * detail)
    cloud = cloud * (0.55 + 0.45 * wisps)

    hue_a = fbm(4.0)
    hue_b = fbm(4.0)
    blue = np.array([0.10, 0.20, 0.62])
    magenta = np.array([0.56, 0.12, 0.46])
    teal = np.array([0.04, 0.42, 0.46])
    ember = np.array([0.95, 0.42, 0.14])

    w_mag = smoothstep(-0.3, 1.0, hue_a)[..., None]
    w_teal = smoothstep(0.2, 1.5, hue_b)[..., None]
    color = blue * (1 - w_mag) + magenta * w_mag
    color = color * (1 - w_teal) + teal * w_teal

    img = color * cloud[..., None] * 0.55
    # Hot cores where the gas is densest.
    core = smoothstep(1.7, 3.0, density + 0.3 * detail)
    img += ember * (core * 0.35)[..., None]

    # Dark dust lanes in front of the gas.
    dust = smoothstep(0.3, 1.8, warp(fbm(3.0, cutoff=2), 70.0))
    img *= (1.0 - 0.75 * dust)[..., None]

    # Unresolved star dust, thicker along a "milky" band.
    band = smoothstep(-0.5, 1.5, fbm(4.2))
    count = 26000
    xs = rng.random(count * 3) * N
    ys = rng.random(count * 3) * N
    keep = rng.random(count * 3) < 0.2 + 0.8 * band[ys.astype(int), xs.astype(int)]
    for x, y in zip(xs[keep][:count], ys[keep][:count]):
        b = 0.05 + 0.22 * rng.random() ** 3
        splat(img, x, y, star_color() * b, 0.55)

    img += np.array([5, 6, 10]) / 255.0
    return img


def stars() -> np.ndarray:
    img = np.zeros((N, N, 3))
    for _ in range(2600):
        # Power law: lots of faint stars, a handful of bright ones.
        b = rng.pareto(1.6) * 0.09 + 0.12
        b = min(b, 3.0)
        sigma = 0.6 + 0.35 * min(b, 2.0)
        spike = 12.0 * b if b > 2.0 else 0.0
        splat(img, rng.random() * N, rng.random() * N, star_color() * b, sigma, spike)
    return img


def save_jpeg(img: np.ndarray, name: str) -> None:
    # Dither, so the dark gradients do not band.
    img = img + rng.random(img.shape) / 255.0
    data = (np.clip(img, 0, 1) * 255).astype(np.uint8)
    Image.fromarray(data, "RGB").save(OUT / name, quality=88, optimize=True, progressive=True)


def save_png_alpha(img: np.ndarray, name: str) -> None:
    alpha = np.clip(img.max(axis=2), 0, 1)
    rgb = img / np.maximum(img.max(axis=2, keepdims=True), 1e-6)
    data = np.dstack([np.clip(rgb, 0, 1), alpha])
    Image.fromarray((data * 255).astype(np.uint8), "RGBA").save(OUT / name, optimize=True)


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    save_jpeg(nebula(), "nebula.jpg")
    save_png_alpha(stars(), "stars.png")
    for f in sorted(OUT.iterdir()):
        print(f"{f.name}: {f.stat().st_size // 1024} KiB")
