// UI description of the pipeline steps (seven for Stage 1, three for Stage 2,
// one for Stage 3):
// parameters, default layer and tools.

import type { LayerId } from './layers';

export type Param = {
  group: string; // key in WorldParams
  key: string;
  label: string;
  min?: number;
  max?: number;
  step?: number;
  options?: [number, string][];
  hint?: string;
};

export type ToolId =
  | 'navigate' | 'land' | 'sea' | 'mountain' | 'erase_hint' | 'scatter_land' | 'scatter_sea' | 'scatter_mountain' | 'pin' | 'arrow' | 'plate_paint'
  | 'raise' | 'lower' | 'smooth' | 'flatten' | 'biome_paint' | 'biome_erase'
  | 'barrier_paint' | 'barrier_erase' | 'site_pin' | 'state_paint' | 'province_paint';

export type ToolDef = {
  id: ToolId;
  label: string;
  key?: string; // keyboard shortcut
  step: string; // step the edit belongs to (auto-run target)
  rust?: string; // core Tool name when it differs from the id
  scatter?: boolean; // noisy natural edge (scatter brush)
  value?: 'metres' | 'plate' | 'terrain' | 'pin' | 'speed' | 'barrier' | 'people';
  defaultValue?: number;
  hint: string;
};

export const TOOLS: ToolDef[] = [
  { id: 'navigate', label: 'Navigate', key: 'v', step: '', hint: 'Drag to rotate or pan, scroll to zoom. Right-drag navigates with any tool.' },
  { id: 'land', label: 'Land', key: 'l', step: 'sketch', hint: 'Paint land with a soft brush.' },
  { id: 'sea', label: 'Sea', key: 's', step: 'sketch', hint: 'Paint sea.' },
  { id: 'mountain', label: 'Mountains here', key: 'm', step: 'sketch', hint: '“Mountains here” hint strokes raise ridged relief.' },
  { id: 'erase_hint', label: 'Erase hint', step: 'sketch', hint: 'Remove mountain hints.' },
  { id: 'scatter_land', label: 'Scatter land', key: 'n', step: 'sketch', rust: 'land', scatter: true, hint: 'Land with a fractal edge: ragged coasts, bays and offshore islands beyond the rim.' },
  { id: 'scatter_sea', label: 'Scatter sea', step: 'sketch', rust: 'sea', scatter: true, hint: 'Sea with a fractal edge: fjords, inlets, lakes and leftover islets.' },
  { id: 'scatter_mountain', label: 'Scatter mountains', step: 'sketch', rust: 'mountain', scatter: true, hint: 'Broken, patchy mountain hints instead of a smooth ridge.' },
  { id: 'pin', label: 'Plate pin', key: 'p', step: 'plates', value: 'pin', hint: 'Click to force a plate seed here.' },
  { id: 'arrow', label: 'Motion arrow', key: 'a', step: 'plates', value: 'speed', defaultValue: 50, hint: 'Drag in the direction the plate under the start point should move.' },
  { id: 'plate_paint', label: 'Plate paint', step: 'plates', value: 'plate', defaultValue: 0, hint: 'Reassign cells to a plate (override layer).' },
  { id: 'raise', label: 'Raise', key: 'r', step: 'relief', value: 'metres', defaultValue: 600, hint: 'Raise elevation (override layer).' },
  { id: 'lower', label: 'Lower', step: 'relief', value: 'metres', defaultValue: 600, hint: 'Lower elevation (override layer).' },
  { id: 'smooth', label: 'Smooth', step: 'relief', hint: 'Smooth elevation (override layer).' },
  { id: 'flatten', label: 'Flatten', step: 'relief', value: 'metres', defaultValue: 200, hint: 'Flatten toward a target elevation (override layer).' },
  { id: 'biome_paint', label: 'Biome paint', key: 'b', step: 'biomes', value: 'terrain', defaultValue: 5, hint: 'Force a game terrain (override layer).' },
  { id: 'biome_erase', label: 'Biome erase', step: 'biomes', hint: 'Remove biome paint.' },
  { id: 'barrier_paint', label: 'Add barrier', key: 'k', step: 'habitability', value: 'barrier', defaultValue: 6, hint: 'Make land costly to cross, so state and province borders follow your stroke.' },
  { id: 'barrier_erase', label: 'Remove barrier', step: 'habitability', hint: 'Remove barriers under the brush, including border rivers.' },
  { id: 'site_pin', label: 'Site pin', key: 't', step: 'habitability', value: 'people', defaultValue: 50000, hint: 'Click to place a town no model explains (a gambling city, an oil port). It becomes habitable land and, in the last culture era, holds this many people.' },
  { id: 'state_paint', label: 'Grow state', key: 'e', step: 'states', hint: 'Start the stroke inside a state, then paint: every land cell you cover joins that state.' },
  { id: 'province_paint', label: 'Grow province', key: 'o', step: 'provinces', hint: 'Start inside a province, then paint over its neighbours in the same state.' },
];

/** Index of the first Stage 2 step. */
export const STAGE2_START = 7;
/** Index of the first Stage 3 step. */
export const STAGE3_START = 10;

export type StepUI = {
  key: string;
  title: string;
  layer: LayerId | null;
  tools: ToolId[];
  params: Param[];
  blurb: string;
};

const p = (group: string, key: string, label: string, min: number, max: number, step: number, hint?: string): Param => ({ group, key, label, min, max, step, hint });

export const STEPS: StepUI[] = [
  {
    key: 'planet', title: 'Planet parameters', layer: null, tools: [],
    blurb: 'Size, orbit and atmosphere. The seed makes every later step reproducible.',
    params: [
      p('planet', 'seed', 'Seed', 0, 4294967295, 1),
      { group: 'planet', key: 'grid_level', label: 'Grid resolution', options: [[6, '6 · 40,962 cells (preview)'], [7, '7 · 163,842 cells'], [8, '8 · 655,362 cells (default)'], [9, '9 · 2,621,442 cells (slow)']] },
      p('planet', 'radius_km', 'Radius (km)', 1000, 20000, 1),
      p('planet', 'axial_tilt_deg', 'Axial tilt (°)', 0, 90, 0.1),
      p('planet', 'day_length_h', 'Day length (h)', 4, 240, 0.5, 'Faster rotation narrows the wind cells and the poleward heat transport.'),
      p('planet', 'year_length_days', 'Year length (days)', 30, 3000, 1),
      p('planet', 'eccentricity', 'Eccentricity', 0, 0.5, 0.001),
      p('planet', 'perihelion_deg', 'Perihelion longitude (°)', 0, 360, 1),
      p('planet', 'solar_constant', 'Solar constant (W/m²)', 600, 2500, 1),
      p('planet', 'greenhouse', 'Greenhouse strength', 0.1, 8, 0.05, '1 = Earth. Each doubling adds about 30 W/m² of forcing.'),
      p('planet', 'ocean_fraction', 'Target ocean fraction', 0.05, 0.95, 0.01, 'Used when continents are generated automatically.'),
    ],
  },
  {
    key: 'sketch', title: 'Continent sketch', layer: 'sketch', tools: ['land', 'sea', 'scatter_land', 'scatter_sea', 'mountain', 'scatter_mountain', 'erase_hint'],
    blurb: 'Paint where the land is. Soft brush edges let coastline noise decide the exact shore.',
    params: [
      p('sketch', 'auto_continents', 'Generated continents', 0, 12, 1, 'Only used when “Start from generated continents” is on.'),
      p('sketch', 'coast_roughness', 'Coast roughness', 0, 1.5, 0.05),
      p('sketch', 'coast_scale_km', 'Coast feature size (km)', 100, 3000, 10),
    ],
  },
  {
    key: 'plates', title: 'Plates', layer: 'plates', tools: ['pin', 'arrow', 'plate_paint'],
    blurb: 'Continental plates are seeded inside landmasses; oceanic plates fill the rest and all grow by weighted flood fill.',
    params: [
      p('plates', 'major_plates', 'Major plates', 2, 40, 1),
      p('plates', 'minor_plates', 'Minor plates', 0, 80, 1),
      p('plates', 'continental_plate_area_mkm2', 'Land per continental plate (M km²)', 2, 120, 1),
      p('plates', 'shelf_width_km', 'Continental shelf (km)', 0, 600, 10),
      p('plates', 'speed_scale', 'Plate speed ×', 0.1, 4, 0.05),
      p('plates', 'edge_roughness', 'Edge roughness', 0, 1.5, 0.05),
    ],
  },
  {
    key: 'relief', title: 'Tectonic relief', layer: 'elevation', tools: ['raise', 'lower', 'smooth', 'flatten'],
    blurb: 'Boundaries are classified by relative plate motion; each type adds its cross-section profile.',
    params: [
      p('tectonics', 'sketch_fidelity', 'Sketch fidelity', 0, 1, 0.01, '0 = pure tectonics, 1 = the sketched coastline always wins.'),
      p('tectonics', 'mountain_scale', 'Mountain height ×', 0.1, 3, 0.05),
      p('tectonics', 'detail_scale', 'Detail noise ×', 0, 3, 0.05),
      p('tectonics', 'hotspots', 'Hotspots', 0, 40, 1),
      p('tectonics', 'hint_height_m', 'Mountain hint height (m)', 0, 8000, 50),
      p('tectonics', 'max_influence_km', 'Boundary influence (km)', 300, 4000, 50),
    ],
  },
  {
    key: 'climate', title: 'Climate', layer: 'temperature', tools: [],
    blurb: 'Seasonal energy balance by latitude, three-cell winds, maritime air, coastal currents and moisture transport.',
    params: [
      { group: 'climate', key: 'climate_level', label: 'Solve on grid level', options: [[5, '5 (fast)'], [6, '6'], [7, '7 (default)'], [8, '8 (slow)']] },
      p('climate', 'lapse_rate_c_per_km', 'Lapse rate (°C/km)', 0, 12, 0.1),
      p('climate', 'heat_diffusion', 'Poleward heat transport', 0.1, 2, 0.01),
      p('climate', 'global_precip_mm', 'Global precipitation (mm/yr)', 100, 3000, 10),
      p('climate', 'ocean_current_c', 'Coastal currents (°C)', 0, 15, 0.5),
      p('climate', 'maritime_reach_km', 'Maritime reach (km)', 100, 5000, 50),
      p('climate', 'orographic', 'Orographic rain ×', 0, 4, 0.05),
      p('climate', 'moisture_passes', 'Moisture passes', 20, 300, 5),
      p('climate', 'thermal_wind', 'Thermal winds ×', 0, 3, 0.05, 'Winds driven by temperature-made pressure systems: monsoons, winter highs over continents, summer heat lows, plateau lows. 0 = zonal belts only.'),
      p('climate', 'mountain_blocking', 'Mountain blocking', 0, 1, 0.05, 'Share of upslope wind turned along steep terrain: air flows around ranges and through gaps instead of over them.'),
    ],
  },
  {
    key: 'hydrology', title: 'Hydrology & erosion', layer: 'discharge', tools: [],
    blurb: 'Priority-flood lakes, flow accumulation from runoff, rivers by discharge, stream-power and thermal erosion.',
    params: [
      p('hydrology', 'river_threshold_m3s', 'River threshold (m³/s)', 5, 5000, 5),
      p('hydrology', 'width_coeff', 'River width coefficient', 0.5, 30, 0.1, 'Bankfull width w = a·Q^b in metres (Q in m³/s). a = 5, b = 0.5 gives ~110 m for a Seine-sized river (500 m³/s) and ~900 m for the Yangtze (30,000 m³/s).'),
      p('hydrology', 'width_exponent', 'River width exponent', 0.3, 0.7, 0.01, 'b in w = a·Q^b. Observed rivers fall around 0.4–0.6.'),
      p('hydrology', 'min_river_length_km', 'Minimum river length (km)', 0, 1000, 10, 'River systems shorter than this from source to mouth are not counted as rivers (hides one-cell coastal stubs).'),
      p('hydrology', 'channel_loss', 'Channel loss per 100 km', 0, 0.2, 0.005, 'Share of flow lost per 100 km of fully arid land (evaporation and seepage). Desert rivers like the Nile narrow downstream.'),
      p('hydrology', 'erosion_passes', 'Erosion passes', 0, 20, 1),
      p('hydrology', 'erosion_strength', 'Erosion strength', 0, 5, 0.05),
      p('hydrology', 'thermal_passes', 'Thermal passes per pass', 0, 10, 1),
      p('hydrology', 'talus_m_per_km', 'Talus slope (m/km)', 5, 300, 1),
      p('hydrology', 'lake_min_depth_m', 'Minimum lake depth (m)', 1, 500, 1, 'Basins shallower than this after outlet incision hold no lake.'),
      p('hydrology', 'breach_depth_m', 'Outlet incision (m)', 0, 1000, 10, 'How far rivers cut into basin rims. Larger values drain more basins and leave smaller, deeper lakes.'),
      p('hydrology', 'open_water_evap', 'Open-water evaporation ×', 0.5, 2.5, 0.05, 'Lake evaporation relative to land potential evapotranspiration. Higher values shrink terminal lakes.'),
      p('hydrology', 'inland_sea_max_mkm2', 'Inland sea limit (M km²)', 0, 20, 0.1, 'Isolated below-sea-level water bodies smaller than this become basins whose level follows their water balance.'),
      { group: 'hydrology', key: 'lake_climate_feedback', label: 'Lake–climate feedback', options: [[0, 'Off'], [1, '1 pass (default)'], [2, '2 passes']], hint: 'Re-solve the climate with lakes as open water: evaporation, lake-effect rain, milder shores.' },
    ],
  },
  {
    key: 'biomes', title: 'Biomes', layer: 'koppen', tools: ['biome_paint', 'biome_erase'],
    blurb: 'Köppen–Geiger classes from monthly temperature and precipitation, mapped to game terrains.',
    params: [
      { group: 'biomes', key: 'cd_threshold_c', label: 'C/D boundary', options: [[0, '0 °C (Köppen–Geiger)'], [-3, '−3 °C (original Köppen)']] },
      p('biomes', 'mountain_elev_m', 'Mountain elevation (m)', 300, 6000, 50),
      p('biomes', 'mountain_relief_m', 'Mountain relief (m)', 100, 4000, 25),
      p('biomes', 'hill_relief_m', 'Hill relief (m)', 50, 2000, 10),
      p('biomes', 'forest_precip_mm', 'Forest precipitation (mm/yr)', 100, 3000, 10),
    ],
  },
  {
    key: 'habitability', title: 'Habitability & barriers', layer: 'habitability', tools: ['barrier_paint', 'barrier_erase', 'site_pin'],
    blurb: 'How well each cell supports people (growing season, water, slope, terrain, rivers, coast), and how costly it is to cross: ridges, border rivers, desert, ice and marsh. Groundwater from the uplands surfaces as springs at mountain feet and basin floors: oases in dry land. Site pins place towns no model explains.',
    params: [
      p('habitability', 'river_reach_km', 'River reach (km)', 5, 300, 5, 'Distance over which a river still waters the land and gives transport.'),
      p('habitability', 'coast_reach_km', 'Coast reach (km)', 5, 500, 5),
      p('habitability', 'barrier_ridge', 'Ridge crest barrier', 0, 20, 0.1),
      p('habitability', 'barrier_mountain', 'High ground barrier', 0, 20, 0.1),
      p('habitability', 'barrier_river', 'Border river barrier', 0, 20, 0.1),
      p('habitability', 'barrier_desert', 'Desert barrier', 0, 20, 0.1),
      p('habitability', 'barrier_ice', 'Ice barrier', 0, 30, 0.5),
      p('habitability', 'barrier_marsh', 'Marsh barrier', 0, 20, 0.1),
      p('habitability', 'river_border_width_m', 'Border river width (m)', 20, 3000, 10, 'Rivers at least this wide can be borders.'),
      p('habitability', 'river_border_habitability', 'Border river habitability', 0, 1, 0.01, 'A wide river divides land at least this habitable (Rhine).'),
      p('habitability', 'backbone_max_precip_mm', 'Backbone river rainfall (mm/yr)', 0, 1500, 10, 'A river through land drier than this holds its valley together instead (Nile): states grow along it.'),
      p('habitability', 'groundwater_recharge', 'Groundwater recharge', 0, 1, 0.01, 'Share of rain above 150 mm/yr that soaks in.'),
      p('habitability', 'groundwater_reach_km', 'Groundwater reach (km)', 20, 3000, 10, 'How far groundwater flows underground before most of it is lost.'),
      p('habitability', 'spring_flux_m3s', 'Spring flow for a full oasis (m³/s)', 0.05, 50, 0.05),
      p('habitability', 'spring_max_precip_mm', 'Springs matter below (mm/yr)', 0, 1500, 10, 'Only land drier than this gets oases.'),
      p('habitability', 'spring_habitability', 'Oasis habitability', 0, 1, 0.01),
    ],
  },
  {
    key: 'states', title: 'States', layer: 'states', tools: ['state_paint'],
    blurb: 'Seeds weighted by habitability grow together; borders settle on barriers, and small islands join the state across the shortest strait. States form regions and continents.',
    params: [
      p('states', 'state_area_km2', 'Mean state area (km²)', 10000, 2000000, 1000),
      p('states', 'min_state_area_km2', 'Minimum state area (km²)', 0, 500000, 1000),
      p('states', 'habitability_density', 'Smaller states on fertile land', 0, 1, 0.05, '0 = equal sizes. 1 = states in river valleys are much smaller than in steppe or tundra.'),
      p('states', 'size_variation', 'Size variation', 0, 2, 0.05, 'Random head start of each seed.'),
      p('states', 'barrier_weight', 'Barrier weight', 0, 5, 0.05),
      p('states', 'sea_crossing', 'Sea crossing cost', 1, 50, 0.5, 'Cost per km of water relative to open land. Lower values let states span straits.'),
      p('states', 'island_state_km2', 'Own state from island size (km²)', 0, 1000000, 1000),
      p('states', 'states_per_region', 'States per region', 1, 30, 0.5),
      p('states', 'continent_min_mkm2', 'Continent size (M km²)', 0.1, 50, 0.1),
    ],
  },
  {
    key: 'provinces', title: 'Provinces', layer: 'provinces', tools: ['province_paint'],
    blurb: 'Each state is cut into provinces, smaller where more people can live. Peaks, ice and deep desert become wasteland; the sea is split into coastal, shelf and open-ocean zones.',
    params: [
      p('provinces', 'province_area_km2', 'Province area, fertile (km²)', 1000, 200000, 500),
      p('provinces', 'sparse_province_area_km2', 'Province area, barren (km²)', 1000, 1000000, 1000),
      p('provinces', 'min_province_area_km2', 'Minimum province area (km²)', 0, 100000, 500),
      p('provinces', 'barrier_weight', 'Barrier weight', 0, 5, 0.05),
      p('provinces', 'lloyd_passes', 'Relaxation passes', 0, 8, 1),
      p('provinces', 'wasteland_habitability', 'Wasteland below habitability', 0, 0.5, 0.01),
      p('provinces', 'wasteland_elev_m', 'Wasteland above (m)', 500, 9000, 50),
      p('provinces', 'wasteland_area_km2', 'Wasteland province area (km²)', 5000, 2000000, 5000),
      p('provinces', 'coastal_band_km', 'Coastal sea band (km)', 20, 1000, 10),
      p('provinces', 'shelf_depth_m', 'Shelf depth (m)', 50, 6000, 50),
      p('provinces', 'coastal_sea_km2', 'Coastal sea zone (km²)', 2000, 1000000, 1000),
      p('provinces', 'shelf_sea_km2', 'Shelf sea zone (km²)', 5000, 3000000, 5000),
      p('provinces', 'open_sea_km2', 'Open ocean zone (km²)', 20000, 10000000, 10000),
      p('provinces', 'lake_province_km2', 'Lake province from (km²)', 100, 500000, 100),
      p('provinces', 'max_strait_km', 'Longest strait crossing (km)', 0, 1000, 5),
    ],
  },
  {
    key: 'cultures', title: 'Cultures', layer: 'cultures', tools: [],
    blurb: 'Bands of people spread over the provinces, grow, split and meet. Contact makes them alike, isolation and drift make them differ; where contact stays rare, cultures split. Groups are cultures that stay in touch.',
    params: [
      p('cultures', 'ticks', 'Generations', 20, 2000, 10, 'One generation is a tick of the simulation.'),
      p('cultures', 'years_per_tick', 'Years per generation', 10, 50, 1),
      p('cultures', 'initial_bands', 'Founding bands', 1, 1000, 1, 'Bands placed at the start. Few founders give a family tree of splits; many give unrelated cultures that merge.'),
      p('cultures', 'max_bands', 'Band limit', 100, 20000, 100, 'Never below the number of habitable provinces (about one band each); more bands are slower.'),
      p('cultures', 'density_per_km2', 'People per km² (fertile land)', 0.1, 100, 0.1),
      p('cultures', 'contact_scale_km', 'Contact distance (km)', 20, 2000, 10, 'Most contact is within about this distance.'),
      p('cultures', 'homophily', 'Like seeks like', 0, 10, 0.1, 'Higher values make unlike bands avoid each other, so cultures form sharper borders.'),
      p('cultures', 'conformity', 'Conformity', 0, 1, 0.05, 'Chance per generation that a band takes the trait most of its contacts share.'),
      p('cultures', 'drift', 'Drift per trait', 0, 0.02, 0.0001, 'Chance per trait and generation of a random change.'),
      p('cultures', 'barrier_weight', 'Barrier weight', 0, 5, 0.05),
      p('cultures', 'sea_cost', 'Sea travel cost', 0.1, 5, 0.05, 'Cost per km at sea once sailing is possible (era 3 coastal, era 4 open sea).'),
      p('cultures', 'culture_resolution', 'Culture detail', 0.5, 20, 0.5, 'Higher values find more, smaller cultures.'),
      p('cultures', 'group_resolution', 'Group detail', 0.005, 2, 0.005, 'Lower values make fewer, larger culture groups.'),
      p('cultures', 'persistence', 'Checks before a change counts', 1, 10, 1, 'A split, merge or change of culture must hold this many checks in a row.'),
      p('cultures', 'caravan_people', 'Caravan stop size (people)', 0, 500000, 1000, 'From era 2: people a watered stop on the busiest route across dry land can hold.'),
      p('cultures', 'mining_people', 'Mining town size (people)', 0, 200000, 500, 'From era 3: people each metal deposit draws.'),
      p('cultures', 'irrigation_share', 'Irrigated share of dry river land', 0, 1, 0.05, 'From era 4: share of a dry province with a river that becomes farmland.'),
    ],
  },
];
