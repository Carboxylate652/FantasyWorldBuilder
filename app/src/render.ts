// WebGL2 renderer for the globe and the flat (equirectangular) map.
//
// The mesh has no vertex buffers: each vertex fetches its triangle's three cell
// ids from a texture, then the cell position. The fragment shader picks the
// colour of the nearest corner (crisp hexagonal cells) or blends the three
// (smooth fields). On the flat map, triangles are unwrapped across the
// antimeridian and drawn three times (shifted by ±360°).

import type { Grid, Vec3 } from './grid';
import { toLatLon } from './grid';

const TEX_W = 2048;

const MESH_VS = `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;
uniform usampler2D uTris;
uniform highp sampler2D uPos;
uniform highp sampler2D uElev;
uniform mat4 uMVP;
uniform int uMode;
uniform float uExag;
uniform vec4 uFlat; // scale.xy, offset.xy
flat out uvec3 vIds;
out vec3 vBary;
out vec3 vNormal;
const float PI = 3.141592653589793;
ivec2 tc(uint i) { return ivec2(int(i % ${TEX_W}u), int(i / ${TEX_W}u)); }
void main() {
  int tri = gl_VertexID / 3;
  int corner = gl_VertexID - tri * 3;
  uvec3 ids = texelFetch(uTris, ivec2(tri % ${TEX_W}, tri / ${TEX_W}), 0).xyz;
  vIds = ids;
  vBary = vec3(corner == 0, corner == 1, corner == 2);
  uint me = corner == 0 ? ids.x : (corner == 1 ? ids.y : ids.z);
  vec3 p = texelFetch(uPos, tc(me), 0).xyz;
  vNormal = p;
  if (uMode == 0) {
    float e = texelFetch(uElev, tc(me), 0).r;
    gl_Position = uMVP * vec4(p * (1.0 + uExag * max(e, 0.0)), 1.0);
  } else {
    vec3 c[3];
    c[0] = texelFetch(uPos, tc(ids.x), 0).xyz;
    c[1] = texelFetch(uPos, tc(ids.y), 0).xyz;
    c[2] = texelFetch(uPos, tc(ids.z), 0).xyz;
    // Reference corner: the one furthest from a pole.
    int r = 0;
    if (abs(c[1].y) < abs(c[r].y)) r = 1;
    if (abs(c[2].y) < abs(c[r].y)) r = 2;
    float lref = atan(c[r].x, c[r].z);
    float lons[3];
    for (int k = 0; k < 3; k++) {
      float l = atan(c[k].x, c[k].z);
      if (l - lref > PI) l -= 2.0 * PI;
      if (l - lref < -PI) l += 2.0 * PI;
      lons[k] = l;
    }
    // A pole corner takes the mean longitude of the other two.
    for (int k = 0; k < 3; k++) {
      if (abs(c[k].y) > 0.99999) lons[k] = 0.5 * (lons[(k + 1) % 3] + lons[(k + 2) % 3]);
    }
    float lon = lons[corner] + float(gl_InstanceID - 1) * 2.0 * PI;
    float lat = asin(clamp(p.y, -1.0, 1.0));
    gl_Position = vec4(lon * uFlat.x + uFlat.z, lat * uFlat.y + uFlat.w, 0.0, 1.0);
  }
}`;

const MESH_FS = `#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2D uColor;
uniform int uSmooth;
uniform int uMode;
uniform vec3 uLight;
flat in uvec3 vIds;
in vec3 vBary;
in vec3 vNormal;
out vec4 outColor;
ivec2 tc(uint i) { return ivec2(int(i % ${TEX_W}u), int(i / ${TEX_W}u)); }
void main() {
  vec4 c0 = texelFetch(uColor, tc(vIds.x), 0);
  vec4 c1 = texelFetch(uColor, tc(vIds.y), 0);
  vec4 c2 = texelFetch(uColor, tc(vIds.z), 0);
  vec4 c;
  if (uSmooth == 1) {
    c = c0 * vBary.x + c1 * vBary.y + c2 * vBary.z;
  } else {
    c = (vBary.x >= vBary.y && vBary.x >= vBary.z) ? c0 : (vBary.y >= vBary.z ? c1 : c2);
  }
  if (uMode == 0) {
    float l = 0.72 + 0.28 * max(dot(normalize(vNormal), uLight), 0.0);
    c.rgb *= l;
  }
  outColor = vec4(c.rgb, 1.0);
}`;

const LINE_VS = `#version 300 es
precision highp float;
in vec3 aPos;
in vec4 aColor;
uniform mat4 uMVP;
uniform int uMode;
uniform vec4 uFlat;
uniform float uShift;
out vec4 vColor;
void main() {
  vColor = aColor;
  if (uMode == 0) {
    gl_Position = uMVP * vec4(aPos, 1.0);
  } else {
    gl_Position = vec4((aPos.x + uShift) * uFlat.x + uFlat.z, aPos.y * uFlat.y + uFlat.w, 0.0, 1.0);
  }
}`;

// Screen-space ribbons: one instance per segment, expanded to a quad of
// `aW` pixels (with square caps so consecutive segments join without gaps).
const RIBBON_VS = `#version 300 es
precision highp float;
in vec3 aA;
in vec3 aB;
in float aW;
in vec4 aC;
uniform mat4 uMVP;
uniform int uMode;
uniform vec4 uFlat;
uniform float uShift;
uniform vec2 uViewport;
uniform float uScale;
out vec4 vColor;
vec4 proj(vec3 p) {
  if (uMode == 0) return uMVP * vec4(p, 1.0);
  return vec4((p.x + uShift) * uFlat.x + uFlat.z, p.y * uFlat.y + uFlat.w, 0.0, 1.0);
}
void main() {
  vec4 a = proj(aA);
  vec4 b = proj(aB);
  int k = gl_VertexID;
  float t = (k == 1 || k == 2 || k == 4) ? 1.0 : 0.0;
  float side = (k == 0 || k == 1 || k == 3) ? -1.0 : 1.0;
  vec2 sa = a.xy / a.w;
  vec2 sb = b.xy / b.w;
  vec2 d = (sb - sa) * uViewport;
  float len = length(d);
  vec2 dir = len > 1e-6 ? d / len : vec2(1.0, 0.0);
  vec2 nrm = vec2(-dir.y, dir.x);
  float hw = 0.5 * aW * uScale;
  vec2 off = (nrm * side * hw + dir * (t * 2.0 - 1.0) * hw) / uViewport * 2.0;
  vec4 p = mix(a, b, t);
  p.xy += off * p.w;
  gl_Position = p;
  vColor = aC;
}`;

const LINE_FS = `#version 300 es
precision highp float;
in vec4 vColor;
out vec4 outColor;
void main() { outColor = vColor; }`;

function compile(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const mk = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      const log = gl.getShaderInfoLog(s) ?? 'shader error';
      gl.deleteShader(s);
      throw new Error(log);
    }
    return s;
  };
  const vShader = mk(gl.VERTEX_SHADER, vs);
  let fShader: WebGLShader | null = null;
  const p = gl.createProgram()!;
  try {
    fShader = mk(gl.FRAGMENT_SHADER, fs);
    gl.attachShader(p, vShader);
    gl.attachShader(p, fShader);
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
      const log = gl.getProgramInfoLog(p) ?? 'link error';
      gl.deleteProgram(p);
      throw new Error(log);
    }
    gl.detachShader(p, vShader);
    gl.detachShader(p, fShader);
    return p;
  } finally {
    gl.deleteShader(vShader);
    if (fShader) gl.deleteShader(fShader);
  }
}

// ---- small matrix helpers (column-major)
type M4 = Float32Array;
function persp(fovy: number, aspect: number, near: number, far: number): M4 {
  const f = 1 / Math.tan(fovy / 2);
  const m = new Float32Array(16);
  m[0] = f / aspect; m[5] = f; m[10] = (far + near) / (near - far); m[11] = -1; m[14] = (2 * far * near) / (near - far);
  return m;
}
function mul(a: M4, b: M4): M4 {
  const o = new Float32Array(16);
  for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) {
    let s = 0;
    for (let k = 0; k < 4; k++) s += a[k * 4 + j] * b[i * 4 + k];
    o[i * 4 + j] = s;
  }
  return o;
}
function rotX(a: number): M4 {
  const c = Math.cos(a), s = Math.sin(a);
  return new Float32Array([1, 0, 0, 0, 0, c, s, 0, 0, -s, c, 0, 0, 0, 0, 1]);
}
function rotY(a: number): M4 {
  const c = Math.cos(a), s = Math.sin(a);
  return new Float32Array([c, 0, -s, 0, 0, 1, 0, 0, s, 0, c, 0, 0, 0, 0, 1]);
}
function trans(x: number, y: number, z: number): M4 {
  return new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, x, y, z, 1]);
}

export type LineSet = { pos: Float32Array; color: Float32Array; count: number }; // globe xyz or flat lon,lat,0
/** Segments with a width in CSS pixels: a/b are xyz (globe) or lon,lat,0 (flat). */
export type RibbonSet = { a: Float32Array; b: Float32Array; width: Float32Array; color: Float32Array; count: number };

type GpuLine = {
  vao: WebGLVertexArrayObject;
  posBuf: WebGLBuffer;
  colBuf: WebGLBuffer;
  count: number;
};

type GpuLinePair = { globe: GpuLine; flat: GpuLine };

type GpuRibbon = {
  vao: WebGLVertexArrayObject;
  aBuf: WebGLBuffer;
  bBuf: WebGLBuffer;
  wBuf: WebGLBuffer;
  cBuf: WebGLBuffer;
  count: number;
};

type GpuRibbonPair = { globe: GpuRibbon; flat: GpuRibbon };

const LIGHT_VEC = norm([-0.5, 0.6, 0.65]);

export class Renderer {
  gl: WebGL2RenderingContext;
  canvas: HTMLCanvasElement;
  mode: 0 | 1 = 0;
  // Globe camera
  yaw = 0.3;
  pitch = 0.35;
  dist = 3.2;
  // Flat camera (radians)
  cx = 0;
  cy = 0;
  zoom = 1;
  exag = 0;
  smooth = false;
  private mesh: WebGLProgram;
  private line: WebGLProgram;
  private ribbonProg: WebGLProgram;
  private meshU: {
    uTris: WebGLUniformLocation | null;
    uPos: WebGLUniformLocation | null;
    uColor: WebGLUniformLocation | null;
    uElev: WebGLUniformLocation | null;
    uMVP: WebGLUniformLocation | null;
    uMode: WebGLUniformLocation | null;
    uSmooth: WebGLUniformLocation | null;
    uExag: WebGLUniformLocation | null;
    uFlat: WebGLUniformLocation | null;
    uLight: WebGLUniformLocation | null;
  };
  private lineU: {
    uMVP: WebGLUniformLocation | null;
    uMode: WebGLUniformLocation | null;
    uFlat: WebGLUniformLocation | null;
    uShift: WebGLUniformLocation | null;
  };
  private ribbonU: {
    uMVP: WebGLUniformLocation | null;
    uMode: WebGLUniformLocation | null;
    uFlat: WebGLUniformLocation | null;
    uViewport: WebGLUniformLocation | null;
    uScale: WebGLUniformLocation | null;
    uShift: WebGLUniformLocation | null;
  };
  private lineAttr: { aPos: number; aColor: number };
  private ribbonAttr: { aA: number; aB: number; aW: number; aC: number };
  private triTex: WebGLTexture | null = null;
  private posTex: WebGLTexture | null = null;
  private colorTex: WebGLTexture | null = null;
  private elevTex: WebGLTexture | null = null;
  private colorBuf: Uint8Array | null = null;
  private elevBuf: Float32Array | null = null;
  private triCount = 0;
  private nCells = 0;
  private vao: WebGLVertexArrayObject;
  private overlays: Map<string, GpuLinePair> = new Map();
  private graticule: GpuLinePair | null = null;
  private ribbons: Map<string, GpuRibbonPair> = new Map();
  private dirty = false;
  private rafId: number | null = null;
  private disposed = false;
  private contextLost = false;
  private clearColor: [number, number, number] = [0.05, 0.07, 0.09];
  private ro: ResizeObserver;
  private themeMedia: MediaQueryList | null = null;
  private onThemeChange: () => void;
  private onContextLost: (e: Event) => void;
  private onContextRestored: () => void;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    const gl = canvas.getContext('webgl2', { antialias: true, preserveDrawingBuffer: false });
    if (!gl) throw new Error('WebGL2 is not available');
    this.gl = gl;
    this.mesh = compile(gl, MESH_VS, MESH_FS);
    this.line = compile(gl, LINE_VS, LINE_FS);
    this.ribbonProg = compile(gl, RIBBON_VS, LINE_FS);
    this.meshU = {
      uTris: gl.getUniformLocation(this.mesh, 'uTris'),
      uPos: gl.getUniformLocation(this.mesh, 'uPos'),
      uColor: gl.getUniformLocation(this.mesh, 'uColor'),
      uElev: gl.getUniformLocation(this.mesh, 'uElev'),
      uMVP: gl.getUniformLocation(this.mesh, 'uMVP'),
      uMode: gl.getUniformLocation(this.mesh, 'uMode'),
      uSmooth: gl.getUniformLocation(this.mesh, 'uSmooth'),
      uExag: gl.getUniformLocation(this.mesh, 'uExag'),
      uFlat: gl.getUniformLocation(this.mesh, 'uFlat'),
      uLight: gl.getUniformLocation(this.mesh, 'uLight'),
    };
    this.lineU = {
      uMVP: gl.getUniformLocation(this.line, 'uMVP'),
      uMode: gl.getUniformLocation(this.line, 'uMode'),
      uFlat: gl.getUniformLocation(this.line, 'uFlat'),
      uShift: gl.getUniformLocation(this.line, 'uShift'),
    };
    this.ribbonU = {
      uMVP: gl.getUniformLocation(this.ribbonProg, 'uMVP'),
      uMode: gl.getUniformLocation(this.ribbonProg, 'uMode'),
      uFlat: gl.getUniformLocation(this.ribbonProg, 'uFlat'),
      uViewport: gl.getUniformLocation(this.ribbonProg, 'uViewport'),
      uScale: gl.getUniformLocation(this.ribbonProg, 'uScale'),
      uShift: gl.getUniformLocation(this.ribbonProg, 'uShift'),
    };
    this.lineAttr = {
      aPos: gl.getAttribLocation(this.line, 'aPos'),
      aColor: gl.getAttribLocation(this.line, 'aColor'),
    };
    this.ribbonAttr = {
      aA: gl.getAttribLocation(this.ribbonProg, 'aA'),
      aB: gl.getAttribLocation(this.ribbonProg, 'aB'),
      aW: gl.getAttribLocation(this.ribbonProg, 'aW'),
      aC: gl.getAttribLocation(this.ribbonProg, 'aC'),
    };
    this.vao = gl.createVertexArray()!;

    this.updateTheme();
    this.onThemeChange = () => {
      this.updateTheme();
      this.requestDraw();
    };
    if (typeof window.matchMedia === 'function') {
      this.themeMedia = window.matchMedia('(prefers-color-scheme: dark)');
      this.themeMedia.addEventListener?.('change', this.onThemeChange);
    }

    this.onContextLost = (e: Event) => {
      e.preventDefault();
      this.contextLost = true;
    };
    this.onContextRestored = () => {
      this.contextLost = false;
      this.requestDraw();
    };
    canvas.addEventListener('webglcontextlost', this.onContextLost);
    canvas.addEventListener('webglcontextrestored', this.onContextRestored);

    this.ro = new ResizeObserver(() => {
      this.updateTheme();
      this.requestDraw();
    });
    this.ro.observe(canvas);
    this.requestDraw();
  }

  updateTheme() {
    const bg = getComputedStyle(document.documentElement).getPropertyValue('--viewport-bg').trim() || '#0d1117';
    this.clearColor = parseColor(bg);
  }

  private schedule() {
    if (this.rafId !== null || this.disposed) return;
    this.rafId = requestAnimationFrame(() => {
      this.rafId = null;
      if (this.dirty && !this.disposed) {
        this.dirty = false;
        this.draw();
      }
    });
  }

  requestDraw() {
    this.dirty = true;
    this.schedule();
  }

  setExag(v: number) {
    if (this.exag === v) return;
    this.exag = v;
    this.requestDraw();
  }

  private createGpuLine(): GpuLine {
    const gl = this.gl;
    const vao = gl.createVertexArray()!;
    const posBuf = gl.createBuffer()!;
    const colBuf = gl.createBuffer()!;
    gl.bindVertexArray(vao);
    if (this.lineAttr.aPos >= 0) {
      gl.bindBuffer(gl.ARRAY_BUFFER, posBuf);
      gl.enableVertexAttribArray(this.lineAttr.aPos);
      gl.vertexAttribPointer(this.lineAttr.aPos, 3, gl.FLOAT, false, 0, 0);
    }
    if (this.lineAttr.aColor >= 0) {
      gl.bindBuffer(gl.ARRAY_BUFFER, colBuf);
      gl.enableVertexAttribArray(this.lineAttr.aColor);
      gl.vertexAttribPointer(this.lineAttr.aColor, 4, gl.FLOAT, false, 0, 0);
    }
    gl.bindVertexArray(null);
    return { vao, posBuf, colBuf, count: 0 };
  }

  private uploadGpuLine(dst: GpuLine, set: LineSet, usage: number) {
    const gl = this.gl;
    dst.count = set.count;
    if (set.count === 0) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, dst.posBuf);
    gl.bufferData(gl.ARRAY_BUFFER, set.pos, usage);
    gl.bindBuffer(gl.ARRAY_BUFFER, dst.colBuf);
    gl.bufferData(gl.ARRAY_BUFFER, set.color, usage);
  }

  private deleteGpuLinePair(pair: GpuLinePair) {
    const gl = this.gl;
    for (const part of [pair.globe, pair.flat]) {
      gl.deleteBuffer(part.posBuf);
      gl.deleteBuffer(part.colBuf);
      gl.deleteVertexArray(part.vao);
    }
  }

  private createGpuRibbon(): GpuRibbon {
    const gl = this.gl;
    const vao = gl.createVertexArray()!;
    const aBuf = gl.createBuffer()!;
    const bBuf = gl.createBuffer()!;
    const wBuf = gl.createBuffer()!;
    const cBuf = gl.createBuffer()!;
    gl.bindVertexArray(vao);
    const bindAttr = (loc: number, buf: WebGLBuffer, size: number) => {
      if (loc < 0) return;
      gl.bindBuffer(gl.ARRAY_BUFFER, buf);
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
      gl.vertexAttribDivisor(loc, 1);
    };
    bindAttr(this.ribbonAttr.aA, aBuf, 3);
    bindAttr(this.ribbonAttr.aB, bBuf, 3);
    bindAttr(this.ribbonAttr.aW, wBuf, 1);
    bindAttr(this.ribbonAttr.aC, cBuf, 4);
    gl.bindVertexArray(null);
    return { vao, aBuf, bBuf, wBuf, cBuf, count: 0 };
  }

  private uploadGpuRibbon(dst: GpuRibbon, set: RibbonSet) {
    const gl = this.gl;
    dst.count = set.count;
    if (set.count === 0) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, dst.aBuf);
    gl.bufferData(gl.ARRAY_BUFFER, set.a, gl.STATIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, dst.bBuf);
    gl.bufferData(gl.ARRAY_BUFFER, set.b, gl.STATIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, dst.wBuf);
    gl.bufferData(gl.ARRAY_BUFFER, set.width, gl.STATIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, dst.cBuf);
    gl.bufferData(gl.ARRAY_BUFFER, set.color, gl.STATIC_DRAW);
  }

  private deleteGpuRibbonPair(pair: GpuRibbonPair) {
    const gl = this.gl;
    for (const part of [pair.globe, pair.flat]) {
      gl.deleteBuffer(part.aBuf);
      gl.deleteBuffer(part.bBuf);
      gl.deleteBuffer(part.wBuf);
      gl.deleteBuffer(part.cBuf);
      gl.deleteVertexArray(part.vao);
    }
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    this.ro.disconnect();
    this.themeMedia?.removeEventListener?.('change', this.onThemeChange);
    this.canvas.removeEventListener('webglcontextlost', this.onContextLost);
    this.canvas.removeEventListener('webglcontextrestored', this.onContextRestored);
    const gl = this.gl;
    for (const t of [this.triTex, this.posTex, this.colorTex, this.elevTex]) if (t) gl.deleteTexture(t);
    this.triTex = this.posTex = this.colorTex = this.elevTex = null;
    if (this.graticule) {
      this.deleteGpuLinePair(this.graticule);
      this.graticule = null;
    }
    for (const pair of this.overlays.values()) this.deleteGpuLinePair(pair);
    this.overlays.clear();
    for (const pair of this.ribbons.values()) this.deleteGpuRibbonPair(pair);
    this.ribbons.clear();
    gl.deleteVertexArray(this.vao);
    gl.deleteProgram(this.mesh);
    gl.deleteProgram(this.line);
    gl.deleteProgram(this.ribbonProg);
    this.colorBuf = null;
    this.elevBuf = null;
  }

  private tex(internal: number, format: number, type: number, data: ArrayBufferView, comps: number, count: number): WebGLTexture {
    const gl = this.gl;
    const rows = Math.max(1, Math.ceil(count / TEX_W));
    const Ctor = (data as any).constructor;
    const full = new Ctor(TEX_W * rows * comps);
    full.set(data as any);
    const t = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(gl.TEXTURE_2D, 0, internal, TEX_W, rows, 0, format, type, full);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    return t;
  }

  setGrid(g: Grid) {
    const gl = this.gl;
    for (const t of [this.triTex, this.posTex, this.colorTex, this.elevTex]) if (t) gl.deleteTexture(t);
    this.triTex = this.tex(gl.RGB32UI, gl.RGB_INTEGER, gl.UNSIGNED_INT, g.tris, 3, g.t);
    this.posTex = this.tex(gl.RGB32F, gl.RGB, gl.FLOAT, g.pos, 3, g.n);
    this.colorTex = this.tex(gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(g.n * 4).fill(128), 4, g.n);
    this.elevTex = this.tex(gl.R32F, gl.RED, gl.FLOAT, new Float32Array(g.n), 1, g.n);
    this.triCount = g.t;
    this.nCells = g.n;
    const rows = Math.max(1, Math.ceil(g.n / TEX_W));
    this.colorBuf = new Uint8Array(TEX_W * rows * 4);
    this.elevBuf = new Float32Array(TEX_W * rows);
    this.requestDraw();
  }

  setColors(rgba: Uint8Array) {
    if (!this.colorTex || this.nCells === 0) return;
    const gl = this.gl;
    const rows = Math.max(1, Math.ceil(this.nCells / TEX_W));
    const needed = TEX_W * rows * 4;
    if (!this.colorBuf || this.colorBuf.length !== needed) {
      this.colorBuf = new Uint8Array(needed);
    }
    this.colorBuf.set(rgba);
    gl.bindTexture(gl.TEXTURE_2D, this.colorTex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, TEX_W, rows, gl.RGBA, gl.UNSIGNED_BYTE, this.colorBuf);
    this.requestDraw();
  }

  /** Elevation for globe displacement, as a fraction of the radius. */
  setRelief(frac: Float32Array) {
    if (!this.elevTex || this.nCells === 0) return;
    const gl = this.gl;
    const rows = Math.max(1, Math.ceil(this.nCells / TEX_W));
    const needed = TEX_W * rows;
    if (!this.elevBuf || this.elevBuf.length !== needed) {
      this.elevBuf = new Float32Array(needed);
    }
    this.elevBuf.set(frac);
    gl.bindTexture(gl.TEXTURE_2D, this.elevTex);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, TEX_W, rows, gl.RED, gl.FLOAT, this.elevBuf);
    this.requestDraw();
  }

  setOverlay(name: string, globe: LineSet | null, flat: LineSet | null) {
    const existing = this.overlays.get(name);
    if (!globe || !flat) {
      if (existing) {
        this.deleteGpuLinePair(existing);
        this.overlays.delete(name);
      }
    } else {
      const pair = existing ?? { globe: this.createGpuLine(), flat: this.createGpuLine() };
      const usage = name === 'brush' ? this.gl.DYNAMIC_DRAW : this.gl.STATIC_DRAW;
      this.uploadGpuLine(pair.globe, globe, usage);
      this.uploadGpuLine(pair.flat, flat, usage);
      if (!existing) this.overlays.set(name, pair);
    }
    this.requestDraw();
  }

  /** Reference lines drawn under every other overlay (null removes them). */
  setGraticule(globe: LineSet | null, flat: LineSet | null) {
    if (!globe || !flat) {
      if (this.graticule) {
        this.deleteGpuLinePair(this.graticule);
        this.graticule = null;
      }
    } else {
      const pair = this.graticule ?? { globe: this.createGpuLine(), flat: this.createGpuLine() };
      this.uploadGpuLine(pair.globe, globe, this.gl.STATIC_DRAW);
      this.uploadGpuLine(pair.flat, flat, this.gl.STATIC_DRAW);
      this.graticule = pair;
    }
    this.requestDraw();
  }

  setRibbons(name: string, globe: RibbonSet | null, flat: RibbonSet | null) {
    const existing = this.ribbons.get(name);
    if (!globe || !flat) {
      if (existing) {
        this.deleteGpuRibbonPair(existing);
        this.ribbons.delete(name);
      }
    } else {
      const pair = existing ?? { globe: this.createGpuRibbon(), flat: this.createGpuRibbon() };
      this.uploadGpuRibbon(pair.globe, globe);
      this.uploadGpuRibbon(pair.flat, flat);
      if (!existing) this.ribbons.set(name, pair);
    }
    this.requestDraw();
  }

  private resize(): [number, number] {
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(this.canvas.clientWidth * dpr));
    const h = Math.max(1, Math.round(this.canvas.clientHeight * dpr));
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    return [w, h];
  }

  mvp(): M4 {
    const [w, h] = [this.canvas.width, this.canvas.height];
    const view = mul(trans(0, 0, -this.dist), mul(rotX(this.pitch), rotY(-this.yaw)));
    return mul(persp(0.7, w / h, 0.01, 100), view);
  }

  flatXform(): [number, number, number, number] {
    const [w, h] = [this.canvas.width, this.canvas.height];
    // Whole world (2π wide) fits the width at zoom 1, or the height if that is tighter.
    const base = Math.min(2 / (2 * Math.PI), (2 / Math.PI) * (h / w));
    const sx = base * this.zoom;
    const sy = sx * (w / h);
    return [sx, sy, -this.cx * sx, -this.cy * sy];
  }

  draw() {
    if (this.disposed || this.contextLost) return;
    const gl = this.gl;
    const [w, h] = this.resize();
    gl.viewport(0, 0, w, h);
    const c = this.clearColor;
    gl.clearColor(c[0], c[1], c[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    if (!this.triTex) return;
    const mvp = this.mvp();
    const fl = this.flatXform();

    gl.useProgram(this.mesh);
    gl.bindVertexArray(this.vao);
    const u = this.meshU;
    const bind = (unit: number, t: WebGLTexture | null, loc: WebGLUniformLocation | null) => {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.uniform1i(loc, unit);
    };
    bind(0, this.triTex, u.uTris);
    bind(1, this.posTex, u.uPos);
    bind(2, this.colorTex, u.uColor);
    bind(3, this.elevTex, u.uElev);
    gl.uniformMatrix4fv(u.uMVP, false, mvp);
    gl.uniform1i(u.uMode, this.mode);
    gl.uniform1i(u.uSmooth, this.smooth ? 1 : 0);
    gl.uniform1f(u.uExag, this.exag);
    gl.uniform4f(u.uFlat, fl[0], fl[1], fl[2], fl[3]);
    gl.uniform3f(u.uLight, LIGHT_VEC[0], LIGHT_VEC[1], LIGHT_VEC[2]);
    if (this.mode === 0) {
      gl.enable(gl.DEPTH_TEST);
      gl.enable(gl.CULL_FACE);
      gl.cullFace(gl.BACK);
      gl.frontFace(gl.CCW);
      gl.drawArrays(gl.TRIANGLES, 0, this.triCount * 3);
    } else {
      gl.disable(gl.DEPTH_TEST);
      gl.disable(gl.CULL_FACE);
      gl.drawArraysInstanced(gl.TRIANGLES, 0, this.triCount * 3, 3);
    }

    // Reference lines (latitudes), right on the map: under rivers, borders and every other overlay.
    if (this.graticule) this.drawLines([this.mode === 0 ? this.graticule.globe : this.graticule.flat], mvp, fl);

    // Ribbons (rivers), drawn under the line overlays.
    if (this.ribbons.size) {
      gl.useProgram(this.ribbonProg);
      const q = this.ribbonU;
      gl.uniformMatrix4fv(q.uMVP, false, mvp);
      gl.uniform1i(q.uMode, this.mode);
      gl.uniform4f(q.uFlat, fl[0], fl[1], fl[2], fl[3]);
      gl.uniform2f(q.uViewport, w, h);
      const dpr = window.devicePixelRatio || 1;
      // Rivers thicken a little as you zoom in.
      const zoomK = this.mode === 0 ? Math.min(3, Math.max(0.8, Math.sqrt(2.2 / (this.dist - 1)))) : Math.min(3, Math.max(0.8, Math.sqrt(this.zoom)));
      gl.uniform1f(q.uScale, dpr * zoomK);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      const shifts = this.mode === 0 ? [0] : [-2 * Math.PI, 0, 2 * Math.PI];
      for (const rb of this.ribbons.values()) {
        const set = this.mode === 0 ? rb.globe : rb.flat;
        if (!set.count) continue;
        gl.bindVertexArray(set.vao);
        for (const s of shifts) {
          gl.uniform1f(q.uShift, s);
          gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, set.count);
        }
      }
      gl.disable(gl.BLEND);
      gl.bindVertexArray(null);
    }

    // Line overlays (wind, plate motion, hairline borders, markers).
    if (this.overlays.size) {
      const sets: GpuLine[] = [];
      for (const ov of this.overlays.values()) sets.push(this.mode === 0 ? ov.globe : ov.flat);
      this.drawLines(sets, mvp, fl);
    }
  }

  /** Hairlines (GL lines) over the map; on the flat map, also one world to each side. */
  private drawLines(sets: GpuLine[], mvp: M4, fl: [number, number, number, number]) {
    const gl = this.gl;
    gl.useProgram(this.line);
    const l = this.lineU;
    gl.uniformMatrix4fv(l.uMVP, false, mvp);
    gl.uniform1i(l.uMode, this.mode);
    gl.uniform4f(l.uFlat, fl[0], fl[1], fl[2], fl[3]);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    for (const set of sets) {
      if (set.count === 0) continue;
      gl.bindVertexArray(set.vao);
      if (this.mode === 0) {
        gl.uniform1f(l.uShift, 0);
        gl.drawArrays(gl.LINES, 0, set.count);
      } else {
        for (const s of [-2 * Math.PI, 0, 2 * Math.PI]) {
          gl.uniform1f(l.uShift, s);
          gl.drawArrays(gl.LINES, 0, set.count);
        }
      }
    }
    gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
  }

  /** Unit vector under a canvas pixel (CSS pixels), or null if off the planet. */
  pick(x: number, y: number): Vec3 | null {
    const dpr = window.devicePixelRatio || 1;
    const [w, h] = [this.canvas.width, this.canvas.height];
    const nx = (x * dpr / w) * 2 - 1;
    const ny = 1 - (y * dpr / h) * 2;
    if (this.mode === 1) {
      const fl = this.flatXform();
      let lon = (nx - fl[2]) / fl[0];
      const lat = (ny - fl[3]) / fl[1];
      if (Math.abs(lat) > Math.PI / 2) return null;
      lon = ((lon + Math.PI) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI) - Math.PI;
      const c = Math.cos(lat);
      return [c * Math.sin(lon), Math.sin(lat), c * Math.cos(lon)];
    }
    const inv = invert(this.mvp());
    if (!inv) return null;
    const a = xform(inv, [nx, ny, -1, 1]);
    const b = xform(inv, [nx, ny, 1, 1]);
    const o: Vec3 = [a[0] / a[3], a[1] / a[3], a[2] / a[3]];
    const e: Vec3 = [b[0] / b[3], b[1] / b[3], b[2] / b[3]];
    const d = norm([e[0] - o[0], e[1] - o[1], e[2] - o[2]]);
    const bq = o[0] * d[0] + o[1] * d[1] + o[2] * d[2];
    const cq = o[0] * o[0] + o[1] * o[1] + o[2] * o[2] - 1;
    const disc = bq * bq - cq;
    if (disc < 0) return null;
    const t = -bq - Math.sqrt(disc);
    return norm([o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t]);
  }

  /** Centre the view on a point. */
  lookAt(p: Vec3) {
    const [lat, lon] = toLatLon(p);
    this.yaw = lon;
    this.pitch = lat;
    this.cx = lon;
    this.cy = lat;
    this.requestDraw();
  }
}

function norm(v: number[]): Vec3 {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
}
function xform(m: M4, v: number[]): number[] {
  const o = [0, 0, 0, 0];
  for (let i = 0; i < 4; i++) for (let k = 0; k < 4; k++) o[i] += m[k * 4 + i] * v[k];
  return o;
}
function invert(m: M4): M4 | null {
  const inv = new Float32Array(16);
  const a = m;
  inv[0] = a[5]*a[10]*a[15]-a[5]*a[11]*a[14]-a[9]*a[6]*a[15]+a[9]*a[7]*a[14]+a[13]*a[6]*a[11]-a[13]*a[7]*a[10];
  inv[4] = -a[4]*a[10]*a[15]+a[4]*a[11]*a[14]+a[8]*a[6]*a[15]-a[8]*a[7]*a[14]-a[12]*a[6]*a[11]+a[12]*a[7]*a[10];
  inv[8] = a[4]*a[9]*a[15]-a[4]*a[11]*a[13]-a[8]*a[5]*a[15]+a[8]*a[7]*a[13]+a[12]*a[5]*a[11]-a[12]*a[7]*a[9];
  inv[12] = -a[4]*a[9]*a[14]+a[4]*a[10]*a[13]+a[8]*a[5]*a[14]-a[8]*a[6]*a[13]-a[12]*a[5]*a[10]+a[12]*a[6]*a[9];
  inv[1] = -a[1]*a[10]*a[15]+a[1]*a[11]*a[14]+a[9]*a[2]*a[15]-a[9]*a[3]*a[14]-a[13]*a[2]*a[11]+a[13]*a[3]*a[10];
  inv[5] = a[0]*a[10]*a[15]-a[0]*a[11]*a[14]-a[8]*a[2]*a[15]+a[8]*a[3]*a[14]+a[12]*a[2]*a[11]-a[12]*a[3]*a[10];
  inv[9] = -a[0]*a[9]*a[15]+a[0]*a[11]*a[13]+a[8]*a[1]*a[15]-a[8]*a[3]*a[13]-a[12]*a[1]*a[11]+a[12]*a[3]*a[9];
  inv[13] = a[0]*a[9]*a[14]-a[0]*a[10]*a[13]-a[8]*a[1]*a[14]+a[8]*a[2]*a[13]+a[12]*a[1]*a[10]-a[12]*a[2]*a[9];
  inv[2] = a[1]*a[6]*a[15]-a[1]*a[7]*a[14]-a[5]*a[2]*a[15]+a[5]*a[3]*a[14]+a[13]*a[2]*a[7]-a[13]*a[3]*a[6];
  inv[6] = -a[0]*a[6]*a[15]+a[0]*a[7]*a[14]+a[4]*a[2]*a[15]-a[4]*a[3]*a[14]-a[12]*a[2]*a[7]+a[12]*a[3]*a[6];
  inv[10] = a[0]*a[5]*a[15]-a[0]*a[7]*a[13]-a[4]*a[1]*a[15]+a[4]*a[3]*a[13]+a[12]*a[1]*a[7]-a[12]*a[3]*a[5];
  inv[14] = -a[0]*a[5]*a[14]+a[0]*a[6]*a[13]+a[4]*a[1]*a[14]-a[4]*a[2]*a[13]-a[12]*a[1]*a[6]+a[12]*a[2]*a[5];
  inv[3] = -a[1]*a[6]*a[11]+a[1]*a[7]*a[10]+a[5]*a[2]*a[11]-a[5]*a[3]*a[10]-a[9]*a[2]*a[7]+a[9]*a[3]*a[6];
  inv[7] = a[0]*a[6]*a[11]-a[0]*a[7]*a[10]-a[4]*a[2]*a[11]+a[4]*a[3]*a[10]+a[8]*a[2]*a[7]-a[8]*a[3]*a[6];
  inv[11] = -a[0]*a[5]*a[11]+a[0]*a[7]*a[9]+a[4]*a[1]*a[11]-a[4]*a[3]*a[9]-a[8]*a[1]*a[7]+a[8]*a[3]*a[5];
  inv[15] = a[0]*a[5]*a[10]-a[0]*a[6]*a[9]-a[4]*a[1]*a[10]+a[4]*a[2]*a[9]+a[8]*a[1]*a[6]-a[8]*a[2]*a[5];
  let det = a[0]*inv[0]+a[1]*inv[4]+a[2]*inv[8]+a[3]*inv[12];
  if (Math.abs(det) < 1e-12) return null;
  det = 1 / det;
  for (let i = 0; i < 16; i++) inv[i] *= det;
  return inv;
}

function parseColor(s: string): [number, number, number] {
  const m = s.match(/^#([0-9a-f]{6})$/i);
  if (!m) return [0.05, 0.07, 0.09];
  const v = parseInt(m[1], 16);
  return [((v >> 16) & 255) / 255, ((v >> 8) & 255) / 255, (v & 255) / 255];
}
