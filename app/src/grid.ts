// Client-side copy of the geodesic grid: positions, triangles, neighbours,
// plus nearest-cell lookup (greedy walk) for picking and brush previews.

export type Vec3 = [number, number, number];

const TW = 96;
const TH = 48;

export class Grid {
  n: number;
  t: number;
  pos: Float32Array;
  tris: Uint32Array;
  off: Uint32Array;
  nbr: Uint32Array;
  lat: Float32Array;
  lon: Float32Array;
  spacing: number; // radians
  private table: Uint32Array;

  constructor(buf: ArrayBuffer) {
    const dv = new DataView(buf);
    const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
    if (magic !== 'GRD1') throw new Error('bad grid data');
    this.n = dv.getUint32(4, true);
    this.t = dv.getUint32(8, true);
    const nn = dv.getUint32(12, true);
    let o = 16;
    this.pos = new Float32Array(buf, o, this.n * 3); o += this.n * 12;
    this.tris = new Uint32Array(buf, o, this.t * 3); o += this.t * 12;
    this.off = new Uint32Array(buf, o, this.n + 1); o += (this.n + 1) * 4;
    this.nbr = new Uint32Array(buf, o, nn);
    this.lat = new Float32Array(this.n);
    this.lon = new Float32Array(this.n);
    for (let i = 0; i < this.n; i++) {
      const x = this.pos[i * 3], y = this.pos[i * 3 + 1], z = this.pos[i * 3 + 2];
      this.lat[i] = Math.asin(Math.max(-1, Math.min(1, y)));
      this.lon[i] = Math.atan2(x, z);
    }
    let s = 0, c = 0;
    for (let i = 0; i < this.n; i += Math.max(1, Math.floor(this.n / 3000))) {
      for (let k = this.off[i]; k < this.off[i + 1]; k++) {
        s += angle(this.p(i), this.p(this.nbr[k]));
        c++;
      }
    }
    this.spacing = s / c;
    this.table = new Uint32Array(TW * TH);
    let cur = 0;
    for (let y = 0; y < TH; y++) {
      for (let x = 0; x < TW; x++) {
        const la = -Math.PI / 2 + ((y + 0.5) / TH) * Math.PI;
        const lo = -Math.PI + ((x + 0.5) / TW) * 2 * Math.PI;
        cur = this.walk(fromLatLon(la, lo), cur);
        this.table[y * TW + x] = cur;
      }
    }
  }

  p(i: number): Vec3 {
    return [this.pos[i * 3], this.pos[i * 3 + 1], this.pos[i * 3 + 2]];
  }

  neighbors(i: number): Uint32Array {
    return this.nbr.subarray(this.off[i], this.off[i + 1]);
  }

  private walk(p: Vec3, start: number): number {
    let cur = start;
    let best = this.dot(cur, p);
    for (;;) {
      let next = cur;
      for (let k = this.off[cur]; k < this.off[cur + 1]; k++) {
        const j = this.nbr[k];
        const d = this.dot(j, p);
        if (d > best) { best = d; next = j; }
      }
      if (next === cur) return cur;
      cur = next;
    }
  }

  private dot(i: number, p: Vec3): number {
    return this.pos[i * 3] * p[0] + this.pos[i * 3 + 1] * p[1] + this.pos[i * 3 + 2] * p[2];
  }

  nearest(p: Vec3, hint?: number): number {
    let start = hint;
    if (start === undefined) {
      const la = Math.asin(Math.max(-1, Math.min(1, p[1])));
      const lo = Math.atan2(p[0], p[2]);
      const y = Math.min(TH - 1, Math.max(0, Math.floor(((la + Math.PI / 2) / Math.PI) * TH)));
      const x = Math.min(TW - 1, Math.max(0, Math.floor(((lo + Math.PI) / (2 * Math.PI)) * TW)));
      start = this.table[y * TW + x];
    }
    return this.walk(p, start);
  }

  /** All cells within `radius` (radians) of p, with their angular distance. */
  within(p: Vec3, radius: number, cb: (cell: number, d: number) => void) {
    const start = this.nearest(p);
    const cosR = Math.cos(radius + this.spacing * 0.5);
    const seen = new Set<number>([start]);
    const stack = [start];
    while (stack.length) {
      const c = stack.pop()!;
      const d = angle(this.p(c), p);
      if (d <= radius) cb(c, d);
      for (let k = this.off[c]; k < this.off[c + 1]; k++) {
        const j = this.nbr[k];
        if (!seen.has(j) && this.dot(j, p) >= cosR) {
          seen.add(j);
          stack.push(j);
        }
      }
    }
  }
}

export function fromLatLon(lat: number, lon: number): Vec3 {
  const c = Math.cos(lat);
  return [c * Math.sin(lon), Math.sin(lat), c * Math.cos(lon)];
}

export function toLatLon(p: Vec3): [number, number] {
  return [Math.asin(Math.max(-1, Math.min(1, p[1]))), Math.atan2(p[0], p[2])];
}

export function angle(a: Vec3, b: Vec3): number {
  const cx = a[1] * b[2] - a[2] * b[1];
  const cy = a[2] * b[0] - a[0] * b[2];
  const cz = a[0] * b[1] - a[1] * b[0];
  return Math.atan2(Math.hypot(cx, cy, cz), a[0] * b[0] + a[1] * b[1] + a[2] * b[2]);
}

export function eastNorth(p: Vec3): [Vec3, Vec3] {
  let e: Vec3 = [p[2], 0, -p[0]]; // up × p with up = +y
  const l = Math.hypot(e[0], e[1], e[2]);
  e = l < 1e-9 ? [1, 0, 0] : [e[0] / l, e[1] / l, e[2] / l];
  const n: Vec3 = [p[1] * e[2] - p[2] * e[1], p[2] * e[0] - p[0] * e[2], p[0] * e[1] - p[1] * e[0]];
  return [e, n];
}
