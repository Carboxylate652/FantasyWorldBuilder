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
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'shader error');
    return s;
  };
  const p = gl.createProgram()!;
  gl.attachShader(p, mk(gl.VERTEX_SHADER, vs));
  gl.attachShader(p, mk(gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? 'link error');
  return p;
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
  private triTex: WebGLTexture | null = null;
  private posTex: WebGLTexture | null = null;
  private colorTex: WebGLTexture | null = null;
  private elevTex: WebGLTexture | null = null;
  private triCount = 0;
  private nCells = 0;
  private vao: WebGLVertexArrayObject;
  private lineVao: WebGLVertexArrayObject;
  private lineBuf: WebGLBuffer;
  private lineColBuf: WebGLBuffer;
  private overlays: Map<string, { globe: LineSet; flat: LineSet }> = new Map();
  private ribbonProg: WebGLProgram;
  private ribbonVao: WebGLVertexArrayObject;
  private ribbonBufs: Record<string, WebGLBuffer> = {};
  private ribbons: Map<string, { globe: RibbonSet; flat: RibbonSet }> = new Map();
  private dirty = true;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    const gl = canvas.getContext('webgl2', { antialias: true, preserveDrawingBuffer: false });
    if (!gl) throw new Error('WebGL2 is not available');
    this.gl = gl;
    this.mesh = compile(gl, MESH_VS, MESH_FS);
    this.line = compile(gl, LINE_VS, LINE_FS);
    this.vao = gl.createVertexArray()!;
    this.lineVao = gl.createVertexArray()!;
    this.lineBuf = gl.createBuffer()!;
    this.lineColBuf = gl.createBuffer()!;
    gl.bindVertexArray(this.lineVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.lineBuf);
    const aPos = gl.getAttribLocation(this.line, 'aPos');
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 3, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.lineColBuf);
    const aCol = gl.getAttribLocation(this.line, 'aColor');
    gl.enableVertexAttribArray(aCol);
    gl.vertexAttribPointer(aCol, 4, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
    this.ribbonProg = compile(gl, RIBBON_VS, LINE_FS);
    this.ribbonVao = gl.createVertexArray()!;
    gl.bindVertexArray(this.ribbonVao);
    for (const [name, size] of [['aA', 3], ['aB', 3], ['aW', 1], ['aC', 4]] as [string, number][]) {
      const buf = gl.createBuffer()!;
      this.ribbonBufs[name] = buf;
      const loc = gl.getAttribLocation(this.ribbonProg, name);
      gl.bindBuffer(gl.ARRAY_BUFFER, buf);
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
      gl.vertexAttribDivisor(loc, 1);
    }
    gl.bindVertexArray(null);
    const loop = () => {
      if (this.dirty) {
        this.dirty = false;
        this.draw();
      }
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
    new ResizeObserver(() => this.requestDraw()).observe(canvas);
  }

  requestDraw() {
    this.dirty = true;
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
    this.requestDraw();
  }

  setColors(rgba: Uint8Array) {
    const gl = this.gl;
    const rows = Math.ceil(this.nCells / TEX_W);
    const full = new Uint8Array(TEX_W * rows * 4);
    full.set(rgba);
    gl.bindTexture(gl.TEXTURE_2D, this.colorTex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, TEX_W, rows, gl.RGBA, gl.UNSIGNED_BYTE, full);
    this.requestDraw();
  }

  /** Elevation for globe displacement, as a fraction of the radius. */
  setRelief(frac: Float32Array) {
    const gl = this.gl;
    const rows = Math.ceil(this.nCells / TEX_W);
    const full = new Float32Array(TEX_W * rows);
    full.set(frac);
    gl.bindTexture(gl.TEXTURE_2D, this.elevTex);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, TEX_W, rows, gl.RED, gl.FLOAT, full);
    this.requestDraw();
  }

  setOverlay(name: string, globe: LineSet | null, flat: LineSet | null) {
    if (!globe || !flat) this.overlays.delete(name);
    else this.overlays.set(name, { globe, flat });
    this.requestDraw();
  }

  setRibbons(name: string, globe: RibbonSet | null, flat: RibbonSet | null) {
    if (!globe || !flat) this.ribbons.delete(name);
    else this.ribbons.set(name, { globe, flat });
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
    const gl = this.gl;
    const [w, h] = this.resize();
    gl.viewport(0, 0, w, h);
    const bg = getComputedStyle(document.documentElement).getPropertyValue('--viewport-bg').trim() || '#0d1117';
    const c = parseColor(bg);
    gl.clearColor(c[0], c[1], c[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    if (!this.triTex) return;
    const mvp = this.mvp();
    const fl = this.flatXform();

    gl.useProgram(this.mesh);
    gl.bindVertexArray(this.vao);
    const U = (n: string) => gl.getUniformLocation(this.mesh, n);
    const bind = (unit: number, t: WebGLTexture | null, name: string) => {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.uniform1i(U(name), unit);
    };
    bind(0, this.triTex, 'uTris');
    bind(1, this.posTex, 'uPos');
    bind(2, this.colorTex, 'uColor');
    bind(3, this.elevTex, 'uElev');
    gl.uniformMatrix4fv(U('uMVP'), false, mvp);
    gl.uniform1i(U('uMode'), this.mode);
    gl.uniform1i(U('uSmooth'), this.smooth ? 1 : 0);
    gl.uniform1f(U('uExag'), this.exag);
    gl.uniform4f(U('uFlat'), fl[0], fl[1], fl[2], fl[3]);
    const lv = norm([-0.5, 0.6, 0.65]);
    gl.uniform3f(U('uLight'), lv[0], lv[1], lv[2]);
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

    // Ribbons (rivers), drawn under the line overlays.
    if (this.ribbons.size) {
      gl.useProgram(this.ribbonProg);
      gl.bindVertexArray(this.ribbonVao);
      const Q = (n: string) => gl.getUniformLocation(this.ribbonProg, n);
      gl.uniformMatrix4fv(Q('uMVP'), false, mvp);
      gl.uniform1i(Q('uMode'), this.mode);
      gl.uniform4f(Q('uFlat'), fl[0], fl[1], fl[2], fl[3]);
      gl.uniform2f(Q('uViewport'), w, h);
      const dpr = window.devicePixelRatio || 1;
      // Rivers thicken a little as you zoom in.
      const zoomK = this.mode === 0 ? Math.min(3, Math.max(0.8, Math.sqrt(2.2 / (this.dist - 1)))) : Math.min(3, Math.max(0.8, Math.sqrt(this.zoom)));
      gl.uniform1f(Q('uScale'), dpr * zoomK);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      for (const rb of this.ribbons.values()) {
        const set = this.mode === 0 ? rb.globe : rb.flat;
        if (!set.count) continue;
        const put = (n: string, d: Float32Array) => { gl.bindBuffer(gl.ARRAY_BUFFER, this.ribbonBufs[n]); gl.bufferData(gl.ARRAY_BUFFER, d, gl.DYNAMIC_DRAW); };
        put('aA', set.a); put('aB', set.b); put('aW', set.width); put('aC', set.color);
        for (const s of this.mode === 0 ? [0] : [-2 * Math.PI, 0, 2 * Math.PI]) {
          gl.uniform1f(Q('uShift'), s);
          gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, set.count);
        }
      }
      gl.disable(gl.BLEND);
    }

    // Overlays.
    gl.useProgram(this.line);
    gl.bindVertexArray(this.lineVao);
    const L = (n: string) => gl.getUniformLocation(this.line, n);
    gl.uniformMatrix4fv(L('uMVP'), false, mvp);
    gl.uniform1i(L('uMode'), this.mode);
    gl.uniform4f(L('uFlat'), fl[0], fl[1], fl[2], fl[3]);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    for (const ov of this.overlays.values()) {
      const set = this.mode === 0 ? ov.globe : ov.flat;
      if (set.count === 0) continue;
      gl.bindBuffer(gl.ARRAY_BUFFER, this.lineBuf);
      gl.bufferData(gl.ARRAY_BUFFER, set.pos, gl.DYNAMIC_DRAW);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.lineColBuf);
      gl.bufferData(gl.ARRAY_BUFFER, set.color, gl.DYNAMIC_DRAW);
      if (this.mode === 0) {
        gl.uniform1f(L('uShift'), 0);
        gl.drawArrays(gl.LINES, 0, set.count);
      } else {
        for (const s of [-2 * Math.PI, 0, 2 * Math.PI]) {
          gl.uniform1f(L('uShift'), s);
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
