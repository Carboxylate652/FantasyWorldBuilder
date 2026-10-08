import './style.css';
import * as api from './api';
import { Grid, eastNorth, fromLatLon, toLatLon, type Vec3 } from './grid';
import { BOUNDARY, KOPPEN, LAYERS, MONTHS, TERRAIN, colorize, cultureColor, groupColor, hillshade, plateColor, stateColor, type LayerId, type Legend } from './layers';
import { Renderer, type LineSet, type RibbonSet } from './render';
import { Noise, SCATTER_STREAM, scatterOctaves, scatterWeight } from './noise';
import { EDITOR_TOOLS, STAGE2_START, STAGE3_START, STAGE_ENDS, STEPS, TOOLS, type Param, type StepUI, type ToolId } from './schema';

// ------------------------------------------------------------------ state

/** States, regions, continents, provinces and straits of the latest run, plus
 *  cultures when they are up to date, indexed by id. */
type Political = {
  states: any[]; regions: any[]; continents: any[]; provinces: any[]; adjacencies: any[];
  cultures: any[]; groups: any[]; events: any[];
  byState: Map<number, any>; byRegion: Map<number, any>; byProv: Map<number, any>; byCont: Map<number, any>;
  byCulture: Map<number, any>; byGroup: Map<number, any>;
};

type StepStatus = { key: string; title: string; state: 'done' | 'stale' | 'empty'; millis: number; meta: any };
type Status = {
  steps: StepStatus[];
  params: any;
  edits: {
    elevation_import: any | null; province_import: any | null; auto_base: boolean; sketch_strokes: number; pins: any[]; arrows: any[];
    plate_strokes: number; elevation_strokes: number; biome_strokes: number; barrier_strokes: number; state_strokes: number; province_strokes: number;
    site_pins: number; sites: { lat: number; lon: number; population: number }[];
    fertility_strokes: number; band_pins: { lat: number; lon: number; bands: number }[];
  };
  grid: { level: number; cells: number; spacing_km: number };
  path: string | null;
  can_undo: boolean;
  can_redo: boolean;
  dirty: boolean;
};

const S = {
  status: null as Status | null,
  grid: null as Grid | null,
  layer: 'sketch' as LayerId,
  month: null as number | null,
  tool: 'navigate' as ToolId,
  brush: { radius_km: 400, strength: 1, hardness: 0.5, scatter: 0.7, grain_km: 250 },
  toolValue: {} as Record<string, number>,
  pinKind: 'auto',
  openStep: 'sketch',
  /** Map editor toolbar shown (province, state and goods tools). */
  editor: false,
  overlays: { rivers: true, wind: false, motion: false, edits: true, borders: false },
  political: null as Political | null,
  exag: 0,
  autoRun: true,
  busy: false,
  cache: new Map<string, api.FieldData | null>(),
  shade: null as Float32Array | null,
  colors: null as Uint8Array | null,
  legend: null as Legend | null,
};

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector(sel) as T;
const h = (tag: string, attrs: Record<string, any> = {}, ...kids: (Node | string | null | undefined | false)[]) => {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'class') el.className = v;
    else if (v === true) el.setAttribute(k, '');
    else if (v !== false && v !== null && v !== undefined) el.setAttribute(k, String(v));
  }
  for (const c of kids) if (c !== null && c !== undefined && c !== false) el.append(c);
  return el;
};

let R: Renderer;

// ------------------------------------------------------------------ boot

async function boot() {
  document.body.innerHTML = '';
  document.body.append(
    h('header', { id: 'topbar' }),
    h('div', { id: 'body' },
      h('aside', { id: 'steps' }),
      h('main', { id: 'viewport' },
        h('canvas', { id: 'canvas' }),
        h('div', { id: 'toolbar' }),
        h('div', { id: 'brushbar' }),
        h('div', { id: 'legend' }),
        h('div', { id: 'toast' }),
      ),
    ),
    h('footer', { id: 'statusbar' }, h('span', { id: 'hover' }, 'Hover the planet for cell details.'), h('span', { id: 'progress' })),
  );
  try {
    R = new Renderer($('#canvas') as HTMLCanvasElement);
  } catch (e) {
    toast(String(e), true);
    return;
  }
  bindCanvas();
  bindKeys();
  // Handle for debugging from the devtools console.
  (window as any).fwm = { R, S };
  try {
    await refreshStatus();
  } catch (e) {
    $('#steps').append(h('div', { class: 'error' },
      'Cannot reach the world engine. ',
      api.isTauri ? String(e) : h('span', {}, 'Start it with ', h('code', {}, 'worldgen serve --static app/dist'), '.')));
    return;
  }
  // A fresh session: generate the sketch so there is something to look at.
  if (S.status!.steps[1].state === 'empty') await run('sketch');
}

// ------------------------------------------------------------------ status & data

async function refreshStatus(st?: Status) {
  S.status = st ?? (await api.json<Status>('status'));
  if (!S.grid || S.grid.n !== S.status.grid.cells) {
    S.grid = new Grid(await api.bin('grid'));
    R.setGrid(S.grid);
    S.cache.clear();
  }
  renderTopbar();
  renderSteps();
  renderToolbar();
  await refreshLayer();
}

function invalidate() {
  S.cache.clear();
  S.shade = null;
  S.political = null;
}

async function political(): Promise<Political | null> {
  if (S.political) return S.political;
  const st = S.status?.steps;
  if (!st || (st[STAGE2_START + 1]?.state === 'empty' && st[STAGE2_START + 2]?.state === 'empty')) return null;
  try {
    const t = await api.json('political');
    if (!t) return null;
    const idx = (a: any[] | undefined) => new Map<number, any>((a ?? []).map((x) => [x.id, x]));
    S.political = {
      states: t.states ?? [], regions: t.regions ?? [], continents: t.continents ?? [], provinces: t.provinces ?? [], adjacencies: t.adjacencies ?? [],
      cultures: t.cultures ?? [], groups: t.culture_groups ?? [], events: t.culture_events ?? [],
      byState: idx(t.states), byRegion: idx(t.regions), byProv: idx(t.provinces), byCont: idx(t.continents),
      byCulture: idx(t.cultures), byGroup: idx(t.culture_groups),
    };
  } catch {
    return null;
  }
  return S.political;
}

async function getField(name: string, month: number | null = null): Promise<api.FieldData | null> {
  const key = `${name}|${month}`;
  if (!S.cache.has(key)) S.cache.set(key, await api.field(name, month ?? undefined));
  return S.cache.get(key)!;
}

async function refreshLayer() {
  const g = S.grid!;
  const def = LAYERS.find((l) => l.id === S.layer)!;
  const f: Record<string, ArrayLike<number>> = {};
  let stale = false;
  let missing = false;
  for (const name of def.fields) {
    const d = await getField(name, def.monthly && ['temp', 'precip', 'wind_u', 'wind_v'].includes(name) ? S.month : null);
    if (d) {
      f[name] = d.values;
      stale ||= d.stale;
    } else if (name === def.fields[0]) missing = true;
  }
  if (!S.shade && f['elevation']) S.shade = hillshade(g, f['elevation'], S.status!.params.planet.radius_km);
  if (missing) {
    // Fall back to whatever relief exists so the globe is never blank.
    const e = await getField('elevation');
    const fb = e ? colorize('elevation', g, { elevation: e.values }, S.shade, null) : colorize('sketch', g, { sketch: (await getField('sketch'))?.values }, null, null);
    S.colors = fb.rgba;
    S.legend = { title: `${def.label}: run the ${stepOfLayer(S.layer)} step to see this layer` };
  } else {
    const r = colorize(S.layer, g, f, S.shade, S.month);
    S.colors = r.rgba;
    S.legend = r.legend;
  }
  R.smooth = def.smooth;
  R.setColors(S.colors);
  const elev = await getField('elevation');
  if (elev && S.exag > 0) {
    const R0 = S.status!.params.planet.radius_km * 1000;
    R.setRelief(Float32Array.from(elev.values as Float32Array, (e) => e / R0));
  }
  R.exag = S.exag;
  renderLegend(stale);
  await refreshOverlays();
}

function stepOfLayer(id: LayerId): string {
  const m: Record<string, string> = {
    sketch: 'Continent sketch', plates: 'Plates', crust: 'Plates', boundaries: 'Tectonic relief', elevation: 'Tectonic relief',
    stress: 'Tectonic relief', ocean_age: 'Tectonic relief', temperature: 'Climate', precipitation: 'Climate', continentality: 'Climate',
    currents: 'Climate', discharge: 'Hydrology', erosion: 'Hydrology', koppen: 'Biomes', terrain: 'Biomes',
    habitability: 'Habitability & barriers', barrier: 'Habitability & barriers', springs: 'Habitability & barriers', states: 'States', regions: 'States', provinces: 'Provinces',
    resources: 'Provinces',
    cultures: 'Cultures', culture_groups: 'Cultures', population: 'Cultures',
  };
  return m[id];
}

// ------------------------------------------------------------------ overlays

function surface(p: Vec3, lift = 0.002): number[] {
  const k = 1 + lift;
  return [p[0] * k, p[1] * k, p[2] * k];
}

class Lines {
  g: number[] = [];
  f: number[] = [];
  c: number[] = [];
  seg(a: Vec3, b: Vec3, col: [number, number, number, number], lift = 0.002, la = 0, lb = 0) {
    this.g.push(...surface(a, lift + la), ...surface(b, lift + lb));
    const [lat1, lon1] = toLatLon(a);
    let [lat2, lon2] = toLatLon(b);
    if (lon2 - lon1 > Math.PI) lon2 -= 2 * Math.PI;
    if (lon2 - lon1 < -Math.PI) lon2 += 2 * Math.PI;
    this.f.push(lon1, lat1, 0, lon2, lat2, 0);
    this.c.push(...col, ...col);
  }
  sets(): [LineSet, LineSet] {
    const count = this.g.length / 3;
    const color = new Float32Array(this.c);
    return [{ pos: new Float32Array(this.g), color, count }, { pos: new Float32Array(this.f), color, count }];
  }
}

function offset(p: Vec3, e: Vec3, n: Vec3, de: number, dn: number): Vec3 {
  const q: Vec3 = [p[0] + e[0] * de + n[0] * dn, p[1] + e[1] * de + n[1] * dn, p[2] + e[2] * de + n[2] * dn];
  const l = Math.hypot(...q);
  return [q[0] / l, q[1] / l, q[2] / l];
}

function arrow(L: Lines, p: Vec3, de: number, dn: number, col: [number, number, number, number]) {
  const [e, n] = eastNorth(p);
  const q = offset(p, e, n, de, dn);
  L.seg(p, q, col);
  const len = Math.hypot(de, dn);
  if (len < 1e-6) return;
  const ue = de / len, un = dn / len;
  const hl = len * 0.35;
  for (const s of [1, -1]) {
    const be = -ue * 0.8 + s * -un * 0.5, bn = -un * 0.8 + s * ue * 0.5;
    L.seg(q, offset(q, e, n, be * hl, bn * hl), col);
  }
}

async function refreshOverlays() {
  const g = S.grid!;
  const exagLift = (i: number, elev: ArrayLike<number> | undefined) =>
    elev && S.exag > 0 ? (S.exag * Math.max(0, elev[i])) / (S.status!.params.planet.radius_km * 1000) : 0;

  // Rivers as ribbons whose on-screen width follows the real bankfull width
  // (log scale: ~0.8 px for a 60 m stream, ~5 px for a 2 km great river).
  const width = S.overlays.rivers ? await getField('river_width') : null;
  const recv = width ? await getField('receiver') : null;
  if (width && recv && !width.stale) {
    const elev = (await getField('elevation'))?.values;
    const wv = width.values, rc = recv.values;
    const segs: number[] = [];
    for (let i = 0; i < g.n; i++) if (wv[i] > 0 && rc[i] >= 0) segs.push(i);
    const mk = (): RibbonSet => ({ a: new Float32Array(segs.length * 3), b: new Float32Array(segs.length * 3), width: new Float32Array(segs.length), color: new Float32Array(segs.length * 4), count: segs.length });
    const gl3 = mk(), fl = mk();
    segs.forEach((i, k) => {
      const r = rc[i];
      const t = Math.min(1, Math.max(0, Math.log(wv[i] / 60) / Math.log(2000 / 60)));
      const px = 0.8 + 4.2 * t;
      const col = [0.32 - 0.22 * t, 0.58 - 0.3 * t, 0.95 - 0.15 * t, 0.85 + 0.15 * t];
      const pa = surface(g.p(i), 0.0025 + exagLift(i, elev)), pb = surface(g.p(r), 0.0025 + exagLift(r, elev));
      gl3.a.set(pa, k * 3); gl3.b.set(pb, k * 3);
      const [la, loa] = toLatLon(g.p(i));
      let [lb, lob] = toLatLon(g.p(r));
      if (lob - loa > Math.PI) lob -= 2 * Math.PI;
      if (lob - loa < -Math.PI) lob += 2 * Math.PI;
      fl.a.set([loa, la, 0], k * 3); fl.b.set([lob, lb, 0], k * 3);
      gl3.width[k] = fl.width[k] = px;
      gl3.color.set(col, k * 4); fl.color.set(col, k * 4);
    });
    R.setRibbons('rivers', gl3, fl);
  } else R.setRibbons('rivers', null, null);

  // Winds (sampled on the first 2,562 cells = the level-4 sub-grid)
  if (S.overlays.wind) {
    const w = new Float32Array(await api.bin('wind', { month: S.month ?? 0 }));
    const L = new Lines();
    const m = Math.min(g.n, 2562);
    // Winds arrive in m/s; ~8 m/s draws about as long as a grid-4 cell spacing.
    for (let i = 0; i < m; i++) arrow(L, g.p(i), w[i * 2] * 0.0045, w[i * 2 + 1] * 0.0045, [1, 1, 1, 0.75]);
    const [a, b] = L.sets();
    R.setOverlay('wind', a, b);
  } else R.setOverlay('wind', null, null);

  // Plate motion
  const ve = S.overlays.motion ? await getField('vel_e') : null;
  const vn = ve ? await getField('vel_n') : null;
  if (ve && vn) {
    const L = new Lines();
    const m = Math.min(g.n, 2562);
    for (let i = 0; i < m; i++) arrow(L, g.p(i), ve.values[i] * 0.0006, vn.values[i] * 0.0006, [1, 0.95, 0.5, 0.9]);
    const [a, b] = L.sets();
    R.setOverlay('motion', a, b);
  } else R.setOverlay('motion', null, null);

  // Political borders, drawn along cell edges: province hairlines in the
  // state and province layers, state (or region) borders as 2 px ribbons, and
  // state borders over any other layer with the Borders overlay.
  const lay = S.layer;
  const political = lay === 'states' || lay === 'provinces' || lay === 'regions';
  const cultural = lay === 'cultures' || lay === 'culture_groups';
  const stf = cultural ? await getField('culture_group') : political || S.overlays.borders ? await getField('state') : null;
  if (stf && !stf.stale) {
    const L = new Lines();
    // Culture layers: culture borders as hairlines, group borders as ribbons.
    if (lay === 'cultures') {
      const cu = await getField('culture');
      if (cu) for (const [a, b] of cellEdges(g, cu.values, (x, y) => x > 0 && y > 0)) L.seg(a, b, [0.05, 0.05, 0.05, 0.45], 0.0024);
    }
    if (lay === 'states' || lay === 'provinces') {
      const pv = await getField('province'), pk = await getField('province_kind');
      if (pv && pk) {
        for (const [a, b, i, j] of cellEdges(g, pv.values, () => true)) {
          const sea = pk.values[i] >= 2 && pk.values[j] >= 2;
          L.seg(a, b, sea ? [0.65, 0.8, 1, 0.3] : [0.05, 0.05, 0.05, lay === 'provinces' ? 0.55 : 0.3], 0.0024);
        }
      }
    }
    const main = lay === 'regions' ? await getField('region') : stf;
    if (lay === 'regions') for (const [a, b] of cellEdges(g, stf.values, (x, y) => x > 0 && y > 0)) L.seg(a, b, [0.05, 0.05, 0.05, 0.35], 0.0024);
    const [la, lb] = L.sets();
    R.setOverlay('borders', la, lb);
    const segs = main ? cellEdges(g, main.values, (x, y) => x > 0 && y > 0) : [];
    const elev = (await getField('elevation'))?.values;
    const mk = (): RibbonSet => ({ a: new Float32Array(segs.length * 3), b: new Float32Array(segs.length * 3), width: new Float32Array(segs.length).fill(1.7), color: new Float32Array(segs.length * 4), count: segs.length });
    const gl3 = mk(), fl = mk();
    segs.forEach(([a, b, i, j], k) => {
      const lift = 0.0026 + Math.max(exagLift(i, elev), exagLift(j, elev));
      gl3.a.set(surface(a, lift), k * 3); gl3.b.set(surface(b, lift), k * 3);
      const [la1, lo1] = toLatLon(a);
      let [la2, lo2] = toLatLon(b);
      if (lo2 - lo1 > Math.PI) lo2 -= 2 * Math.PI;
      if (lo2 - lo1 < -Math.PI) lo2 += 2 * Math.PI;
      fl.a.set([lo1, la1, 0], k * 3); fl.b.set([lo2, la2, 0], k * 3);
      gl3.color.set([0.06, 0.06, 0.06, 0.92], k * 4); fl.color.set([0.06, 0.06, 0.06, 0.92], k * 4);
    });
    R.setRibbons('borders', gl3, fl);
  } else {
    R.setOverlay('borders', null, null);
    R.setRibbons('borders', null, null);
  }

  drawEditMarkers();
}

/** Cell-edge segments between neighbouring cells whose values differ (and pass `keep`):
 *  [start, end, cell i, cell j], each edge running between the two triangle centres. */
function cellEdges(g: Grid, v: ArrayLike<number>, keep: (a: number, b: number) => boolean): [Vec3, Vec3, number, number][] {
  const out: [Vec3, Vec3, number, number][] = [];
  const isNb = (a: number, b: number) => { for (let k = g.off[a]; k < g.off[a + 1]; k++) if (g.nbr[k] === b) return true; return false; };
  const mid = (a: number, b: number, c: number): Vec3 => {
    const [p, q, r] = [g.p(a), g.p(b), g.p(c)];
    const s: Vec3 = [p[0] + q[0] + r[0], p[1] + q[1] + r[1], p[2] + q[2] + r[2]];
    const l = Math.hypot(...s);
    return [s[0] / l, s[1] / l, s[2] / l];
  };
  for (let i = 0; i < g.n; i++) {
    for (let k = g.off[i]; k < g.off[i + 1]; k++) {
      const j = g.nbr[k];
      if (j < i || v[j] === v[i] || !keep(v[i], v[j])) continue;
      let c1 = -1, c2 = -1;
      for (let kk = g.off[i]; kk < g.off[i + 1]; kk++) {
        const c = g.nbr[kk];
        if (c !== j && isNb(j, c)) { if (c1 < 0) c1 = c; else c2 = c; }
      }
      if (c2 >= 0) out.push([mid(i, j, c1), mid(i, j, c2), i, j]);
    }
  }
  return out;
}

function drawEditMarkers() {
  const st = S.status!;
  if (!S.overlays.edits) {
    R.setOverlay('edits', null, null);
    return;
  }
  const L = new Lines();
  for (const pin of st.edits.pins) {
    const p = fromLatLon((pin.lat * Math.PI) / 180, (pin.lon * Math.PI) / 180);
    const [e, n] = eastNorth(p);
    const r = 0.02;
    const col: [number, number, number, number] = pin.kind === 'oceanic' ? [0.4, 0.8, 1, 1] : pin.kind === 'continental' ? [1, 0.7, 0.3, 1] : [1, 1, 1, 1];
    for (let k = 0; k < 12; k++) {
      const a1 = (k / 12) * 2 * Math.PI, a2 = ((k + 1) / 12) * 2 * Math.PI;
      L.seg(offset(p, e, n, Math.cos(a1) * r, Math.sin(a1) * r), offset(p, e, n, Math.cos(a2) * r, Math.sin(a2) * r), col, 0.004);
    }
    L.seg(offset(p, e, n, -r * 0.5, 0), offset(p, e, n, r * 0.5, 0), col, 0.004);
    L.seg(offset(p, e, n, 0, -r * 0.5), offset(p, e, n, 0, r * 0.5), col, 0.004);
  }
  for (const s of st.edits.sites ?? []) {
    const p = fromLatLon((s.lat * Math.PI) / 180, (s.lon * Math.PI) / 180);
    const [e, n] = eastNorth(p);
    const r = 0.018;
    const col: [number, number, number, number] = [1, 0.25, 0.8, 1];
    const pts = [[0, r], [r, 0], [0, -r], [-r, 0], [0, r]];
    for (let k = 0; k < 4; k++) L.seg(offset(p, e, n, pts[k][0], pts[k][1]), offset(p, e, n, pts[k + 1][0], pts[k + 1][1]), col, 0.004);
  }
  // Founding-band pins (filled, size by bands) and, without pins, where the
  // random founders started (hollow).
  const ring = (p: Vec3, r: number, col: [number, number, number, number], fill: boolean) => {
    const [e, n] = eastNorth(p);
    for (let k = 0; k < 16; k++) {
      const a1 = (k / 16) * 2 * Math.PI, a2 = ((k + 1) / 16) * 2 * Math.PI;
      L.seg(offset(p, e, n, Math.cos(a1) * r, Math.sin(a1) * r), offset(p, e, n, Math.cos(a2) * r, Math.sin(a2) * r), col, 0.004);
      if (fill) L.seg(p, offset(p, e, n, Math.cos(a1) * r, Math.sin(a1) * r), col, 0.004);
    }
  };
  for (const b of st.edits.band_pins ?? []) ring(fromLatLon((b.lat * Math.PI) / 180, (b.lon * Math.PI) / 180), 0.01 + 0.004 * Math.sqrt(b.bands), [1, 0.85, 0.2, 1], true);
  if (!(st.edits.band_pins ?? []).length) {
    const founders = st.steps.find((s) => s.key === 'cultures')?.meta?.founders ?? [];
    for (const f of founders) ring(fromLatLon((f.lat * Math.PI) / 180, (f.lon * Math.PI) / 180), 0.012, [1, 0.85, 0.2, 0.7], false);
  }
  for (const a of st.edits.arrows) {
    const p = fromLatLon((a.lat * Math.PI) / 180, (a.lon * Math.PI) / 180);
    const b = (a.bearing_deg * Math.PI) / 180;
    const len = 0.05 + 0.0008 * a.speed_mm_yr;
    arrow(L, p, Math.sin(b) * len, Math.cos(b) * len, [1, 0.35, 0.3, 1]);
  }
  const [x, y] = L.sets();
  R.setOverlay('edits', x, y);
}

function drawBrush(p: Vec3 | null, trail: Vec3[] = []) {
  const tool = TOOLS.find((t) => t.id === S.tool)!;
  if (!p || S.tool === 'navigate' || tool.value === 'pin' || S.tool === 'arrow') {
    R.setOverlay('brush', null, null);
    return;
  }
  const L = new Lines();
  const r = S.brush.radius_km / S.status!.params.planet.radius_km;
  const [e, n] = eastNorth(p);
  const col: [number, number, number, number] = [1, 1, 1, 0.9];
  const pts: Vec3[] = [];
  for (let k = 0; k <= 48; k++) {
    const a = (k / 48) * 2 * Math.PI;
    const q: Vec3 = [
      p[0] * Math.cos(r) + (e[0] * Math.cos(a) + n[0] * Math.sin(a)) * Math.sin(r),
      p[1] * Math.cos(r) + (e[1] * Math.cos(a) + n[1] * Math.sin(a)) * Math.sin(r),
      p[2] * Math.cos(r) + (e[2] * Math.cos(a) + n[2] * Math.sin(a)) * Math.sin(r),
    ];
    pts.push(q);
  }
  for (let k = 0; k < 48; k++) L.seg(pts[k], pts[k + 1], col, 0.006);
  if (tool.scatter) {
    const rr = r * (1 + S.brush.scatter);
    const ring = (a: number): Vec3 => [
      p[0] * Math.cos(rr) + (e[0] * Math.cos(a) + n[0] * Math.sin(a)) * Math.sin(rr),
      p[1] * Math.cos(rr) + (e[1] * Math.cos(a) + n[1] * Math.sin(a)) * Math.sin(rr),
      p[2] * Math.cos(rr) + (e[2] * Math.cos(a) + n[2] * Math.sin(a)) * Math.sin(rr),
    ];
    for (let k = 0; k < 48; k += 2) L.seg(ring((k / 48) * 2 * Math.PI), ring(((k + 1) / 48) * 2 * Math.PI), [1, 1, 1, 0.4], 0.006);
  }
  for (let k = 1; k < trail.length; k++) L.seg(trail[k - 1], trail[k], [1, 0.85, 0.3, 0.9], 0.006);
  const [a, b] = L.sets();
  R.setOverlay('brush', a, b);
}

// ------------------------------------------------------------------ running

async function run(to: string) {
  if (S.busy) return;
  S.busy = true;
  renderSteps();
  const prog = $('#progress');
  const poll = setInterval(async () => {
    try {
      const p = await api.json('progress');
      if (p.running) {
        const st = STEPS.find((s) => s.key === p.step);
        prog.innerHTML = '';
        prog.append(h('span', {}, `${st ? st.title : p.step}: ${p.msg}`), h('progress', { max: 1, value: p.frac }));
      }
    } catch { /* ignore */ }
  }, 150);
  try {
    const st = await api.json<Status & { ran: string[] }>('run', { to });
    invalidate();
    S.busy = false;
    const total = st.steps.filter((s) => st.ran.includes(s.key)).reduce((a, s) => a + s.millis, 0);
    if (st.ran.length) toast(`Ran ${st.ran.length} step${st.ran.length > 1 ? 's' : ''} in ${(total / 1000).toFixed(1)} s`);
    await refreshStatus(st);
  } catch (e) {
    toast(String(e), true);
  } finally {
    clearInterval(poll);
    S.busy = false;
    prog.textContent = '';
    renderSteps();
  }
}

async function mutate(cmd: string, args: any, autoStep?: string) {
  try {
    const st = await api.json<Status>(cmd, args);
    S.status = st;
    if (autoStep && S.autoRun) {
      await run(autoStep);
    } else {
      invalidate();
      await refreshStatus(st);
    }
  } catch (e) {
    toast(String(e), true);
  }
}

let paramTimer: number | undefined;
function setParam(group: string, key: string, value: number) {
  const params = structuredClone(S.status!.params);
  params[group][key] = value;
  S.status!.params = params;
  clearTimeout(paramTimer);
  paramTimer = window.setTimeout(async () => {
    const st = await api.json<Status>('set_params', { params });
    const levelChanged = S.grid && st.grid.cells !== S.grid.n;
    if (levelChanged) invalidate();
    await refreshStatus(st);
  }, 250);
}

// ------------------------------------------------------------------ top bar

function renderTopbar() {
  const st = S.status!;
  const bar = $('#topbar');
  bar.innerHTML = '';
  const name = st.path ? st.path.split(/[\\/]/).filter(Boolean).pop() : 'Untitled world';
  const btn = (label: string, on: () => void, opts: Record<string, any> = {}) => h('button', { onclick: on, ...opts }, label);
  const layerSel = h('select', { title: 'Map layer', onchange: (e: Event) => setLayer((e.target as HTMLSelectElement).value as LayerId) });
  let lastGroup = '';
  let og: HTMLElement | null = null;
  for (const l of LAYERS) {
    if (l.group !== lastGroup) {
      og = h('optgroup', { label: l.group });
      layerSel.append(og);
      lastGroup = l.group;
    }
    og!.append(h('option', { value: l.id, selected: l.id === S.layer }, l.label));
  }
  const def = LAYERS.find((l) => l.id === S.layer)!;
  const monthSel = h('select', { title: 'Month', disabled: !def.monthly && !S.overlays.wind, onchange: (e: Event) => {
    const v = (e.target as HTMLSelectElement).value;
    S.month = v === '' ? null : Number(v);
    refreshLayer();
  } }, h('option', { value: '', selected: S.month === null }, 'Annual'), ...MONTHS.map((m, i) => h('option', { value: i, selected: S.month === i }, m)));
  const ov = (key: keyof typeof S.overlays, label: string) =>
    h('label', { class: 'check' }, h('input', { type: 'checkbox', checked: S.overlays[key], onchange: (e: Event) => {
      S.overlays[key] = (e.target as HTMLInputElement).checked;
      renderTopbar();
      refreshOverlays();
    } }), label);
  bar.append(
    h('div', { class: 'brand' }, h('span', { class: 'logo' }), h('b', {}, 'Fantasy World Maker'), h('span', { class: 'proj', title: st.path ?? '' }, name + (st.dirty ? ' •' : ''))),
    h('div', { class: 'group' },
      btn('New', onNew), btn('Open…', onOpen), btn('Save', onSave, { title: 'Ctrl+S' }), btn('Save as…', () => onSave(true)), btn('Export…', onExport)),
    h('div', { class: 'group' },
      btn('Undo', () => mutate('undo', {}, undoTarget()), { disabled: !st.can_undo, title: 'Ctrl+Z' }),
      btn('Redo', () => mutate('redo', {}, undoTarget()), { disabled: !st.can_redo, title: 'Ctrl+Y' })),
    h('div', { class: 'group seg' },
      btn('Globe', () => setView(0), { class: R.mode === 0 ? 'on' : '' }),
      btn('Flat map', () => setView(1), { class: R.mode === 1 ? 'on' : '' })),
    h('div', { class: 'group' }, layerSel, monthSel),
    h('div', { class: 'group' }, ov('rivers', 'Rivers'), ov('borders', 'Borders'), ov('wind', 'Wind'), ov('motion', 'Plate motion'), ov('edits', 'Pins'),
      h('label', { class: 'check', title: 'Relief exaggeration on the globe' }, 'Relief',
        h('input', { type: 'range', min: 0, max: 40, step: 1, value: S.exag, oninput: (e: Event) => { S.exag = Number((e.target as HTMLInputElement).value); refreshLayer(); } }))),
    h('div', { class: 'spacer' }),
    btn('Edit map', () => {
      S.editor = !S.editor;
      if (!S.editor && EDITOR_TOOLS.includes(S.tool)) S.tool = 'navigate';
      renderTopbar();
      renderToolbar();
    }, { class: S.editor ? 'on' : '', title: 'Map editor: merge, move and rename provinces and states, paint trade goods' }),
    stageButtons(st),
  );
}

/** "Next stage" (the first stage not fully generated), one button per stage, and "Generate all". */
function stageButtons(st: Status): HTMLElement {
  const done = (step: string) => st.steps.find((s) => s.key === step)?.state === 'done';
  const next = STAGE_ENDS.find((s) => !done(s.step));
  return h('div', { class: 'group' },
    next ? h('button', { class: 'primary', disabled: S.busy, title: `Generate everything up to the end of Stage ${next.stage}`, onclick: () => run(next.step) }, `Next: Stage ${next.stage}`) : null,
    ...STAGE_ENDS.map((s) => h('button', { disabled: S.busy, class: done(s.step) ? 'done' : '', title: `Generate up to the end of Stage ${s.stage}`, onclick: () => run(s.step) }, done(s.step) ? `Stage ${s.stage} ✓` : `Stage ${s.stage}`)),
    h('button', { class: next ? '' : 'primary', disabled: S.busy, onclick: () => run(STAGE_ENDS[STAGE_ENDS.length - 1].step) }, 'Generate all'));
}

function undoTarget(): string | undefined {
  // Re-run the earliest stale step that is cheap to refresh.
  const st = S.status!;
  const firstStale = st.steps.find((s) => s.state !== 'done');
  return firstStale && ['sketch'].includes(firstStale.key) ? 'sketch' : undefined;
}

function setView(m: 0 | 1) {
  R.mode = m;
  R.requestDraw();
  renderTopbar();
}

function setLayer(id: LayerId) {
  S.layer = id;
  renderTopbar();
  refreshLayer();
}

async function onNew() {
  if (S.status?.dirty && !confirm('Discard unsaved changes and start a new world?')) return;
  const st = await api.json<Status>('new', {});
  invalidate();
  S.grid = null;
  await refreshStatus(st);
  await run('sketch');
}

async function onOpen() {
  const path = await api.pickFolder('Open world project folder', false);
  if (!path) return;
  try {
    const st = await api.json<Status>('open', { path });
    invalidate();
    S.grid = null;
    await refreshStatus(st);
    toast(`Opened ${path}`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function onSave(as = false) {
  let path: string | null = null;
  if (as || !S.status!.path) {
    path = await api.pickFolder('Save world project as folder', true);
    if (!path) return;
  }
  try {
    const st = await api.json<Status>('save', { path });
    S.status = st;
    renderTopbar();
    toast(`Saved to ${st.path}`);
  } catch (e) {
    toast(String(e), true);
  }
}

function onExport() {
  const st = S.status!;
  const o = { width: 8192, height: 4096, lat_min: -90, lat_max: 90, sea_level_value: 20, max_elevation_m: 6500, max_depth_m: 6000, detail_noise: true };
  const num = (k: keyof typeof o, label: string, step = 1) =>
    h('label', {}, h('span', {}, label), h('input', { type: 'number', value: o[k] as number, step, onchange: (e: Event) => ((o as any)[k] = Number((e.target as HTMLInputElement).value)) }));
  const dirInput = h('input', { type: 'text', value: st.path ? `${st.path}${st.path.includes('\\') ? '\\' : '/'}export` : '', placeholder: 'Export folder' }) as HTMLInputElement;
  const result = h('div', { class: 'result' });
  const dlg = h('div', { class: 'modal-back' },
    h('div', { class: 'modal' },
      h('h2', {}, 'Export map package'),
      h('p', { class: 'muted' }, 'Equirectangular PNG layers following CK3 / Victoria 3 conventions: heightmap, terrain, rivers, biomes and climate, plus provinces.png, definition.csv and the state, region and strait tables. Detail noise is added at this step only.'),
      h('div', { class: 'form' },
        num('width', 'Width (px)'), num('height', 'Height (px)'), num('lat_min', 'South edge (°)', 0.5), num('lat_max', 'North edge (°)', 0.5),
        num('sea_level_value', 'Heightmap sea level value'), num('max_elevation_m', 'Height mapped to 255 (m)', 100),
        h('label', {}, h('span', {}, 'Detail noise'), h('input', { type: 'checkbox', checked: true, onchange: (e: Event) => (o.detail_noise = (e.target as HTMLInputElement).checked) })),
        h('label', { class: 'wide' }, h('span', {}, 'Folder'), h('div', { class: 'row' }, dirInput,
          h('button', { onclick: async () => { const p = await api.pickFolder('Export folder', false); if (p) dirInput.value = p; } }, 'Browse…'))),
      ),
      result,
      h('div', { class: 'actions' },
        h('button', { onclick: () => dlg.remove() }, 'Close'),
        h('button', { class: 'primary', onclick: async (e: Event) => {
          const b = e.target as HTMLButtonElement;
          b.disabled = true;
          result.textContent = 'Generating missing steps…';
          try {
            await run('provinces');
            result.textContent = 'Rasterising…';
            const r = await api.json('export', { path: dirInput.value || null, options: o });
            result.innerHTML = '';
            result.append(h('b', {}, `Exported ${r.files.length} files (${r.width}×${r.height}) in ${(r.millis / 1000).toFixed(1)} s`), h('div', { class: 'muted' }, r.dir), h('div', { class: 'files' }, r.files.join(' · ')));
          } catch (err) {
            result.textContent = String(err);
          }
          b.disabled = false;
        } }, 'Export'),
      ),
    ),
  );
  document.body.append(dlg);
}

// ------------------------------------------------------------------ step cards

function renderSteps() {
  const st = S.status;
  if (!st) return;
  const box = $('#steps');
  const scroll = box.scrollTop;
  box.innerHTML = '';
  box.append(h('div', { class: 'steps-head' }, h('b', {}, 'Stage 1 · Make the planet'),
    h('label', { class: 'check', title: 'Re-run the edited step automatically after each brush stroke' },
      h('input', { type: 'checkbox', checked: S.autoRun, onchange: (e: Event) => (S.autoRun = (e.target as HTMLInputElement).checked) }), 'Auto-update')));
  STEPS.forEach((ui, idx) => {
    if (idx === STAGE2_START) box.append(h('div', { class: 'steps-head stage' }, h('b', {}, 'Stage 2 · States and provinces')));
    if (idx === STAGE3_START) box.append(h('div', { class: 'steps-head stage' }, h('b', {}, 'Stage 3 · Cultures')));
    box.append(stepCard(ui, idx, st.steps[idx]));
  });
  box.scrollTop = scroll;
}

function stepCard(ui: StepUI, idx: number, ss: StepStatus): HTMLElement {
  const open = S.openStep === ui.key;
  const pill = h('span', { class: `pill ${ss.state}` }, ss.state === 'done' ? 'done' : ss.state === 'stale' ? 'stale' : '—');
  const head = h('div', { class: 'card-head', onclick: () => {
    S.openStep = open ? '' : ui.key;
    if (!open && ui.layer) {
      S.layer = ui.layer;
      if (ui.key === 'hydrology') S.overlays.rivers = true;
      refreshLayer();
      renderTopbar();
    }
    if (!open && ui.tools.length && !ui.tools.includes(S.tool)) S.tool = 'navigate';
    renderSteps();
    renderToolbar();
  } },
    h('span', { class: 'num' }, String(idx + 1)), h('span', { class: 'title' }, ui.title),
    ss.millis ? h('span', { class: 'ms' }, ss.millis < 1000 ? `${ss.millis} ms` : `${(ss.millis / 1000).toFixed(1)} s`) : null, pill);
  const card = h('section', { class: `card ${open ? 'open' : ''} ${ss.state}` }, head);
  if (!open) return card;

  const body = h('div', { class: 'card-body' }, h('p', { class: 'muted' }, ui.blurb));
  const form = h('div', { class: 'form' });
  for (const p of ui.params) form.append(paramInput(p));
  body.append(form);

  // Step-specific extras.
  const e = S.status!.edits;
  if (ui.key === 'sketch') {
    body.append(
      h('label', { class: 'check' }, h('input', { type: 'checkbox', checked: e.auto_base, onchange: (ev: Event) => mutate('set_auto_base', { value: (ev.target as HTMLInputElement).checked }, 'sketch') }), 'Start from generated continents'),
      h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.sketch_strokes} strokes`),
        h('button', { class: 'small', disabled: !e.sketch_strokes && !e.pins.length && !e.arrows.length, onclick: () => confirm('Clear all sketch strokes, pins and arrows?') && mutate('clear_layer', { layer: 'sketch' }, 'sketch') }, 'Clear sketch')),
    );
  }
  if (ui.key === 'plates') {
    const list = h('div', { class: 'list' });
    e.pins.forEach((p, i) => list.append(h('div', { class: 'item' }, `Pin ${p.kind} · ${p.lat.toFixed(1)}°, ${p.lon.toFixed(1)}°`, h('button', { class: 'x', title: 'Remove', onclick: () => mutate('remove_pin', { index: i }) }, '×'))));
    e.arrows.forEach((a, i) => list.append(h('div', { class: 'item' }, `Arrow ${Math.round(a.bearing_deg)}° · ${a.speed_mm_yr} mm/yr`, h('button', { class: 'x', title: 'Remove', onclick: () => mutate('remove_arrow', { index: i }) }, '×'))));
    if (e.pins.length || e.arrows.length) body.append(h('h4', {}, 'Pins & arrows'), list);
    body.append(h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.plate_strokes} plate paint strokes`),
      h('button', { class: 'small', disabled: !e.plate_strokes, onclick: () => mutate('clear_layer', { layer: 'plates' }) }, 'Clear plate paint')));
  }
  if (ui.key === 'relief') {
    const imp = e.elevation_import;
    body.append(h('h4', {}, 'Imported heightmap'),
      imp
        ? h('div', { class: 'row' }, h('span', { class: 'muted', title: imp.path, style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1' }, imp.path.split(/[\\/]/).pop()),
            h('button', { class: 'small', onclick: () => importHeightmap(imp.path, imp.encoding) }, 'Reload'),
            h('button', { class: 'small', onclick: () => mutate('import_heightmap', { path: null }, 'relief') }, 'Remove'))
        : h('div', { class: 'row' }, h('span', { class: 'muted', style: 'flex:1' }, 'Replace the tectonic relief with an equirectangular PNG (e.g. an exported heightmap edited in GIMP).'),
            h('button', { class: 'small', onclick: () => importHeightmap() }, 'Import…')));
    body.append(h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.elevation_strokes} elevation edits`),
      h('button', { class: 'small', disabled: !e.elevation_strokes, onclick: () => mutate('clear_layer', { layer: 'elevation' }) }, 'Clear edits')));
  }
  if (ui.key === 'biomes') {
    body.append(h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.biome_strokes} biome paint strokes`),
      h('button', { class: 'small', disabled: !e.biome_strokes, onclick: () => mutate('clear_layer', { layer: 'biomes' }) }, 'Clear paint')));
  }
  if (ui.key === 'habitability') {
    body.append(h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.barrier_strokes} barrier strokes`),
      h('button', { class: 'small', disabled: !e.barrier_strokes, onclick: () => mutate('clear_layer', { layer: 'barriers' }) }, 'Clear barriers')));
    body.append(h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.site_pins} site pins`),
      h('button', { class: 'small', disabled: !e.site_pins, onclick: () => mutate('clear_layer', { layer: 'sites' }) }, 'Clear pins')));
  }
  if (ui.key === 'states') {
    body.append(h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.state_strokes} state paint strokes`),
      h('button', { class: 'small', disabled: !e.state_strokes, onclick: () => mutate('clear_layer', { layer: 'states' }) }, 'Clear paint')));
    body.append(h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.fertility_strokes} fertility strokes`),
      h('button', { class: 'small', disabled: !e.fertility_strokes, onclick: () => mutate('clear_layer', { layer: 'fertility' }, 'habitability') }, 'Clear fertility')));
  }
  if (ui.key === 'cultures') {
    const pins = e.band_pins ?? [];
    const founders = S.status!.steps.find((x) => x.key === 'cultures')?.meta?.founders ?? [];
    body.append(h('h4', {}, 'Founding peoples'),
      h('div', { class: 'row' }, h('span', { class: 'muted', style: 'flex:1' }, pins.length
        ? `${pins.length} founding-band pins (${pins.reduce((a, b) => a + b.bands, 0)} bands) replace the random founders.`
        : `Random founders (hollow circles on the map). Pin them to move or resize them.`),
        !pins.length && founders.length ? h('button', { class: 'small', onclick: () => pinFounders(founders) }, 'Pin these') : null,
        h('button', { class: 'small', disabled: !pins.length, onclick: () => mutate('clear_layer', { layer: 'bands' }, 'cultures') }, 'Clear pins')));
  }
  if (ui.key === 'provinces') {
    const imp = e.province_import;
    body.append(h('h4', {}, 'Imported provinces'),
      imp
        ? h('div', { class: 'row' }, h('span', { class: 'muted', title: imp.png, style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1' }, imp.png.split(/[\\/]/).pop()),
            h('button', { class: 'small', onclick: () => importProvinces(imp.png, imp.csv) }, 'Reload'),
            h('button', { class: 'small', onclick: () => mutate('import_provinces', { png: null }, 'provinces') }, 'Remove'))
        : h('div', { class: 'row' }, h('span', { class: 'muted', style: 'flex:1' }, 'Replace the generated provinces with an edited provinces.png and definition.csv. Files are checked first; each cell then takes its province by majority pixel vote.'),
            h('button', { class: 'small', onclick: () => importProvinces() }, 'Import…')));
    body.append(h('div', { class: 'row' }, h('span', { class: 'muted' }, `${e.province_strokes} province paint strokes`),
      h('button', { class: 'small', disabled: !e.province_strokes, onclick: () => mutate('clear_layer', { layer: 'provinces' }) }, 'Clear paint')));
  }

  body.append(h('div', { class: 'actions' },
    h('button', { class: 'primary', disabled: S.busy || ss.state === 'done', onclick: () => run(ui.key) },
      S.busy ? 'Working…' : ss.state === 'done' ? 'Up to date' : idx === 0 ? 'Build grid' : `Run to step ${idx + 1}`)));
  if (ss.meta && ss.state !== 'empty') body.append(summary(ui.key, ss.meta));
  card.append(body);
  return card;
}

async function importHeightmap(path?: string, encoding?: any) {
  const p = path ?? (await api.pickPng('Import heightmap (equirectangular PNG)'));
  if (!p) return;
  let enc = encoding;
  if (!enc) {
    const kind = window.prompt("Height encoding: heightmap16 (this app's 16-bit export), paradox8 (8-bit export, sea level 20) or linear", 'heightmap16');
    if (!kind) return;
    if (kind === 'paradox8') enc = { kind: 'paradox8', sea_level_value: 20, max_elevation_m: 6500, max_depth_m: 6000 };
    else if (kind === 'linear') {
      const r = window.prompt('Linear range: black and white in metres', '-11000, 9000');
      if (!r) return;
      const [a, b] = r.split(',').map(Number);
      enc = { kind: 'linear', min_m: a, max_m: b };
    } else enc = { kind: 'heightmap16' };
  }
  await mutate('import_heightmap', { path: p, encoding: enc }, 'relief');
}

async function importProvinces(png?: string, csv?: string | null) {
  const p = png ?? (await api.pickPng('Import provinces.png (one colour per province)'));
  if (!p) return;
  const c = png ? csv ?? null : await api.pickFile('Its definition.csv (cancel to number the colours automatically)', 'definition.csv', ['csv']);
  try {
    const st = await api.json<Status & { report?: any }>('import_provinces', { png: p, csv: c });
    const w: string[] = st.report?.warnings ?? [];
    toast(w.length ? `Imported ${st.report.provinces} provinces with ${w.length} warning(s): ${w[0]}` : `Imported ${st.report?.provinces ?? ''} provinces`);
    S.status = st;
    if (S.autoRun) await run('provinces');
    else {
      invalidate();
      await refreshStatus(st);
    }
  } catch (err) {
    toast(String(err), true);
  }
}

function paramInput(p: Param): HTMLElement {
  const v = S.status!.params[p.group][p.key];
  const label = h('span', { title: p.hint ?? '' }, p.label);
  if (p.options) {
    return h('label', {}, label, h('select', { onchange: (e: Event) => setParam(p.group, p.key, Number((e.target as HTMLSelectElement).value)) },
      ...p.options.map(([val, l]) => h('option', { value: val, selected: val === v }, l))));
  }
  const range = p.max! - p.min! <= 100 && p.key !== 'seed';
  const num = h('input', { type: 'number', min: p.min, max: p.max, step: p.step, value: v }) as HTMLInputElement;
  const slider = range ? (h('input', { type: 'range', min: p.min, max: p.max, step: p.step, value: v }) as HTMLInputElement) : null;
  num.addEventListener('change', () => {
    if (slider) slider.value = num.value;
    setParam(p.group, p.key, Number(num.value));
  });
  slider?.addEventListener('input', () => {
    num.value = slider.value;
    setParam(p.group, p.key, Number(slider.value));
  });
  const extra = p.key === 'seed' ? h('button', { class: 'small', title: 'Random seed', onclick: () => {
    const s = Math.floor(Math.random() * 1e9);
    num.value = String(s);
    setParam('planet', 'seed', s);
  } }, 'Dice') : null;
  return h('label', { class: slider ? 'has-range' : '' }, label, h('div', { class: 'row' }, slider, num, extra));
}

/** Turn the random founders of the last run into pins (one band each), so they can be moved, resized or removed. */
async function pinFounders(founders: { lat: number; lon: number; bands: number }[]) {
  try {
    for (const f of founders) {
      await api.json('add_stroke', { stroke: { tool: 'band_pin', value: f.bands, radius_km: 100, points: [[f.lat, f.lon]] } });
    }
    const st = await api.json<Status>('status', {});
    S.status = st;
    invalidate();
    await refreshStatus(st);
    toast(`Pinned ${founders.length} founders`);
  } catch (err) {
    toast(String(err), true);
  }
}

function stat(k: string, v: string) {
  return h('div', { class: 'stat' }, h('span', {}, k), h('b', {}, v));
}

function summary(key: string, m: any): HTMLElement {
  const box = h('div', { class: 'summary' });
  const fmt = (x: number, d = 0) => x.toLocaleString(undefined, { maximumFractionDigits: d, minimumFractionDigits: d });
  switch (key) {
    case 'planet':
      box.append(stat('Cells', fmt(m.cells)), stat('Spacing', `${fmt(m.spacing_km)} km`), stat('Cell area', `${fmt(m.cell_area_km2)} km²`), stat('Surface', `${fmt(m.surface_mkm2)} M km²`));
      break;
    case 'sketch':
      box.append(stat('Land', `${fmt(m.land_fraction * 100, 1)} %`), stat('Land area', `${fmt(m.land_area_mkm2)} M km²`), stat('Strokes', String(m.strokes)));
      break;
    case 'plates': {
      box.append(stat('Plates', String(m.plate_count)), stat('Continental', String(m.continental_plates)), stat('Landmasses', String(m.landmasses)));
      const list = h('div', { class: 'plates' });
      for (const p of (m.plates ?? []).slice().sort((a: any, b: any) => b.area_mkm2 - a.area_mkm2).slice(0, 14)) {
        const c = plateColor(p.id).map(Math.round);
        list.append(h('div', { class: 'plate', title: 'Plate id for plate paint' },
          h('i', { style: `background: rgb(${c.join(',')})` }), `#${p.id}`, h('span', {}, `${fmt(p.area_mkm2, 1)} M km²`),
          h('span', {}, `${fmt(p.omega_deg_myr * 111 * S.status!.params.planet.radius_km / 6371)} mm/yr`), h('span', {}, p.continental_fraction > 0.5 ? 'cont.' : 'ocean')));
      }
      box.append(list);
      break;
    }
    case 'relief': {
      box.append(stat('Highest', `${fmt(m.max_elevation_m)} m`), stat('Deepest', `${fmt(m.min_elevation_m)} m`), stat('Hotspots', String((m.hotspots ?? []).length)));
      const b = m.boundary_km ?? {};
      const names: [string, number][] = [['continent_continent', 1], ['ocean_continent', 2], ['ocean_ocean', 3], ['ocean_rift', 4], ['continental_rift', 5], ['transform', 6]];
      const total = names.reduce((a, [k]) => a + (b[k] ?? 0), 0) || 1;
      box.append(h('div', { class: 'bars' }, ...names.map(([k, i]) => bar(BOUNDARY[i][0], (b[k] ?? 0) / total, BOUNDARY[i][1], `${fmt((b[k] ?? 0) / 1000)}k km`))));
      break;
    }
    case 'climate':
      box.append(stat('Global mean', `${fmt(m.global_mean_temp_c, 1)} °C`), stat('Precipitation', `${fmt(m.global_precip_mm)} mm/yr`),
        stat('Hadley edge', `${fmt(m.hadley_edge_deg)}°`), stat('Ferrel edge', `${fmt(m.ferrel_edge_deg)}°`));
      if (m.zonal) box.append(zonalChart(m.zonal));
      break;
    case 'hydrology': {
      if (m.channel_loss_m3s !== undefined) box.append(stat('Lost in dry channels', `${fmt(m.channel_loss_m3s)} m³/s`));
      box.append(stat('River cells', fmt(m.river_cells)), stat('Lakes', `${m.lakes} (${m.endorheic_lakes} endorheic)`), stat('Eroded', `${fmt(m.eroded_km3 / 1000)}k km³`));
      const list = h('div', { class: 'list' });
      (m.major_rivers ?? []).forEach((r: any, i: number) => list.append(h('div', { class: 'item link', onclick: () => R.lookAt(fromLatLon((r.lat * Math.PI) / 180, (r.lon * Math.PI) / 180)) },
        `${i + 1}. ${fmt(r.discharge_m3s)} m³/s · ${fmt(r.width_m ?? 0)} m wide · ${fmt(r.length_km)} km · basin ${fmt(r.basin_mkm2, 2)} M km²`)));
      box.append(h('h4', {}, 'Largest rivers (click to view)'), list);
      if (m.largest_lakes?.length) {
        const ll = h('div', { class: 'list' });
        for (const l of m.largest_lakes) {
          ll.append(h('div', { class: 'item link', title: `Inflow ${fmt(l.inflow_m3s)} m³/s (rivers and runoff, plus rain on the lake), evaporation ${fmt(l.evaporation_m3s)} m³/s, outflow ${fmt(l.outflow_m3s)} m³/s`,
            onclick: () => R.lookAt(fromLatLon((l.lat * Math.PI) / 180, (l.lon * Math.PI) / 180)) },
            h('span', {}, `${l.class} · ${fmt(l.area_km2)} km² · ${fmt(l.level_m)} m`),
            h('span', { class: 'muted' }, `in ${fmt(l.inflow_m3s)} / evap ${fmt(l.evaporation_m3s)} / out ${fmt(l.outflow_m3s)}`)));
        }
        box.append(h('h4', {}, 'Largest lakes · water balance in m³/s (click to view)'), ll);
      }
      if (m.climate_feedback) box.append(stat('Climate with lakes', `${fmt(m.climate_feedback.global_mean_temp_c, 1)} °C, ${fmt(m.climate_feedback.global_precip_mm)} mm/yr`));
      box.append(stat('Lake area', `${fmt(m.lake_area_km2 ?? 0)} km²`), stat('Inland basins', String(m.inland_seas ?? 0)));
      break;
    }
    case 'biomes': {
      const ks = Object.entries(m.koppen_share ?? {}).sort((a: any, b: any) => b[1] - a[1]).slice(0, 10) as [string, number][];
      box.append(h('h4', {}, 'Köppen share of land'), h('div', { class: 'bars' }, ...ks.map(([k, v]) => {
        const e = KOPPEN.find((x) => x[0] === k)!;
        return bar(`${k} ${e[2]}`, v, e[1], `${fmt(v * 100, 1)} %`);
      })));
      const ts = Object.entries(m.terrain_share ?? {}).sort((a: any, b: any) => b[1] - a[1]).slice(0, 8) as [string, number][];
      box.append(h('h4', {}, 'Terrain'), h('div', { class: 'bars' }, ...ts.map(([k, v]) => bar(k, v, TERRAIN.find((x) => x[0] === k)![1], `${fmt(v * 100, 1)} %`))));
      break;
    }
    case 'habitability':
      box.append(stat('Mean habitability', fmt(m.mean_habitability, 2)), stat('Habitable land (≥ 0.4)', `${fmt(m.habitable_share * 100)} %`),
        stat('Border rivers', `${fmt(m.border_river_km)} km`), stat('Backbone rivers', `${fmt(m.backbone_river_km)} km`));
      if (m.springs !== undefined) box.append(stat('Springs in dry land', fmt(m.springs)));
      if (m.painted_cells) box.append(stat('Painted cells', fmt(m.painted_cells)));
      break;
    case 'states': {
      box.append(stat('States', `${fmt(m.states)} (target ${fmt(m.target_states)})`), stat('Regions', fmt(m.regions)), stat('Continents', fmt(m.continents)),
        stat('Mean area', `${fmt(m.mean_area_km2 / 1000)}k km²`), stat('Range', `${fmt(m.smallest_km2 / 1000)}k – ${fmt(m.largest_km2 / 1000)}k km²`),
        stat('Islands joined across water', fmt(m.attached_islands)));
      const list = h('div', { class: 'list' });
      box.append(h('h4', {}, 'Continents · most populous states (click to view)'), list);
      political().then((P) => {
        if (!P) return;
        for (const c of P.continents) list.append(h('div', { class: 'item' }, h('b', {}, c.name), h('span', { class: 'muted' }, `${c.states} states · ${c.regions} regions · ${fmt(c.area_km2 / 1e6, 1)} M km²`)));
        const top = [...P.states].sort((a, b) => b.capacity_km2 - a.capacity_km2).slice(0, 10);
        for (const s of top) {
          const c = stateColor(s.id).map(Math.round);
          const r = P.byRegion.get(s.region);
          list.append(h('div', { class: 'item link', onclick: () => R.lookAt(fromLatLon((s.capital[0] * Math.PI) / 180, (s.capital[1] * Math.PI) / 180)) },
            h('span', {}, h('i', { class: 'swatch', style: `background: rgb(${c.join(',')})` }), `${s.name}`),
            h('span', { class: 'muted' }, `${r ? r.name + ' · ' : ''}${fmt(s.area_km2 / 1000)}k km² · habitability ${fmt(s.habitability, 2)}`)));
        }
      });
      break;
    }
    case 'cultures': {
      box.append(stat('Cultures', `${fmt(m.cultures)} in ${fmt(m.groups)} groups`), stat('Population', `${fmt(m.population / 1e6, 1)} M of ${fmt(m.capacity / 1e6, 1)} M possible`),
        stat('History', `${fmt(m.years)} years: ${fmt(m.splits)} splits, ${fmt(m.merged)} merges, ${fmt(m.extinct)} died out`), stat('Bands', fmt(m.bands)));
      if (m.desert_towns !== undefined) {
        const by = Object.entries(m.desert_towns_by_cause ?? {}).map(([k, v]) => `${v} ${k}`).join(', ');
        box.append(stat('Desert towns (dry, no river, ≥ 10,000 people)', `${fmt(m.desert_towns)}${by ? ' — ' + by : ''}`));
      }
      const list = h('div', { class: 'list' });
      box.append(list);
      political().then((P) => {
        if (!P || !P.cultures.length) return;
        const yr = (y: number) => `year ${fmt(y)}`;
        for (const g of P.groups.slice(0, 12)) {
          const gc = groupColor(g.id).map(Math.round);
          list.append(h('div', { class: 'item' }, h('b', {}, h('i', { class: 'swatch', style: `background: rgb(${gc.join(',')})` }), g.name),
            h('span', { class: 'muted' }, `${fmt(g.population / 1e6, 1)} M people`)));
          const members = (g.cultures as number[]).map((c) => P.byCulture.get(c)).filter(Boolean).sort((a, b) => b.population - a.population);
          for (const c of members) {
            const cc = cultureColor(c.id, c.group).map(Math.round);
            const parent = c.parent ? P.byCulture.get(c.parent) : null;
            const origin = parent ? `Split from ${parent.name} in ${yr(c.founded_year)}` : `Emerged in ${yr(c.founded_year)}`;
            list.append(h('div', { class: 'item', title: origin },
              h('span', {}, '\u00a0\u00a0', h('i', { class: 'swatch', style: `background: rgb(${cc.join(',')})` }), c.name),
              h('span', { class: 'muted' }, `${fmt(c.population / 1e6, 2)} M · ${c.provinces} prov.${parent ? ' · from ' + parent.name : ''}`)));
          }
        }
        const recent = P.events.filter((e) => e.kind !== 'emerged').slice(-8).reverse();
        if (recent.length) list.append(h('div', { class: 'item' }, h('b', {}, 'Latest events')));
        for (const e of recent) {
          const a = P.byCulture.get(e.culture)?.name ?? `#${e.culture}`, b = P.byCulture.get(e.other)?.name ?? '';
          const text = e.kind === 'split' ? `${a} split from ${b}` : e.kind === 'merged' ? `${a} merged into ${b}` : `${a} died out`;
          list.append(h('div', { class: 'item' }, h('span', {}, text), h('span', { class: 'muted' }, yr(e.year))));
        }
      });
      break;
    }
    case 'provinces': {
      box.append(stat('Provinces', fmt(m.provinces)), stat('Land', fmt(m.land)), stat('Wasteland', fmt(m.wasteland)), stat('Lakes', fmt(m.lakes)),
        stat('Sea zones', fmt(m.sea)), stat('Median land province', `${fmt(m.median_land_km2 / 1000, 1)}k km²`), stat('Strait crossings', fmt(m.straits)));
      if (m.borders) {
        const b = m.borders;
        box.append(stat('Land borders', `${fmt(b.land ?? 0)} open · ${fmt(b.river ?? 0)} river · ${fmt(b.impassable ?? 0)} impassable`),
          stat('Coast and sea borders', `${fmt(b.coast ?? 0)} coast · ${fmt(b.sea ?? 0)} sea · ${fmt(b.lake ?? 0)} lake`));
      }
      if (m.import?.error) box.append(h('div', { class: 'error' }, `Import failed, generated provinces shown: ${m.import.error}`));
      else if (m.import) {
        box.append(stat('Imported from', String(m.import.png).split(/[\\/]/).pop() ?? ''));
        if (m.import.changed_since_import) box.append(h('div', { class: 'error' }, 'The file changed since it was imported; press Reload to check it again.'));
        for (const w of m.import.warnings ?? []) box.append(h('div', { class: 'muted warn' }, `⚠ ${w}`));
      }
      break;
    }
  }
  return box;
}

function bar(label: string, frac: number, color: number[], value: string) {
  return h('div', { class: 'bar' }, h('span', { class: 'bl' }, label),
    h('span', { class: 'bt' }, h('i', { style: `width:${Math.max(1, frac * 100)}%; background: rgb(${color.map(Math.round).join(',')})` })), h('span', { class: 'bv' }, value));
}

function zonalChart(z: any[]): HTMLElement {
  const W = 300, H = 150, pl = 30, pb = 18;
  const x = (lat: number) => pl + ((lat + 90) / 180) * (W - pl - 6);
  const y = (t: number) => 6 + (1 - (t + 50) / 90) * (H - pb - 6);
  const path = (k: string) => z.map((d, i) => `${i ? 'L' : 'M'}${x(d.lat).toFixed(1)},${y(d[k]).toFixed(1)}`).join('');
  const svg = `<svg viewBox="0 0 ${W} ${H}" class="chart" role="img" aria-label="Zonal temperature">
    ${[-40, -20, 0, 20, 40].map((t) => `<line x1="${pl}" x2="${W - 6}" y1="${y(t)}" y2="${y(t)}" class="grid"/><text x="${pl - 4}" y="${y(t) + 3}" text-anchor="end">${t}</text>`).join('')}
    ${[-90, -45, 0, 45, 90].map((l) => `<text x="${x(l)}" y="${H - 4}" text-anchor="middle">${l}°</text>`).join('')}
    <path d="${path('t_land_jan')}" class="l jan"/><path d="${path('t_land_jul')}" class="l jul"/>
    <path d="${path('t_annual')}" class="l ann"/>
  </svg>`;
  const d = h('div', { class: 'chartbox' });
  d.innerHTML = svg;
  d.append(h('div', { class: 'keys' }, h('span', { class: 'k ann' }, 'Annual (zonal)'), h('span', { class: 'k jan' }, 'Land, January'), h('span', { class: 'k jul' }, 'Land, July')));
  return d;
}

// ------------------------------------------------------------------ toolbar & legend

function renderToolbar() {
  const tb = $('#toolbar');
  tb.innerHTML = '';
  const open = STEPS.find((s) => s.key === S.openStep);
  const ids: ToolId[] = ['navigate', ...(S.editor ? EDITOR_TOOLS : open?.tools ?? [])];
  if (S.editor) tb.append(h('div', { class: 'tb-hint' }, 'Map editor'));
  for (const id of ids) {
    const t = TOOLS.find((x) => x.id === id)!;
    tb.append(h('button', { class: S.tool === id ? 'on' : '', title: `${t.hint}${t.key ? ` (${t.key.toUpperCase()})` : ''}`, onclick: () => { S.tool = id; renderToolbar(); } }, t.label));
  }
  if (ids.length === 1) tb.append(h('div', { class: 'tb-hint' }, 'Open a step to see its tools.'));

  const bb = $('#brushbar');
  bb.innerHTML = '';
  const t = TOOLS.find((x) => x.id === S.tool)!;
  if (S.tool === 'navigate') {
    bb.style.display = 'none';
    return;
  }
  bb.style.display = '';
  const slider = (label: string, key: keyof typeof S.brush, min: number, max: number, step: number, fmt: (v: number) => string) => {
    const out = h('b', {}, fmt(S.brush[key]));
    return h('label', {}, h('span', {}, label), h('input', { type: 'range', min, max, step, value: S.brush[key], oninput: (e: Event) => {
      S.brush[key] = Number((e.target as HTMLInputElement).value);
      out.textContent = fmt(S.brush[key]);
    } }), out);
  };
  if (t.value !== 'pin' && S.tool !== 'arrow' && !t.gesture) {
    bb.append(slider('Radius', 'radius_km', 30, 3000, 10, (v) => `${v} km`), slider('Strength', 'strength', 0.05, 1, 0.05, (v) => v.toFixed(2)), slider('Hardness', 'hardness', 0, 0.95, 0.05, (v) => v.toFixed(2)));
    if (t.scatter) bb.append(slider('Scatter', 'scatter', 0.05, 1, 0.05, (v) => v.toFixed(2)), slider('Grain', 'grain_km', 40, 1500, 10, (v) => `${v} km`));
  }
  const val = S.toolValue[t.id] ?? t.defaultValue ?? 0;
  const setVal = (v: number) => (S.toolValue[t.id] = v);
  if (t.value === 'metres') bb.append(h('label', {}, h('span', {}, S.tool === 'flatten' ? 'Target (m)' : 'Amount (m)'), h('input', { type: 'number', step: 50, value: val, onchange: (e: Event) => setVal(Number((e.target as HTMLInputElement).value)) })));
  if (t.value === 'fertility') {
    const out = h('b', {}, (val > 0 ? '+' : '') + val.toFixed(2));
    bb.append(h('label', {}, h('span', {}, 'Fertility (− barren, + fertile)'), h('input', { type: 'range', min: -1, max: 1, step: 0.05, value: val, oninput: (e: Event) => {
      const v = Number((e.target as HTMLInputElement).value);
      setVal(v);
      out.textContent = (v > 0 ? '+' : '') + v.toFixed(2);
    } }), out));
  }
  if (t.value === 'bands') bb.append(h('label', {}, h('span', {}, 'Bands'), h('input', { type: 'number', step: 1, min: 1, max: 200, value: val, onchange: (e: Event) => setVal(Number((e.target as HTMLInputElement).value)) })));
  if (t.value === 'goods') {
    const goods = ['grain', 'wine', 'horses', 'wool', 'cattle', 'wood', 'furs', 'spices', 'fish', 'stone', 'metals', 'dates', 'salt', 'camels'];
    const deps = ['copper', 'gold', 'silver', 'iron', 'coal', 'salt'];
    bb.append(h('label', {}, h('span', {}, 'Paint'), h('select', { onchange: (e: Event) => setVal(Number((e.target as HTMLSelectElement).value)) },
      h('optgroup', { label: 'Trade good' }, ...goods.map((g, i) => h('option', { value: i + 1, selected: val === i + 1 }, g))),
      h('optgroup', { label: 'Add deposit' }, ...deps.map((d, i) => h('option', { value: 101 + i, selected: val === 101 + i }, `+ ${d}`))),
      h('optgroup', { label: 'Remove deposit' }, ...deps.map((d, i) => h('option', { value: 201 + i, selected: val === 201 + i }, `− ${d}`))))));
  }
  if (t.value === 'people') bb.append(h('label', {}, h('span', {}, 'Population'), h('input', { type: 'number', step: 1000, min: 0, value: val, onchange: (e: Event) => setVal(Number((e.target as HTMLInputElement).value)) })));
  if (t.value === 'barrier') bb.append(h('label', {}, h('span', {}, 'Barrier strength'), h('input', { type: 'number', step: 1, min: 0, max: 50, value: val, onchange: (e: Event) => setVal(Number((e.target as HTMLInputElement).value)) })));
  if (t.value === 'speed') bb.append(h('label', {}, h('span', {}, 'Speed (mm/yr)'), h('input', { type: 'number', step: 5, min: 1, max: 300, value: val, onchange: (e: Event) => setVal(Number((e.target as HTMLInputElement).value)) })));
  if (t.value === 'plate') {
    const plates = S.status!.steps[2].meta?.plates ?? [];
    bb.append(h('label', {}, h('span', {}, 'Plate'), h('select', { onchange: (e: Event) => setVal(Number((e.target as HTMLSelectElement).value)) },
      ...plates.map((p: any) => h('option', { value: p.id, selected: p.id === val }, `#${p.id} ${p.continental_fraction > 0.5 ? 'continental' : 'oceanic'} · ${p.area_mkm2.toFixed(1)} M km²`)))));
    bb.append(h('span', { class: 'muted' }, 'Tip: hover a cell to see its plate id.'));
  }
  if (t.value === 'terrain') {
    bb.append(h('label', {}, h('span', {}, 'Terrain'), h('select', { onchange: (e: Event) => setVal(Number((e.target as HTMLSelectElement).value)) },
      ...TERRAIN.slice(2).map(([n], i) => h('option', { value: i + 2, selected: i + 2 === val }, n)))));
  }
  if (t.value === 'pin') {
    bb.append(h('label', {}, h('span', {}, 'Plate kind'), h('select', { onchange: (e: Event) => (S.pinKind = (e.target as HTMLSelectElement).value) },
      ...['auto', 'continental', 'oceanic'].map((k) => h('option', { value: k, selected: k === S.pinKind }, k)))));
  }
  bb.append(h('span', { class: 'muted hint' }, t.hint));
}

function renderLegend(stale: boolean) {
  const lg = $('#legend');
  lg.innerHTML = '';
  const L = S.legend;
  if (!L) return;
  lg.append(h('div', { class: 'lg-title' }, L.title, stale ? h('span', { class: 'pill stale' }, 'stale') : null));
  if (L.gradient) {
    const st = L.gradient.stops;
    const lo = st[0][0], hi = st[st.length - 1][0];
    const css = st.map(([v, c]) => `rgb(${c.map(Math.round).join(',')}) ${(((v - lo) / (hi - lo)) * 100).toFixed(1)}%`).join(',');
    lg.append(h('div', { class: 'grad', style: `background: linear-gradient(90deg, ${css})` }),
      h('div', { class: 'grad-l' }, h('span', {}, `${lo.toLocaleString()} ${L.gradient.unit}`), h('span', {}, `${hi.toLocaleString()} ${L.gradient.unit}`)));
  }
  if (L.items) {
    const box = h('div', { class: 'items' });
    for (const it of L.items.slice(0, 24)) box.append(h('div', {}, h('i', { style: `background: rgb(${it.color.map(Math.round).join(',')})` }), it.label));
    lg.append(box);
  }
}

let toastTimer: number | undefined;
function toast(msg: string, err = false) {
  const t = $('#toast');
  t.textContent = msg;
  t.className = err ? 'show err' : 'show';
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (t.className = ''), err ? 7000 : 3000);
}

// ------------------------------------------------------------------ canvas interaction

type Drag =
  | { kind: 'nav'; x: number; y: number }
  | { kind: 'link'; start: Vec3; end: Vec3 }
  | { kind: 'stroke'; points: Vec3[]; base: Map<number, number>; dmin: Map<number, number>; noise: Map<number, number>; seed: number; gen: Noise | null }
  | { kind: 'arrow'; start: Vec3 };

function bindCanvas() {
  const cv = $('#canvas') as HTMLCanvasElement;
  let drag: Drag | null = null;
  let space = false;
  window.addEventListener('keydown', (e) => { if (e.code === 'Space' && e.target === document.body) { space = true; e.preventDefault(); } });
  window.addEventListener('keyup', (e) => { if (e.code === 'Space') space = false; });
  cv.addEventListener('contextmenu', (e) => e.preventDefault());

  const pos = (e: PointerEvent | WheelEvent) => {
    const r = cv.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top] as const;
  };

  cv.addEventListener('pointerdown', (e) => {
    if (!S.grid || !S.status) return;
    cv.setPointerCapture(e.pointerId);
    const [x, y] = pos(e);
    const p = R.pick(x, y);
    if (e.button !== 0 || S.tool === 'navigate' || space || !p || S.busy) {
      drag = { kind: 'nav', x: e.clientX, y: e.clientY };
      cv.style.cursor = 'grabbing';
      return;
    }
    if (S.tool === 'pin') {
      const [lat, lon] = toLatLon(p);
      mutate('add_pin', { pin: { lat: (lat * 180) / Math.PI, lon: (lon * 180) / Math.PI, kind: S.pinKind } }, S.autoRun ? 'plates' : undefined);
      return;
    }
    if (S.tool === 'arrow') {
      drag = { kind: 'arrow', start: p };
      return;
    }
    const tdef = TOOLS.find((x) => x.id === S.tool)!;
    if (tdef.gesture) {
      drag = { kind: 'link', start: p, end: p };
      return;
    }
    const seed = Math.floor(Math.random() * 2 ** 32);
    drag = { kind: 'stroke', points: [p], base: new Map(), dmin: new Map(), noise: new Map(), seed, gen: tdef.scatter ? new Noise(seed, SCATTER_STREAM) : null };
    paintPreview(drag, p);
  });

  cv.addEventListener('pointermove', (e) => {
    const [x, y] = pos(e);
    if (drag?.kind === 'nav') {
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      drag.x = e.clientX;
      drag.y = e.clientY;
      if (R.mode === 0) {
        const k = 0.004 * (R.dist - 1) / 2.2;
        R.yaw -= dx * k;
        R.pitch = Math.max(-1.55, Math.min(1.55, R.pitch + dy * k));
      } else {
        const fl = R.flatXform();
        const dpr = window.devicePixelRatio || 1;
        R.cx -= ((2 * dx * dpr) / cv.width) / fl[0];
        R.cy += ((2 * dy * dpr) / cv.height) / fl[1];
        R.cy = Math.max(-Math.PI / 2, Math.min(Math.PI / 2, R.cy));
      }
      R.requestDraw();
      return;
    }
    const p = R.pick(x, y);
    if (drag?.kind === 'stroke' && p) {
      const last = drag.points[drag.points.length - 1];
      const r = S.brush.radius_km / S.status!.params.planet.radius_km;
      const d = Math.acos(Math.max(-1, Math.min(1, last[0] * p[0] + last[1] * p[1] + last[2] * p[2])));
      if (d > r * 0.2) {
        drag.points.push(p);
        paintPreview(drag, p);
      }
      drawBrush(p, drag.points);
      return;
    }
    if (drag?.kind === 'link' && p) {
      drag.end = p;
      const L = new Lines();
      L.seg(drag.start, p, [1, 0.85, 0.2, 1], 0.006);
      const [a, b] = L.sets();
      R.setOverlay('brush', a, b);
      return;
    }
    if (drag?.kind === 'arrow' && p) {
      const L = new Lines();
      L.seg(drag.start, p, [1, 0.35, 0.3, 1], 0.006);
      const [a, b] = L.sets();
      R.setOverlay('brush', a, b);
      return;
    }
    drawBrush(p);
    hover(p);
  });

  const end = async (e: PointerEvent) => {
    const d = drag;
    drag = null;
    cv.style.cursor = '';
    if (!d) return;
    if (d.kind === 'stroke') {
      const t = TOOLS.find((x) => x.id === S.tool)!;
      const stroke = {
        tool: t.rust ?? S.tool,
        scatter: t.scatter ? S.brush.scatter : 0,
        scatter_scale_km: S.brush.grain_km,
        seed: d.seed,
        radius_km: S.brush.radius_km,
        strength: S.brush.strength,
        hardness: S.brush.hardness,
        value: S.toolValue[t.id] ?? t.defaultValue ?? 0,
        points: d.points.map((p) => toLatLon(p).map((v) => +((v * 180) / Math.PI).toFixed(4))),
      };
      drawBrush(null);
      await mutate('add_stroke', { stroke }, t.step);
    } else if (d.kind === 'link') {
      R.setOverlay('brush', null, null);
      const t = TOOLS.find((x) => x.id === S.tool)!;
      const deg = (p: Vec3) => toLatLon(p).map((v) => +((v * 180) / Math.PI).toFixed(4));
      const moved = Math.acos(Math.max(-1, Math.min(1, d.start[0] * d.end[0] + d.start[1] * d.end[1] + d.start[2] * d.end[2]))) > 0.004;
      if (t.gesture === 'link' && !moved && t.id !== 'band_pin') {
        toast('Drag from the first place to the second.');
        return;
      }
      let name = '';
      if (t.rename) {
        name = window.prompt(t.id === 'rename_state' ? 'New state name' : 'New province name')?.trim() ?? '';
        if (!name) return;
      }
      const stroke = {
        tool: t.rust ?? S.tool,
        radius_km: S.brush.radius_km,
        value: S.toolValue[t.id] ?? t.defaultValue ?? 0,
        points: (moved && t.gesture === 'link' ? [d.start, d.end] : [d.start]).map(deg),
        name,
      };
      await mutate('add_stroke', { stroke }, t.step);
    } else if (d.kind === 'arrow') {
      const [x, y] = pos(e);
      const p = R.pick(x, y);
      R.setOverlay('brush', null, null);
      if (!p) return;
      const s = d.start;
      const [ee, nn] = eastNorth(s);
      const v = [p[0] - s[0], p[1] - s[1], p[2] - s[2]];
      const de = v[0] * ee[0] + v[1] * ee[1] + v[2] * ee[2];
      const dn = v[0] * nn[0] + v[1] * nn[1] + v[2] * nn[2];
      if (Math.hypot(de, dn) < 1e-4) return;
      const [lat, lon] = toLatLon(s);
      const bearing = ((Math.atan2(de, dn) * 180) / Math.PI + 360) % 360;
      await mutate('add_arrow', { arrow: { lat: (lat * 180) / Math.PI, lon: (lon * 180) / Math.PI, bearing_deg: +bearing.toFixed(1), speed_mm_yr: S.toolValue['arrow'] ?? 50 } }, 'plates');
    }
  };
  cv.addEventListener('pointerup', end);
  cv.addEventListener('pointercancel', end);
  cv.addEventListener('pointerleave', () => { if (!drag) drawBrush(null); });

  cv.addEventListener('wheel', (e) => {
    e.preventDefault();
    const k = Math.exp(e.deltaY * 0.0012);
    if (R.mode === 0) R.dist = Math.max(1.08, Math.min(8, 1 + (R.dist - 1) * k));
    else R.zoom = Math.max(0.5, Math.min(60, R.zoom / k));
    R.requestDraw();
  }, { passive: false });
}

/** Instant local feedback while painting the sketch; the core recomputes on release.
 *  Uses the same falloff and, for scatter strokes, the same seeded noise as the core. */
function paintPreview(d: Extract<Drag, { kind: 'stroke' }>, p: Vec3) {
  const g = S.grid!;
  const tdef = TOOLS.find((x) => x.id === S.tool)!;
  const tool = tdef.rust ?? S.tool;
  if (!['land', 'sea', 'mountain', 'erase_hint'].includes(tool) || S.layer !== 'sketch' || !S.colors) return;
  const R0 = S.status!.params.planet.radius_km;
  const r = S.brush.radius_km / R0;
  const sk = S.cache.get('sketch|null')?.values as Float32Array | undefined;
  const hint = S.cache.get('mountain_hint|null')?.values as Float32Array | undefined;
  const target = tool === 'land' || tool === 'sea' ? sk : hint;
  if (!target) return;
  const goal = tool === 'land' ? 1 : tool === 'sea' ? -1 : tool === 'mountain' ? 1 : 0;
  const inner = Math.min(0.98, S.brush.hardness);
  const reach = d.gen ? r * (1 + S.brush.scatter) : r;
  const freq = R0 / Math.max(10, S.brush.grain_km);
  const octaves = scatterOctaves(S.brush.grain_km, g.spacing * R0);
  g.within(p, reach, (c, dist) => {
    const prev = d.dmin.get(c) ?? Infinity;
    if (dist >= prev) return;
    d.dmin.set(c, dist);
    let w: number;
    if (d.gen) {
      let nz = d.noise.get(c);
      if (nz === undefined) {
        nz = d.gen.fbm(g.p(c), freq, octaves);
        d.noise.set(c, nz);
      }
      w = scatterWeight(S.brush.scatter, S.brush.hardness, S.brush.strength, dist / r, nz);
    } else {
      if (dist > r) return;
      const t = dist / r;
      w = t <= inner ? 1 : (() => { const u = (t - inner) / (1 - inner); const s = 1 - u; return s * s * (3 - 2 * s); })();
      w *= S.brush.strength;
    }
    if (!d.base.has(c)) d.base.set(c, target[c]);
    const b = d.base.get(c)!;
    target[c] = b + (goal - b) * w;
    // Recolour the cell the way the sketch layer does.
    const v = sk ? sk[c] : -1;
    const land = v > 0;
    let col = land ? [118, 160, 90] : [38, 78, 128];
    const soft = 1 - Math.abs(v);
    col = col.map((x) => x + soft * 25);
    const hv = hint ? hint[c] : 0;
    if (hv > 0.02) col = col.map((x, k) => x + ([150, 95, 60][k] - x) * Math.min(1, hv));
    S.colors![c * 4] = col[0];
    S.colors![c * 4 + 1] = col[1];
    S.colors![c * 4 + 2] = col[2];
  });
  R.setColors(S.colors);
}

let hoverTimer: number | undefined;
let hoverCell = -1;
function hover(p: Vec3 | null) {
  if (!p || !S.grid) return;
  const c = S.grid.nearest(p, hoverCell >= 0 ? hoverCell : undefined);
  if (c === hoverCell) return;
  hoverCell = c;
  clearTimeout(hoverTimer);
  hoverTimer = window.setTimeout(async () => {
    if (S.busy) return;
    try {
      const d = await api.json('probe', { cell: c, month: S.month });
      if (d.state !== undefined) await political();
      if (c !== hoverCell) return;
      $('#hover').textContent = describe(d);
    } catch { /* ignore */ }
  }, 60);
}

function riverClass(w: number): string {
  return w >= 1000 ? 'great river' : w >= 400 ? 'major river' : w >= 150 ? 'river' : 'stream';
}

function describe(d: any): string {
  const parts: string[] = [];
  const ns = d.lat >= 0 ? 'N' : 'S', ew = d.lon >= 0 ? 'E' : 'W';
  parts.push(`${Math.abs(d.lat).toFixed(1)}°${ns} ${Math.abs(d.lon).toFixed(1)}°${ew}`);
  if (d.elevation !== undefined) parts.push(`${Math.round(d.elevation).toLocaleString()} m`);
  if (d.plate !== undefined) parts.push(`plate #${d.plate}${d.crust === 1 ? ' (continental)' : d.crust === 0 ? ' (oceanic)' : ''}`);
  if (d.boundary) parts.push(BOUNDARY[d.boundary][0]);
  const adj = d.temp_adjust ?? 0;
  if (Array.isArray(d.temp)) {
    const mean = d.temp.reduce((a: number, b: number) => a + b, 0) / 12 + adj;
    parts.push(`${mean.toFixed(1)} °C (${(Math.min(...d.temp) + adj).toFixed(0)} to ${(Math.max(...d.temp) + adj).toFixed(0)})`);
  } else if (typeof d.temp === 'number') parts.push(`${(d.temp + adj).toFixed(1)} °C in ${MONTHS[S.month ?? 0]}`);
  if (d.p_ann !== undefined) parts.push(`${Math.round(d.p_ann).toLocaleString()} mm/yr`);
  if (d.koppen) parts.push(`${KOPPEN[d.koppen][0]} ${KOPPEN[d.koppen][2]}`);
  if (d.terrain !== undefined) parts.push(TERRAIN[d.terrain][0]);
  if (d.river) parts.push(`${riverClass(d.river_width)} ${Math.round(d.river_width).toLocaleString()} m wide, ${Math.round(d.discharge).toLocaleString()} m³/s, order ${d.river}`);
  if (d.lake) parts.push(['', 'fresh lake', 'salt lake', 'dry lake'][d.lake]);
  if (d.habitability !== undefined && d.water === 0) parts.push(`habitability ${d.habitability.toFixed(2)}${d.river_role === 1 ? ', border river' : d.river_role === 2 ? ', backbone river' : ''}`);
  const P = S.political;
  if (P && d.province) {
    const p = P.byProv.get(d.province);
    if (p) {
      parts.push(`${p.name} (province ${p.id}, ${p.kind}${p.band ? ' ' + p.band : ''}, ${Math.round(p.area_km2 / 1000)}k km²)`);
      const goods = [p.trade_good && p.trade_good !== 'none' ? p.trade_good : null, ...(p.resources ?? [])].filter(Boolean);
      if (goods.length) parts.push(goods.join(', '));
    }
  }
  if (P && d.state) {
    const s = P.byState.get(d.state);
    const r = s ? P.byRegion.get(s.region) : null;
    const ct = s ? P.byCont.get(s.continent) : null;
    if (s) parts.push(`state ${s.name}${r ? ', ' + r.name : ''}${ct ? ', ' + ct.name : ''}`);
  }
  if (P && d.culture) {
    const c = P.byCulture.get(d.culture), g = c ? P.byGroup.get(c.group) : null;
    if (c) parts.push(`${c.name} culture${g ? ' (' + g.name + ')' : ''}`);
  }
  if (d.population) parts.push(`${d.population.toFixed(1)} people/km²`);
  if (d.site_kind) parts.push(d.site_kind === 3 ? 'site pin' : `spring (${d.site_kind === 2 ? 'basin floor' : 'mountain foot'}, ${d.groundwater?.toFixed(1)} m³/s)`);
  return parts.join('  ·  ');
}

function bindKeys() {
  window.addEventListener('keydown', (e) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
    if (e.ctrlKey || e.metaKey) {
      if (e.key === 'z') { e.preventDefault(); if (S.status?.can_undo) mutate('undo', {}, undoTarget()); }
      if (e.key === 'y' || (e.key === 'Z' && e.shiftKey)) { e.preventDefault(); if (S.status?.can_redo) mutate('redo', {}, undoTarget()); }
      if (e.key === 's') { e.preventDefault(); onSave(); }
      return;
    }
    if (e.key === '[') { S.brush.radius_km = Math.max(30, Math.round(S.brush.radius_km / 1.25)); renderToolbar(); }
    if (e.key === ']') { S.brush.radius_km = Math.min(3000, Math.round(S.brush.radius_km * 1.25)); renderToolbar(); }
    if (e.key === 'g') setView(0);
    if (e.key === 'f') setView(1);
    const open = STEPS.find((s) => s.key === S.openStep);
    const t = TOOLS.find((x) => x.key === e.key && (x.id === 'navigate' || (S.editor ? EDITOR_TOOLS : open?.tools ?? []).includes(x.id)));
    if (t) { S.tool = t.id; renderToolbar(); }
  });
}

boot();
