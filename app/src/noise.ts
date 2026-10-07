// Exact port of core/src/noise.rs + rng.rs (permutation, gradient noise, fBm),
// so scatter-brush previews match what the core computes on release.

import type { Vec3 } from './grid';

const M64 = (1n << 64n) - 1n;

function mix64(z: bigint): bigint {
  z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & M64;
  z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & M64;
  return z ^ (z >> 31n);
}

class Rng {
  private state: bigint;
  constructor(seed: bigint, stream: bigint) {
    this.state = mix64(seed ^ mix64((stream + 0x632be59bd9b4e019n) & M64));
  }
  next(): bigint {
    this.state = (this.state + 0x9e3779b97f4a7c15n) & M64;
    return mix64(this.state);
  }
  below(n: number): number {
    return Number((this.next() >> 1n) % BigInt(Math.max(1, n)));
  }
}

const GRAD = [
  [1, 1, 0], [-1, 1, 0], [1, -1, 0], [-1, -1, 0], [1, 0, 1], [-1, 0, 1],
  [1, 0, -1], [-1, 0, -1], [0, 1, 1], [0, -1, 1], [0, 1, -1], [0, -1, -1],
];

const fade = (t: number) => t * t * t * (t * (t * 6 - 15) + 10);
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

export class Noise {
  private perm = new Uint8Array(512);

  constructor(seed: number | bigint, stream: number | bigint) {
    const rng = new Rng(BigInt(seed) & M64, BigInt(stream));
    const p = new Uint8Array(256).map((_, i) => i);
    for (let i = 255; i >= 1; i--) {
      const j = rng.below(i + 1);
      [p[i], p[j]] = [p[j], p[i]];
    }
    for (let i = 0; i < 512; i++) this.perm[i] = p[i & 255];
  }

  private grad(h: number, x: number, y: number, z: number) {
    const g = GRAD[h % 12];
    return g[0] * x + g[1] * y + g[2] * z;
  }

  sample(px: number, py: number, pz: number): number {
    const fx = Math.floor(px), fy = Math.floor(py), fz = Math.floor(pz);
    const xi = ((fx % 256) + 256) % 256, yi = ((fy % 256) + 256) % 256, zi = ((fz % 256) + 256) % 256;
    const x = px - fx, y = py - fy, z = pz - fz;
    const u = fade(x), v = fade(y), w = fade(z);
    const pm = this.perm;
    const a = pm[xi] + yi, aa = pm[a] + zi, ab = pm[a + 1] + zi;
    const b = pm[xi + 1] + yi, ba = pm[b] + zi, bb = pm[b + 1] + zi;
    const r = lerp(
      lerp(lerp(this.grad(pm[aa], x, y, z), this.grad(pm[ba], x - 1, y, z), u),
        lerp(this.grad(pm[ab], x, y - 1, z), this.grad(pm[bb], x - 1, y - 1, z), u), v),
      lerp(lerp(this.grad(pm[aa + 1], x, y, z - 1), this.grad(pm[ba + 1], x - 1, y, z - 1), u),
        lerp(this.grad(pm[ab + 1], x, y - 1, z - 1), this.grad(pm[bb + 1], x - 1, y - 1, z - 1), u), v),
      w);
    return r * 1.1;
  }

  fbm(p: Vec3, freq: number, octaves: number): number {
    let sum = 0, amp = 1, norm = 0, f = freq;
    for (let o = 0; o < octaves; o++) {
      sum += amp * this.sample(p[0] * f + o * 17.13, p[1] * f + o * 3.71, p[2] * f + o * 11.37);
      norm += amp;
      amp *= 0.5;
      f *= 2.03;
    }
    return sum / norm;
  }
}

export const SCATTER_STREAM = 9;

/** Mirror of edits::scatter_octaves. */
export function scatterOctaves(grainKm: number, spacingKm: number): number {
  const ratio = Math.max(10, grainKm) / (2.5 * Math.max(1e-6, spacingKm));
  return Math.min(5, Math.max(1, Math.floor(Math.log2(ratio)) + 1));
}

/** Mirror of edits::scatter_weight. */
export function scatterWeight(scatter: number, hardness: number, strength: number, dOverR: number, noise: number): number {
  const s = Math.min(1, Math.max(0, scatter));
  const v = 1 - dOverR + s * 1.6 * noise;
  const soft = 0.04 + 0.3 * (1 - Math.min(1, Math.max(0, hardness)));
  const t = Math.min(1, Math.max(0, v / soft));
  return t * t * (3 - 2 * t) * Math.min(1, Math.max(0, strength));
}
