// Map layers: which fields each layer needs and how cells are coloured.

import type { Grid } from './grid';
import { eastNorth } from './grid';

export type RGB = [number, number, number];

// Must match core/src/stages/biomes.rs
export const KOPPEN: [string, RGB, string][] = [
  ['Ocean', [24, 52, 96], ''],
  ['Af', [0, 0, 255], 'Tropical rainforest'],
  ['Am', [0, 120, 255], 'Tropical monsoon'],
  ['Aw', [70, 170, 250], 'Tropical savanna'],
  ['BWh', [255, 0, 0], 'Hot desert'],
  ['BWk', [255, 150, 150], 'Cold desert'],
  ['BSh', [245, 165, 0], 'Hot steppe'],
  ['BSk', [255, 220, 100], 'Cold steppe'],
  ['Csa', [255, 255, 0], 'Hot-summer Mediterranean'],
  ['Csb', [200, 200, 0], 'Warm-summer Mediterranean'],
  ['Csc', [150, 150, 0], 'Cold-summer Mediterranean'],
  ['Cwa', [150, 255, 150], 'Monsoon humid subtropical'],
  ['Cwb', [100, 200, 100], 'Subtropical highland'],
  ['Cwc', [50, 150, 50], 'Cold subtropical highland'],
  ['Cfa', [200, 255, 80], 'Humid subtropical'],
  ['Cfb', [100, 255, 80], 'Oceanic'],
  ['Cfc', [50, 200, 0], 'Subpolar oceanic'],
  ['Dsa', [255, 0, 255], 'Dry hot-summer continental'],
  ['Dsb', [200, 0, 200], 'Dry warm-summer continental'],
  ['Dsc', [150, 50, 150], 'Dry subarctic'],
  ['Dsd', [150, 100, 150], 'Dry extremely cold subarctic'],
  ['Dwa', [170, 175, 255], 'Monsoon hot-summer continental'],
  ['Dwb', [90, 120, 220], 'Monsoon warm-summer continental'],
  ['Dwc', [75, 80, 180], 'Monsoon subarctic'],
  ['Dwd', [50, 0, 135], 'Monsoon extremely cold subarctic'],
  ['Dfa', [0, 255, 255], 'Hot-summer humid continental'],
  ['Dfb', [55, 200, 255], 'Warm-summer humid continental'],
  ['Dfc', [0, 125, 125], 'Subarctic'],
  ['Dfd', [0, 70, 95], 'Extremely cold subarctic'],
  ['ET', [178, 178, 178], 'Tundra'],
  ['EF', [102, 102, 102], 'Ice cap'],
  ['Lake', [60, 110, 190], ''],
];

export const TERRAIN: [string, RGB][] = [
  ['Ocean', [30, 60, 110]],
  ['Lake', [70, 120, 200]],
  ['Glacier', [235, 240, 248]],
  ['Tundra', [160, 170, 150]],
  ['Taiga', [40, 90, 70]],
  ['Forest', [50, 125, 50]],
  ['Plains', [150, 190, 90]],
  ['Steppe', [200, 190, 110]],
  ['Desert', [235, 205, 140]],
  ['Drylands', [205, 160, 95]],
  ['Mediterranean', [170, 165, 70]],
  ['Savanna', [190, 175, 70]],
  ['Jungle', [20, 100, 30]],
  ['Wetlands', [80, 120, 100]],
  ['Floodplains', [120, 165, 60]],
  ['Hills', [140, 120, 90]],
  ['Mountains', [110, 95, 85]],
  ['Highlands', [150, 135, 110]],
];

export const BOUNDARY: [string, RGB][] = [
  ['None', [0, 0, 0]],
  ['Continent–continent collision', [230, 60, 40]],
  ['Ocean–continent subduction', [250, 160, 30]],
  ['Ocean–ocean subduction', [240, 220, 60]],
  ['Mid-ocean ridge', [60, 200, 240]],
  ['Continental rift', [180, 90, 230]],
  ['Transform fault', [150, 150, 150]],
];

type Stop = [number, RGB];
export const RAMPS: Record<string, Stop[]> = {
  temp: [[-40, [80, 0, 120]], [-25, [40, 40, 200]], [-10, [60, 140, 240]], [0, [200, 235, 255]], [10, [120, 200, 120]], [20, [250, 220, 80]], [30, [240, 100, 30]], [40, [150, 0, 0]]],
  precip: [[0, [150, 90, 40]], [250, [220, 190, 110]], [500, [230, 230, 160]], [1000, [130, 200, 120]], [2000, [40, 140, 160]], [3000, [30, 70, 170]], [4500, [60, 20, 120]]],
  land: [[0, [86, 140, 82]], [300, [128, 168, 96]], [800, [196, 196, 128]], [1600, [190, 150, 100]], [2800, [150, 110, 90]], [4200, [200, 190, 185]], [6000, [250, 250, 252]]],
  sea: [[-8000, [8, 20, 60]], [-5000, [20, 45, 100]], [-3000, [30, 70, 135]], [-1000, [50, 105, 170]], [-150, [90, 155, 205]], [0, [130, 190, 225]]],
  density: [[0, [235, 228, 205]], [1, [220, 200, 120]], [5, [215, 150, 60]], [15, [190, 80, 40]], [40, [120, 20, 40]], [100, [50, 0, 40]]],
  unit: [[0, [20, 30, 60]], [0.25, [40, 90, 160]], [0.5, [80, 170, 170]], [0.75, [210, 200, 90]], [1, [250, 240, 200]]],
  diverging: [[-1, [40, 80, 200]], [0, [240, 240, 240]], [1, [200, 50, 40]]],
  wind: [[0, [30, 40, 90]], [3, [50, 110, 170]], [6, [90, 180, 160]], [9, [220, 220, 110]], [13, [240, 140, 60]], [18, [200, 50, 60]]],
  habit: [[0, [120, 100, 85]], [0.15, [190, 160, 110]], [0.35, [220, 210, 120]], [0.6, [130, 190, 90]], [0.85, [40, 140, 60]], [1, [20, 100, 50]]],
  barrier: [[0, [235, 232, 220]], [1, [215, 200, 160]], [3, [190, 140, 90]], [6, [140, 80, 70]], [10, [70, 40, 60]]],
  age: [[0, [250, 60, 60]], [20, [250, 170, 40]], [60, [240, 240, 120]], [120, [80, 170, 200]], [200, [40, 50, 140]]],
};

export function ramp(stops: Stop[], v: number): RGB {
  if (!(v > stops[0][0])) return stops[0][1];
  for (let k = 1; k < stops.length; k++) {
    const [b, cb] = stops[k];
    if (v <= b) {
      const [a, ca] = stops[k - 1];
      const t = (v - a) / (b - a);
      return [ca[0] + (cb[0] - ca[0]) * t, ca[1] + (cb[1] - ca[1]) * t, ca[2] + (cb[2] - ca[2]) * t];
    }
  }
  return stops[stops.length - 1][1];
}

export function plateColor(id: number): RGB {
  // Golden-angle hues, alternating lightness.
  const h = (id * 137.508) % 360;
  const l = id % 2 ? 0.62 : 0.5;
  return hsl(h, 0.55, l);
}

function hsl(h: number, s: number, l: number): RGB {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [(r + m) * 255, (g + m) * 255, (b + m) * 255];
}

/** Per-cell hillshade factor (≈0.55–1.45) from elevation, lit from the north-west. */
export function hillshade(g: Grid, elev: ArrayLike<number>, radiusKm: number, strength = 1): Float32Array {
  const out = new Float32Array(g.n);
  const spacingM = g.spacing * radiusKm * 1000;
  for (let i = 0; i < g.n; i++) {
    const p = g.p(i);
    const [e, n] = eastNorth(p);
    const L = [(-e[0] + n[0]) * Math.SQRT1_2, (-e[1] + n[1]) * Math.SQRT1_2, (-e[2] + n[2]) * Math.SQRT1_2];
    let num = 0;
    let den = 0;
    const ei = Math.max(0, elev[i]);
    for (let k = g.off[i]; k < g.off[i + 1]; k++) {
      const j = g.nbr[k];
      const q = g.p(j);
      const d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
      const dl = Math.hypot(d[0], d[1], d[2]);
      const c = (d[0] * L[0] + d[1] * L[1] + d[2] * L[2]) / dl;
      num += c * (Math.max(0, elev[j]) - ei);
      den += c * c;
    }
    const slope = den > 0 ? num / den / spacingM : 0; // rise toward the light per metre
    out[i] = 1 + Math.max(-0.45, Math.min(0.45, slope * 14 * strength));
  }
  return out;
}

export type LegendItem = { color: RGB; label: string };
export type Legend = { title: string; items?: LegendItem[]; gradient?: { stops: Stop[]; unit: string } };

export type LayerId =
  | 'sketch' | 'plates' | 'crust' | 'boundaries' | 'elevation' | 'temperature' | 'precipitation'
  | 'continentality' | 'currents' | 'wind' | 'ocean_age' | 'koppen' | 'terrain' | 'discharge' | 'erosion' | 'stress'
  | 'habitability' | 'barrier' | 'springs' | 'states' | 'regions' | 'provinces' | 'resources' | 'cultures' | 'culture_groups' | 'population' | 'attraction';

export type LayerDef = {
  id: LayerId;
  label: string;
  fields: string[];
  monthly?: boolean;
  smooth: boolean;
  group: string;
};

export const LAYERS: LayerDef[] = [
  { id: 'sketch', label: 'Sketch', fields: ['sketch', 'land', 'mountain_hint'], smooth: false, group: 'Planet' },
  { id: 'plates', label: 'Plates', fields: ['plate', 'crust'], smooth: false, group: 'Planet' },
  { id: 'crust', label: 'Crust & plate motion', fields: ['crust', 'plate'], smooth: false, group: 'Planet' },
  { id: 'boundaries', label: 'Plate boundaries', fields: ['boundary', 'elevation'], smooth: false, group: 'Planet' },
  { id: 'elevation', label: 'Elevation', fields: ['elevation', 'water'], smooth: true, group: 'Planet' },
  { id: 'stress', label: 'Tectonic stress', fields: ['stress'], smooth: true, group: 'Planet' },
  { id: 'ocean_age', label: 'Sea-floor age', fields: ['ocean_age', 'elevation'], smooth: true, group: 'Planet' },
  { id: 'temperature', label: 'Temperature', fields: ['temp', 'elevation'], monthly: true, smooth: true, group: 'Climate' },
  { id: 'precipitation', label: 'Precipitation', fields: ['precip', 'elevation'], monthly: true, smooth: true, group: 'Climate' },
  { id: 'wind', label: 'Wind speed', fields: ['wind_u', 'wind_v', 'elevation'], monthly: true, smooth: true, group: 'Climate' },
  { id: 'continentality', label: 'Continentality', fields: ['continentality', 'elevation'], smooth: true, group: 'Climate' },
  { id: 'currents', label: 'Coastal currents', fields: ['current_offset', 'elevation'], smooth: true, group: 'Climate' },
  { id: 'discharge', label: 'Rivers & lakes', fields: ['discharge', 'elevation', 'lake', 'water'], smooth: false, group: 'Water' },
  { id: 'erosion', label: 'Erosion / deposition', fields: ['erosion', 'elevation'], smooth: true, group: 'Water' },
  { id: 'koppen', label: 'Köppen climate', fields: ['koppen', 'elevation'], smooth: false, group: 'Biomes' },
  { id: 'terrain', label: 'Game terrain', fields: ['terrain', 'elevation'], smooth: false, group: 'Biomes' },
  { id: 'habitability', label: 'Habitability', fields: ['habitability', 'water', 'elevation'], smooth: true, group: 'Political' },
  { id: 'springs', label: 'Groundwater & springs', fields: ['groundwater', 'site', 'site_kind', 'water', 'elevation'], smooth: false, group: 'Political' },
  { id: 'barrier', label: 'Barriers', fields: ['barrier', 'river_role', 'water', 'elevation'], smooth: false, group: 'Political' },
  { id: 'states', label: 'States', fields: ['state', 'province', 'province_kind', 'water', 'elevation'], smooth: false, group: 'Political' },
  { id: 'regions', label: 'Regions & continents', fields: ['region', 'state', 'continent', 'water', 'elevation'], smooth: false, group: 'Political' },
  { id: 'provinces', label: 'Provinces', fields: ['province', 'province_kind', 'state', 'water', 'elevation'], smooth: false, group: 'Political' },
  { id: 'resources', label: 'Trade goods', fields: ['trade_good', 'province_kind', 'water', 'elevation'], smooth: false, group: 'Political' },
  { id: 'cultures', label: 'Cultures', fields: ['culture', 'culture_group', 'province_kind', 'water', 'elevation'], smooth: false, group: 'Cultures' },
  { id: 'culture_groups', label: 'Culture groups', fields: ['culture_group', 'province_kind', 'water', 'elevation'], smooth: false, group: 'Cultures' },
  { id: 'attraction', label: 'Attraction', fields: ['attraction', 'population', 'province_kind', 'water', 'elevation'], smooth: false, group: 'Cultures' },
  { id: 'population', label: 'Population density', fields: ['population', 'province_kind', 'water', 'elevation'], smooth: false, group: 'Cultures' },
];

/** Same colours as states.png (core/src/export.rs state_color). */
export function stateColor(id: number): RGB {
  const h = (id * 137.508) % 360;
  const l = id % 3 === 0 ? 0.48 : id % 3 === 1 ? 0.6 : 0.7;
  return hsl(h, 0.5, l).map((v) => Math.floor(v)) as RGB;
}

/** Trade goods in the order of the trade_good field (value = index + 1), as in core/src/stages/resources.rs. */
export const TRADE_GOODS: [string, RGB][] = [
  ['grain', [226, 196, 86]], ['wine', [140, 40, 90]], ['horses', [170, 120, 70]], ['wool', [220, 220, 205]], ['cattle', [190, 150, 110]],
  ['wood', [50, 120, 50]], ['furs', [110, 80, 60]], ['spices', [220, 90, 40]], ['fish', [70, 150, 200]], ['stone', [140, 140, 140]],
  ['metals', [90, 90, 120]], ['dates', [210, 140, 40]], ['salt', [245, 245, 250]], ['camels', [200, 170, 120]],
];

/** Culture colour: its group's hue, shifted a little per culture (same as culture_color in core/src/stages/cultures.rs). */
export function cultureColor(id: number, group: number): RGB {
  const h = ((((group * 137.508) % 360) + ((id * 67) % 40) - 20) % 360 + 360) % 360;
  return hsl(h, 0.42 + 0.12 * (id % 3), 0.42 + 0.07 * (id % 4)).map((v) => Math.floor(v)) as RGB;
}

export function groupColor(group: number): RGB {
  return hsl((group * 137.508) % 360, 0.55, 0.5).map((v) => Math.floor(v)) as RGB;
}

/** Stable pseudo-random colour for a province id (the UI's view; provinces.png uses the table colours). */
export function provinceColor(id: number): RGB {
  let x = Math.imul(id ^ 0x9e3779b9, 0x85ebca6b);
  x = Math.imul(x ^ (x >>> 13), 0xc2b2ae35);
  x ^= x >>> 16;
  return hsl((x >>> 0) % 360, 0.45 + ((x >>> 9) % 30) / 100, 0.5 + ((x >>> 17) % 25) / 100);
}



type F = Record<string, ArrayLike<number> | undefined>;

/** Colour every cell for a layer. Returns RGBA bytes and a legend. */
export function colorize(id: LayerId, g: Grid, f: F, shade: Float32Array | null, month: number | null): { rgba: Uint8Array; legend: Legend } {
  const n = g.n;
  const out = new Uint8Array(n * 4);
  const put = (i: number, c: RGB, s = 1) => {
    out[i * 4] = Math.min(255, c[0] * s);
    out[i * 4 + 1] = Math.min(255, c[1] * s);
    out[i * 4 + 2] = Math.min(255, c[2] * s);
    out[i * 4 + 3] = 255;
  };
  const elev = f['elevation'];
  const sh = (i: number) => (shade ? shade[i] : 1);
  // Hydrology's surface map (0 land, 1 ocean, 2 lake) once it exists; before
  // that, sea level decides. Dry basins below sea level are land.
  const wm = f['water'];
  const isSea = (i: number) => (wm ? wm[i] === 1 : !elev || elev[i] <= 0);
  const relief = (i: number): RGB => {
    const e = elev ? elev[i] : 0;
    if (wm && wm[i] === 2) return [70, 130, 210];
    return !isSea(i) ? ramp(RAMPS.land, Math.max(0, e)) : ramp(RAMPS.sea, e);
  };
  const grey = (i: number): RGB => {
    const e = elev ? elev[i] : 0;
    const v = e > 0 ? 120 + Math.min(1, e / 5000) * 100 : 45 + Math.max(-1, e / 6000) * 25;
    return [v, v, v * 1.05];
  };
  let legend: Legend = { title: '' };

  switch (id) {
    case 'sketch': {
      const s = f['sketch'], land = f['land'], hint = f['mountain_hint'];
      for (let i = 0; i < n; i++) {
        const v = s ? s[i] : -1;
        const isLand = land ? land[i] === 1 : v > 0;
        let c: RGB = isLand ? [118, 160, 90] : [38, 78, 128];
        const soft = 1 - Math.abs(v);
        c = [c[0] + soft * 25, c[1] + soft * 25, c[2] + soft * 25];
        const h = hint ? hint[i] : 0;
        if (h > 0.02) c = mix(c, [150, 95, 60], Math.min(1, h));
        put(i, c);
      }
      legend = { title: 'Sketch', items: [
        { color: [118, 160, 90], label: 'Land' }, { color: [38, 78, 128], label: 'Sea' },
        { color: [150, 95, 60], label: '“Mountains here” hint' },
        { color: [150, 170, 160], label: 'Soft edge (coast noise decides)' },
      ] };
      break;
    }
    case 'plates': {
      const pl = f['plate'], cr = f['crust'];
      for (let i = 0; i < n; i++) {
        const id = pl ? pl[i] : 0;
        let edge = false;
        for (let k = g.off[i]; k < g.off[i + 1]; k++) if (pl && pl[g.nbr[k]] !== id) edge = true;
        const c = plateColor(id);
        put(i, edge ? [30, 30, 30] : c, cr && cr[i] === 1 ? 1.12 : 0.8);
      }
      legend = { title: 'Plates', items: [
        { color: [200, 200, 200], label: 'Brighter: continental crust' },
        { color: [130, 130, 130], label: 'Darker: oceanic crust' },
        { color: [30, 30, 30], label: 'Plate boundary' },
      ] };
      break;
    }
    case 'crust': {
      const cr = f['crust'];
      for (let i = 0; i < n; i++) put(i, cr && cr[i] === 1 ? [190, 165, 120] : [55, 80, 120]);
      legend = { title: 'Crust', items: [{ color: [190, 165, 120], label: 'Continental' }, { color: [55, 80, 120], label: 'Oceanic' }] };
      break;
    }
    case 'boundaries': {
      const b = f['boundary'];
      for (let i = 0; i < n; i++) {
        const k = b ? b[i] : 0;
        put(i, k ? BOUNDARY[k][1] : grey(i), k ? 1 : sh(i));
      }
      legend = { title: 'Boundary type', items: BOUNDARY.slice(1).map(([l, c]) => ({ color: c, label: l })) };
      break;
    }
    case 'elevation':
      for (let i = 0; i < n; i++) put(i, relief(i), sh(i));
      legend = { title: 'Elevation', gradient: { stops: [...RAMPS.sea, ...RAMPS.land.slice(1)], unit: 'm' } };
      break;
    case 'stress': {
      const s = f['stress'];
      for (let i = 0; i < n; i++) put(i, ramp(RAMPS.unit, s ? s[i] : 0));
      legend = { title: 'Tectonic stress', gradient: { stops: RAMPS.unit, unit: '' } };
      break;
    }
    case 'ocean_age': {
      const a = f['ocean_age'];
      for (let i = 0; i < n; i++) put(i, elev && elev[i] > 0 ? grey(i) : ramp(RAMPS.age, a ? a[i] : 0), elev && elev[i] > 0 ? sh(i) : 1);
      legend = { title: 'Sea-floor age', gradient: { stops: RAMPS.age, unit: 'Myr' } };
      break;
    }
    case 'temperature': {
      const t = f['temp'];
      for (let i = 0; i < n; i++) put(i, ramp(RAMPS.temp, t ? t[i] : 0), elev && elev[i] > 0 ? sh(i) : 1);
      legend = { title: month === null ? 'Annual mean temperature' : `Temperature, ${MONTHS[month]}`, gradient: { stops: RAMPS.temp, unit: '°C' } };
      break;
    }
    case 'precipitation': {
      const p = f['precip'];
      // Both the monthly field and the annual mean of months are per month;
      // show them at an annual rate so one scale fits both.
      for (let i = 0; i < n; i++) put(i, ramp(RAMPS.precip, p ? p[i] * 12 : 0), elev && elev[i] > 0 ? sh(i) : 1);
      legend = { title: month === null ? 'Annual precipitation' : `Precipitation, ${MONTHS[month]} (×12)`, gradient: { stops: RAMPS.precip, unit: 'mm/yr' } };
      break;
    }
    case 'wind': {
      const u = f['wind_u'], v = f['wind_v'];
      for (let i = 0; i < n; i++) put(i, ramp(RAMPS.wind, u && v ? Math.hypot(u[i], v[i]) : 0), elev && elev[i] > 0 ? 0.75 + 0.25 * sh(i) : 1);
      legend = { title: month === null ? 'Wind speed (annual mean vector)' : `Wind speed, ${MONTHS[month]}`, gradient: { stops: RAMPS.wind, unit: 'm/s' } };
      break;
    }
    case 'continentality': {
      const c = f['continentality'];
      for (let i = 0; i < n; i++) put(i, elev && elev[i] > 0 ? ramp(RAMPS.unit, c ? c[i] : 0) : [35, 55, 90]);
      legend = { title: 'Continentality (0 maritime – 1 continental)', gradient: { stops: RAMPS.unit, unit: '' } };
      break;
    }
    case 'currents': {
      const c = f['current_offset'];
      for (let i = 0; i < n; i++) put(i, ramp(RAMPS.diverging, (c ? c[i] : 0) / 5), elev && elev[i] > 0 ? sh(i) * 0.9 : 1);
      legend = { title: 'Coastal current offset', gradient: { stops: RAMPS.diverging.map(([v, c]) => [v * 5, c] as Stop), unit: '°C' } };
      break;
    }
    case 'discharge': {
      const q = f['discharge'], lk = f['lake'];
      for (let i = 0; i < n; i++) {
        if (isSea(i)) { put(i, [30, 45, 70]); continue; }
        if (lk && lk[i]) { put(i, lk[i] === 1 ? [70, 130, 210] : lk[i] === 2 ? [120, 170, 170] : [210, 200, 170]); continue; }
        const v = q ? Math.log10(Math.max(1, q[i])) / 5 : 0;
        put(i, mix([225, 220, 205], [10, 60, 180], Math.min(1, v)), sh(i));
      }
      legend = { title: 'Discharge', items: [
        { color: [225, 220, 205], label: '< 10 m³/s' }, { color: [120, 140, 190], label: '~1,000 m³/s' }, { color: [10, 60, 180], label: '100,000 m³/s' },
        { color: [70, 130, 210], label: 'Fresh lake' }, { color: [120, 170, 170], label: 'Salt lake' }, { color: [210, 200, 170], label: 'Dry lake / salt flat' },
      ] };
      break;
    }
    case 'erosion': {
      const e = f['erosion'];
      for (let i = 0; i < n; i++) put(i, elev && elev[i] > 0 ? ramp(RAMPS.diverging, Math.max(-1, Math.min(1, -(e ? e[i] : 0) / 400))) : [35, 55, 90]);
      legend = { title: 'Erosion (red) / deposition (blue)', gradient: { stops: RAMPS.diverging.map(([v, c]) => [-v * 400, c] as Stop).reverse(), unit: 'm' } };
      break;
    }
    case 'koppen': {
      const k = f['koppen'];
      const seen = new Set<number>();
      for (let i = 0; i < n; i++) {
        const c = k ? k[i] : 0;
        seen.add(c);
        put(i, KOPPEN[c][1], c === 0 ? 1 : 0.85 + 0.15 * sh(i));
      }
      legend = { title: 'Köppen–Geiger', items: [...seen].sort((a, b) => a - b).filter((c) => c > 0).map((c) => ({ color: KOPPEN[c][1], label: `${KOPPEN[c][0]} ${KOPPEN[c][2]}` })) };
      break;
    }
    case 'terrain': {
      const t = f['terrain'];
      const seen = new Set<number>();
      for (let i = 0; i < n; i++) {
        const c = t ? t[i] : 0;
        seen.add(c);
        put(i, TERRAIN[c][1], c <= 1 ? 1 : sh(i));
      }
      legend = { title: 'Game terrain', items: [...seen].sort((a, b) => a - b).map((c) => ({ color: TERRAIN[c][1], label: TERRAIN[c][0] })) };
      break;
    }
    case 'habitability': {
      const hb = f['habitability'];
      for (let i = 0; i < n; i++) put(i, isSea(i) || (wm && wm[i] === 2) ? relief(i) : ramp(RAMPS.habit, hb ? hb[i] : 0), isSea(i) ? 1 : 0.8 + 0.2 * sh(i));
      legend = { title: 'Habitability (0 barren – 1 fertile)', gradient: { stops: RAMPS.habit, unit: '' } };
      break;
    }
    case 'barrier': {
      const b = f['barrier'], rr = f['river_role'];
      for (let i = 0; i < n; i++) {
        if (isSea(i) || (wm && wm[i] === 2)) { put(i, [32, 48, 78]); continue; }
        if (rr && rr[i] === 1) { put(i, [235, 60, 50]); continue; }
        if (rr && rr[i] === 2) { put(i, [60, 210, 120]); continue; }
        put(i, ramp(RAMPS.barrier, b ? b[i] : 0), sh(i));
      }
      legend = { title: 'Crossing cost (barrier)', gradient: { stops: RAMPS.barrier, unit: '' }, items: [
        { color: [235, 60, 50], label: 'Border river (divides land)' }, { color: [60, 210, 120], label: 'Backbone river (holds its valley together)' },
      ] };
      break;
    }
    case 'states': {
      const st = f['state'], pk = f['province_kind'];
      for (let i = 0; i < n; i++) {
        const k = pk ? pk[i] : isSea(i) ? 3 : 0;
        if (k >= 2) { put(i, k === 2 ? [110, 160, 215] : [40, 70, 120]); continue; }
        let c = stateColor(st ? st[i] : 0);
        if (k === 1) c = [c[0] / 3 + 110, c[1] / 3 + 110, c[2] / 3 + 110];
        put(i, c, 0.9 + 0.1 * sh(i));
      }
      legend = { title: 'States', items: [
        { color: [25, 25, 25], label: 'State border' }, { color: [120, 120, 120], label: 'Thin line: province border' },
        { color: [170, 170, 165], label: 'Pale: wasteland' }, { color: [110, 160, 215], label: 'Lake province' }, { color: [40, 70, 120], label: 'Sea zone' },
      ] };
      break;
    }
    case 'regions': {
      const rg = f['region'], ct = f['continent'];
      for (let i = 0; i < n; i++) {
        const r = rg ? rg[i] : 0;
        if (!r) { put(i, isSea(i) ? [36, 60, 100] : relief(i)); continue; }
        const tint = ct ? ((ct[i] * 53) % 5) * 0.05 : 0;
        put(i, stateColor(r * 7 + 3), 0.92 + tint);
      }
      legend = { title: 'Regions', items: [{ color: [20, 20, 20], label: 'Region border' }, { color: [120, 120, 120], label: 'Thin line: state border' }] };
      break;
    }
    case 'springs': {
      const gw = f['groundwater'], st = f['site'], sk = f['site_kind'];
      for (let i = 0; i < n; i++) {
        if (isSea(i) || (wm && wm[i] === 2)) { put(i, relief(i), 0.6); continue; }
        if (sk && sk[i] === 3) { put(i, [255, 60, 200]); continue; }
        if (st && st[i] > 0) { put(i, mix([120, 230, 120], [20, 140, 40], Math.min(1, st[i])) as RGB); continue; }
        const v = gw ? Math.log10(1 + gw[i]) / 2.5 : 0;
        put(i, mix([225, 210, 170], [60, 110, 200], Math.min(1, v)) as RGB, 0.85 + 0.15 * sh(i));
      }
      legend = { title: 'Groundwater flow and springs', items: [
        { color: [225, 210, 170], label: 'Little groundwater' }, { color: [60, 110, 200], label: 'Much groundwater (flows downhill underground)' },
        { color: [40, 170, 60], label: 'Spring in dry land (oasis)' }, { color: [255, 60, 200], label: 'Site pin' },
      ] };
      break;
    }
    case 'resources': {
      const tg = f['trade_good'], pk = f['province_kind'];
      for (let i = 0; i < n; i++) {
        const k = pk ? pk[i] : isSea(i) ? 3 : 0;
        if (k >= 2) { put(i, k === 2 ? [110, 160, 215] : [40, 70, 120]); continue; }
        const g = tg ? tg[i] : 0;
        put(i, g > 0 && g <= TRADE_GOODS.length ? TRADE_GOODS[g - 1][1] : [120, 120, 120], 0.85 + 0.15 * sh(i));
      }
      legend = { title: 'Trade goods (hover a province for its deposits)', items: TRADE_GOODS.map(([label, color]) => ({ color, label })) };
      break;
    }
    case 'cultures':
    case 'culture_groups': {
      const cu = f['culture'], gr = f['culture_group'], pk = f['province_kind'];
      for (let i = 0; i < n; i++) {
        const k = pk ? pk[i] : isSea(i) ? 3 : 0;
        if (k >= 2) { put(i, k === 2 ? [110, 160, 215] : [40, 70, 120]); continue; }
        const g = gr ? gr[i] : 0;
        const c = id === 'cultures' ? (cu ? cu[i] : 0) : g;
        if (!c) { put(i, [150, 146, 138], 0.85 + 0.15 * sh(i)); continue; }
        put(i, id === 'cultures' ? cultureColor(c, g) : groupColor(g), 0.9 + 0.1 * sh(i));
      }
      legend = { title: id === 'cultures' ? 'Cultures (hue = group)' : 'Culture groups', items: [
        { color: [20, 20, 20], label: 'Group border' },
        ...(id === 'cultures' ? [{ color: [90, 90, 90] as RGB, label: 'Thin line: culture border' }] : []),
        { color: [150, 146, 138], label: 'Unsettled land' },
      ] };
      break;
    }
    case 'attraction': {
      const at = f['attraction'], pk = f['province_kind'];
      for (let i = 0; i < n; i++) {
        const k = pk ? pk[i] : isSea(i) ? 3 : 0;
        if (k >= 2) { put(i, k === 2 ? [110, 160, 215] : [40, 70, 120]); continue; }
        const a = at ? at[i] : 0;
        const c: RGB = a > 0 ? mix([200, 196, 186], [230, 150, 20], a) : mix([200, 196, 186], [90, 90, 160], -a);
        put(i, c, 0.85 + 0.15 * sh(i));
      }
      legend = { title: 'Attraction (culture simulation)', items: [
        { color: [230, 150, 20], label: 'Draws people (metropolis)' }, { color: [200, 196, 186], label: 'Neutral' }, { color: [90, 90, 160], label: 'Drives people away (ghost town)' },
      ] };
      break;
    }
    case 'population': {
      const pd = f['population'], pk = f['province_kind'];
      for (let i = 0; i < n; i++) {
        const k = pk ? pk[i] : isSea(i) ? 3 : 0;
        if (k >= 2) { put(i, k === 2 ? [110, 160, 215] : [40, 70, 120]); continue; }
        put(i, ramp(RAMPS.density, pd ? pd[i] : 0), 0.85 + 0.15 * sh(i));
      }
      legend = { title: 'Population density (people per km²)', gradient: { stops: RAMPS.density, unit: '/km²' } };
      break;
    }
    case 'provinces': {
      const pv = f['province'], pk = f['province_kind'];
      for (let i = 0; i < n; i++) {
        const k = pk ? pk[i] : 0;
        let c = provinceColor(pv ? pv[i] : 0);
        if (k === 3) c = [c[0] * 0.25 + 30, c[1] * 0.3 + 50, c[2] * 0.35 + 95];
        else if (k === 2) c = [c[0] * 0.3 + 90, c[1] * 0.3 + 130, c[2] * 0.3 + 180];
        else if (k === 1) c = [c[0] * 0.25 + 120, c[1] * 0.25 + 118, c[2] * 0.25 + 112];
        put(i, c);
      }
      legend = { title: 'Provinces', items: [
        { color: [25, 25, 25], label: 'State border' }, { color: [160, 158, 150], label: 'Grey: wasteland' },
        { color: [120, 160, 210], label: 'Lake province' }, { color: [50, 80, 130], label: 'Sea zone (coastal, shelf, open)' },
      ] };
      break;
    }
  }
  return { rgba: out, legend };
}

export const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];

function mix(a: RGB, b: RGB, t: number): RGB {
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
}
