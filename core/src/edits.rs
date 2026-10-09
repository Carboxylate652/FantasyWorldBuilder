//! User input that is not a parameter: the continent sketch and the override
//! layers. Everything is stored as resolution-independent vector strokes in
//! lat/lon, so edits survive grid-level changes and regeneration.

use crate::grid::Grid;
use crate::vec3::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    // Sketch tools (step 2)
    Land,
    Sea,
    Mountain,
    EraseHint,
    // Plate override (step 3)
    PlatePaint,
    // Relief overrides (step 4)
    Raise,
    Lower,
    Smooth,
    Flatten,
    // Biome override (step 7)
    BiomePaint,
    BiomeErase,
    // Barrier override (step 8)
    BarrierPaint,
    BarrierErase,
    // Political overrides (steps 9 and 10): the stroke's first point picks the
    // state or province that grows over the painted cells.
    StatePaint,
    ProvincePaint,
    // Fertility (applied with habitability, used from state making on): paint
    // fertile (value > 0) or barren (value < 0) land, or erase the paint.
    FertilityPaint,
    FertilityErase,
    // Map editor (drag from the first point to the last): merge the province
    // or state under the first point into the one under the last; move the
    // province under the first point into the state under the last.
    ProvinceMerge,
    StateMerge,
    ProvinceToState,
    // Rename the province or state under the first point to `name`.
    RenameProvince,
    RenameState,
    // Goods editor: the provinces under the brush get a trade good (value
    // 1–14, as in resources::TRADE_GOODS), gain a deposit (101–106) or lose
    // one (201–207, as in resources::DEPOSITS).
    GoodsPaint,
    // Founding-band pins for the culture simulation: value = bands. A drag
    // that starts on a pin moves it; erase removes the nearest pin.
    BandPin,
    BandErase,
    // Attraction (culture simulation only): value +1 draws people (a
    // metropolis), −1 drives them away (a ghost town); erase removes it.
    Attraction,
    AttractionErase,
    // Settlement site pin (step 8): a town no model explains (a gambling city,
    // an oil port). The stroke's first point places it; `value` is the
    // population it should reach.
    SitePin,
}

impl Tool {
    pub fn layer(self) -> EditLayer {
        match self {
            Tool::Land | Tool::Sea | Tool::Mountain | Tool::EraseHint => EditLayer::Sketch,
            Tool::PlatePaint => EditLayer::Plates,
            Tool::Raise | Tool::Lower | Tool::Smooth | Tool::Flatten => EditLayer::Elevation,
            Tool::BiomePaint | Tool::BiomeErase => EditLayer::Biomes,
            Tool::BarrierPaint | Tool::BarrierErase => EditLayer::Barriers,
            Tool::StatePaint | Tool::StateMerge | Tool::RenameState => EditLayer::States,
            Tool::ProvincePaint | Tool::ProvinceMerge | Tool::ProvinceToState | Tool::RenameProvince | Tool::GoodsPaint => EditLayer::Provinces,
            Tool::SitePin => EditLayer::Sites,
            Tool::FertilityPaint | Tool::FertilityErase => EditLayer::Fertility,
            Tool::BandPin | Tool::BandErase => EditLayer::Bands,
            Tool::Attraction | Tool::AttractionErase => EditLayer::Attraction,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditLayer {
    Sketch,
    Plates,
    Elevation,
    Biomes,
    Barriers,
    States,
    Provinces,
    Sites,
    Fertility,
    Bands,
    Attraction,
    /// Time-stamped steering of the Stage 3 and 4 simulations (not strokes).
    Directives,
}

impl EditLayer {
    pub const ALL: [EditLayer; 12] = [
        EditLayer::Sketch,
        EditLayer::Plates,
        EditLayer::Elevation,
        EditLayer::Biomes,
        EditLayer::Barriers,
        EditLayer::Sites,
        EditLayer::Fertility,
        EditLayer::States,
        EditLayer::Provinces,
        EditLayer::Bands,
        EditLayer::Attraction,
        EditLayer::Directives,
    ];

    /// Name used in files, the API and the CLI (same as the serde name).
    pub fn key(self) -> &'static str {
        match self {
            EditLayer::Sketch => "sketch",
            EditLayer::Plates => "plates",
            EditLayer::Elevation => "elevation",
            EditLayer::Biomes => "biomes",
            EditLayer::Barriers => "barriers",
            EditLayer::States => "states",
            EditLayer::Provinces => "provinces",
            EditLayer::Sites => "sites",
            EditLayer::Fertility => "fertility",
            EditLayer::Bands => "bands",
            EditLayer::Attraction => "attraction",
            EditLayer::Directives => "directives",
        }
    }

    pub fn from_key(k: &str) -> Option<EditLayer> {
        EditLayer::ALL.into_iter().find(|l| l.key() == k)
    }

    /// The first step that reads this layer: editing it makes that step and
    /// everything after it stale.
    pub fn step(self) -> crate::stages::Step {
        use crate::stages::Step;
        match self {
            EditLayer::Sketch => Step::Sketch,
            EditLayer::Plates => Step::Plates,
            EditLayer::Elevation => Step::Relief,
            EditLayer::Biomes => Step::Biomes,
            EditLayer::Barriers | EditLayer::Sites | EditLayer::Fertility => Step::Habitability,
            EditLayer::States => Step::States,
            EditLayer::Provinces => Step::Provinces,
            EditLayer::Bands | EditLayer::Attraction | EditLayer::Directives => Step::Cultures,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            EditLayer::Sketch => "Continent sketch",
            EditLayer::Plates => "Plate paint",
            EditLayer::Elevation => "Relief brushes",
            EditLayer::Biomes => "Biome paint",
            EditLayer::Barriers => "Barriers",
            EditLayer::States => "States (paint, merge, rename)",
            EditLayer::Provinces => "Provinces (paint, merge, move, rename, goods)",
            EditLayer::Sites => "Site pins",
            EditLayer::Fertility => "Fertility",
            EditLayer::Bands => "Founding bands",
            EditLayer::Attraction => "Attraction",
            EditLayer::Directives => "Simulation directives (Stages 3 and 4)",
        }
    }
}

static NO_STROKES: Vec<Stroke> = Vec::new();

/// Format tag of an override bundle file (`*.fwm-overrides.json`).
pub const BUNDLE_FORMAT: &str = "fwm-overrides";
pub const BUNDLE_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Stroke {
    pub tool: Tool,
    pub radius_km: f64,
    /// 0..1 opacity of the brush.
    pub strength: f64,
    /// 0 = soft falloff over the whole radius, 1 = hard edge.
    pub hardness: f64,
    /// Tool-specific value: metres for raise/lower/flatten, plate id, terrain id.
    pub value: f64,
    /// Points as [lat, lon] in degrees.
    pub points: Vec<[f64; 2]>,
    /// Scatter brush: 0 = smooth edge; up to 1 breaks the edge into fractal
    /// coastline, bays and offshore islands.
    pub scatter: f64,
    /// Feature size of the scatter noise (km).
    pub scatter_scale_km: f64,
    /// Per-stroke noise seed, so every scatter stroke looks different but replays identically.
    pub seed: u64,
    /// New name (rename tools).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke { tool: Tool::Land, radius_km: 400.0, strength: 1.0, hardness: 0.5, value: 0.0, points: vec![], scatter: 0.0, scatter_scale_km: 250.0, seed: 0, name: String::new() }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PinKind {
    Auto,
    Continental,
    Oceanic,
}

/// Forces a plate seed at this location.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlatePin {
    pub lat: f64,
    pub lon: f64,
    pub kind: PinKind,
}

/// Sets the motion of the plate under this point.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MotionArrow {
    pub lat: f64,
    pub lon: f64,
    /// Compass bearing of motion, degrees clockwise from north.
    pub bearing_deg: f64,
    pub speed_mm_yr: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Sketch {
    /// Start from generated continents (from the seed) instead of an empty ocean.
    pub auto_base: bool,
    pub strokes: Vec<Stroke>,
    pub pins: Vec<PlatePin>,
    pub arrows: Vec<MotionArrow>,
}

impl Default for Sketch {
    fn default() -> Self {
        Sketch { auto_base: true, strokes: vec![], pins: vec![], arrows: vec![] }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Overrides {
    pub plates: Vec<Stroke>,
    pub elevation: Vec<Stroke>,
    pub biomes: Vec<Stroke>,
    pub barriers: Vec<Stroke>,
    pub states: Vec<Stroke>,
    pub provinces: Vec<Stroke>,
    pub sites: Vec<Stroke>,
    pub fertility: Vec<Stroke>,
    pub bands: Vec<Stroke>,
    pub attraction: Vec<Stroke>,
    /// Steering of the Stage 3 and 4 simulations, by hand or by the AI guide.
    pub directives: Vec<crate::directives::Directive>,
}

/// How heightmap pixel values map to metres.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum HeightEncoding {
    /// Our heightmap16.png: metres = v / 65535 · 20000 − 11000.
    Heightmap16,
    /// Our 8-bit Paradox-style heightmap.png (inverse of the export mapping).
    Paradox8 { sea_level_value: f64, max_elevation_m: f64, max_depth_m: f64 },
    /// Linear: black = min_m, white = max_m (any bit depth).
    Linear { min_m: f64, max_m: f64 },
}

/// An equirectangular heightmap that replaces (or blends into) the tectonic relief.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ElevationImport {
    pub path: String,
    pub encoding: HeightEncoding,
    pub lat_min: f64,
    pub lat_max: f64,
    /// 1 = replace the tectonic relief, 0 = ignore the import.
    pub blend: f64,
    /// Content hash of the file when it was imported (cache key).
    pub content_hash: String,
}

/// An edited provinces.png (and optionally definition.csv) that replaces the
/// generated provinces. Each cell takes its province by majority pixel vote.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProvinceImport {
    pub png: String,
    pub csv: Option<String>,
    pub lat_min: f64,
    pub lat_max: f64,
    /// Content hash of both files when imported (cache key).
    pub content_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Imports {
    pub elevation: Option<ElevationImport>,
    pub provinces: Option<ProvinceImport>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Edits {
    pub sketch: Sketch,
    pub overrides: Overrides,
    pub imports: Imports,
}

impl Edits {
    pub fn add_stroke(&mut self, s: Stroke) {
        match s.tool.layer() {
            EditLayer::Sketch => self.sketch.strokes.push(s),
            EditLayer::Plates => self.overrides.plates.push(s),
            EditLayer::Elevation => self.overrides.elevation.push(s),
            EditLayer::Biomes => self.overrides.biomes.push(s),
            EditLayer::Barriers => self.overrides.barriers.push(s),
            EditLayer::States => self.overrides.states.push(s),
            EditLayer::Provinces => self.overrides.provinces.push(s),
            EditLayer::Sites => self.overrides.sites.push(s),
            EditLayer::Fertility => self.overrides.fertility.push(s),
            EditLayer::Bands => self.add_band_pin(s),
            EditLayer::Attraction => self.overrides.attraction.push(s),
            EditLayer::Directives => {}
        }
    }

    /// Founding-band pins: a drag that starts within 400 km of a pin moves
    /// it; otherwise the pin goes where the stroke ends. Erase removes the
    /// nearest pin within 600 km.
    fn add_band_pin(&mut self, s: Stroke) {
        let (Some(first), Some(last)) = (s.points.first().copied(), s.points.last().copied()) else { return };
        let km = |a: [f64; 2], b: [f64; 2]| Vec3::from_lat_lon_deg(a[0], a[1]).angle_to(Vec3::from_lat_lon_deg(b[0], b[1])) * 6371.0;
        let pins = &mut self.overrides.bands;
        let nearest = |pins: &Vec<Stroke>, max: f64| {
            pins.iter().enumerate().filter_map(|(k, p)| p.points.first().map(|q| (k, km(*q, first)))).filter(|x| x.1 <= max).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|x| x.0)
        };
        match s.tool {
            Tool::BandErase => {
                if let Some(k) = nearest(pins, 600.0) {
                    pins.remove(k);
                }
            }
            _ => match nearest(pins, 400.0).filter(|_| s.points.len() >= 2 && km(first, last) > 50.0) {
                Some(k) => pins[k].points = vec![last],
                None => pins.push(Stroke { points: vec![last], ..s }),
            },
        }
    }
    /// Strokes of an override layer (the sketch layer: its strokes only).
    pub fn strokes(&self, layer: EditLayer) -> &Vec<Stroke> {
        let o = &self.overrides;
        match layer {
            EditLayer::Sketch => &self.sketch.strokes,
            EditLayer::Plates => &o.plates,
            EditLayer::Elevation => &o.elevation,
            EditLayer::Biomes => &o.biomes,
            EditLayer::Barriers => &o.barriers,
            EditLayer::States => &o.states,
            EditLayer::Provinces => &o.provinces,
            EditLayer::Sites => &o.sites,
            EditLayer::Fertility => &o.fertility,
            EditLayer::Bands => &o.bands,
            EditLayer::Attraction => &o.attraction,
            EditLayer::Directives => &NO_STROKES,
        }
    }

    fn strokes_mut(&mut self, layer: EditLayer) -> &mut Vec<Stroke> {
        let o = &mut self.overrides;
        match layer {
            EditLayer::Sketch => &mut self.sketch.strokes,
            EditLayer::Plates => &mut o.plates,
            EditLayer::Elevation => &mut o.elevation,
            EditLayer::Biomes => &mut o.biomes,
            EditLayer::Barriers => &mut o.barriers,
            EditLayer::States => &mut o.states,
            EditLayer::Provinces => &mut o.provinces,
            EditLayer::Sites => &mut o.sites,
            EditLayer::Fertility => &mut o.fertility,
            EditLayer::Bands => &mut o.bands,
            EditLayer::Attraction => &mut o.attraction,
            EditLayer::Directives => unreachable!("directives are not strokes"),
        }
    }

    /// Number of edits in a layer (the sketch layer counts its plate pins and
    /// motion arrows too).
    pub fn count(&self, layer: EditLayer) -> usize {
        match layer {
            EditLayer::Sketch => self.sketch.strokes.len() + self.sketch.pins.len() + self.sketch.arrows.len(),
            EditLayer::Directives => self.overrides.directives.len(),
            _ => self.strokes(layer).len(),
        }
    }

    /// Remove strokes by index (indices out of range are ignored).
    pub fn remove_strokes(&mut self, layer: EditLayer, indices: &[usize]) -> usize {
        let mut idx: Vec<usize> = indices.to_vec();
        idx.sort_unstable();
        idx.dedup();
        fn drop<T>(v: &mut Vec<T>, idx: &[usize]) -> usize {
            let mut removed = 0;
            for &i in idx.iter().rev() {
                if i < v.len() {
                    v.remove(i);
                    removed += 1;
                }
            }
            removed
        }
        if layer == EditLayer::Directives {
            return drop(&mut self.overrides.directives, &idx);
        }
        drop(self.strokes_mut(layer), &idx)
    }

    /// An override bundle with the given layers: a JSON file that carries
    /// edits to another project or seed. File imports (heightmap, provinces
    /// image) are not included, since they point at files on this machine.
    pub fn bundle(&self, layers: &[EditLayer], seed: u64) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for &l in layers {
            let v = match l {
                EditLayer::Sketch => serde_json::to_value(&self.sketch),
                EditLayer::Directives => serde_json::to_value(&self.overrides.directives),
                _ => serde_json::to_value(self.strokes(l)),
            };
            m.insert(l.key().into(), v.unwrap());
        }
        serde_json::json!({ "format": BUNDLE_FORMAT, "version": BUNDLE_VERSION, "source_seed": seed, "layers": m })
    }

    /// Add (or with `replace`, swap in) the layers of a bundle. Only the
    /// layers in `only` are taken when it is given. Returns (layer, edits) per
    /// layer taken.
    pub fn apply_bundle(&mut self, bundle: &serde_json::Value, only: Option<&[EditLayer]>, replace: bool) -> Result<Vec<(EditLayer, usize)>, String> {
        if bundle["format"].as_str() != Some(BUNDLE_FORMAT) {
            return Err("not an override bundle (format should be \"fwm-overrides\")".into());
        }
        let version = bundle["version"].as_u64().unwrap_or(0);
        if version == 0 || version > BUNDLE_VERSION as u64 {
            return Err(format!("override bundle version {version} is not supported (this build reads up to {BUNDLE_VERSION})"));
        }
        let layers = bundle["layers"].as_object().ok_or("override bundle has no layers")?;
        // Parse everything first, so a bad layer changes nothing.
        let mut parsed: Vec<(EditLayer, Option<Sketch>, Vec<Stroke>)> = Vec::new();
        let mut directives: Option<Vec<crate::directives::Directive>> = None;
        for (k, v) in layers {
            let l = EditLayer::from_key(k).ok_or_else(|| format!("unknown override layer `{k}`"))?;
            if only.is_some_and(|o| !o.contains(&l)) {
                continue;
            }
            if l == EditLayer::Directives {
                let d: Vec<crate::directives::Directive> = serde_json::from_value(v.clone()).map_err(|e| format!("layer directives: {e}"))?;
                if let Some(bad) = d.iter().find_map(|x| crate::directives::validate(x).err()) {
                    return Err(format!("layer directives: {bad}"));
                }
                directives = Some(d);
            } else if l == EditLayer::Sketch {
                let sk: Sketch = serde_json::from_value(v.clone()).map_err(|e| format!("layer sketch: {e}"))?;
                parsed.push((l, Some(sk), vec![]));
            } else {
                let st: Vec<Stroke> = serde_json::from_value(v.clone()).map_err(|e| format!("layer {k}: {e}"))?;
                if let Some(bad) = st.iter().find(|s| s.tool.layer() != l) {
                    return Err(format!("layer {k}: a {:?} stroke does not belong to it", bad.tool));
                }
                parsed.push((l, None, st));
            }
        }
        let mut out = Vec::new();
        if let Some(d) = directives {
            if replace {
                self.overrides.directives.clear();
            }
            out.push((EditLayer::Directives, d.len()));
            self.overrides.directives.extend(d);
        }
        for (l, sk, st) in parsed {
            if replace {
                self.clear_layer(l);
            }
            match sk {
                Some(sk) => {
                    let n = sk.strokes.len() + sk.pins.len() + sk.arrows.len();
                    if replace {
                        self.sketch = sk;
                    } else {
                        self.sketch.strokes.extend(sk.strokes);
                        self.sketch.pins.extend(sk.pins);
                        self.sketch.arrows.extend(sk.arrows);
                    }
                    out.push((l, n));
                }
                None => {
                    out.push((l, st.len()));
                    self.strokes_mut(l).extend(st);
                }
            }
        }
        Ok(out)
    }

    pub fn clear_layer(&mut self, layer: EditLayer) {
        match layer {
            EditLayer::Sketch => {
                self.sketch.strokes.clear();
                self.sketch.pins.clear();
                self.sketch.arrows.clear();
            }
            EditLayer::Plates => self.overrides.plates.clear(),
            EditLayer::Elevation => self.overrides.elevation.clear(),
            EditLayer::Biomes => self.overrides.biomes.clear(),
            EditLayer::Barriers => self.overrides.barriers.clear(),
            EditLayer::States => self.overrides.states.clear(),
            EditLayer::Provinces => self.overrides.provinces.clear(),
            EditLayer::Sites => self.overrides.sites.clear(),
            EditLayer::Fertility => self.overrides.fertility.clear(),
            EditLayer::Bands => self.overrides.bands.clear(),
            EditLayer::Attraction => self.overrides.attraction.clear(),
            EditLayer::Directives => self.overrides.directives.clear(),
        }
    }
}

fn brush_weight(stroke: &Stroke, d: f64, r: f64) -> f64 {
    let t = (d / r).clamp(0.0, 1.0);
    let inner = stroke.hardness.clamp(0.0, 0.98);
    let w = if t <= inner {
        1.0
    } else {
        let u = (t - inner) / (1.0 - inner);
        let s = 1.0 - u;
        s * s * (3.0 - 2.0 * s)
    };
    w * stroke.strength.clamp(0.0, 1.0)
}

/// Weight of a scatter-brush cell: the smooth falloff (1 at the stroke line, 0 at
/// the rim) plus fractal noise, thresholded. Noise pushes the edge in and out,
/// carving bays and leaving islands up to one radius beyond the rim.
pub fn scatter_weight(stroke: &Stroke, d_over_r: f64, noise: f64) -> f64 {
    let s = stroke.scatter.clamp(0.0, 1.0);
    let v = (1.0 - d_over_r) + s * 1.6 * noise;
    let soft = 0.04 + 0.3 * (1.0 - stroke.hardness.clamp(0.0, 1.0));
    let t = (v / soft).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t) * stroke.strength.clamp(0.0, 1.0)
}

/// Noise octaves for a scatter stroke: as many as fit while the finest detail
/// stays at least 2.5 grid cells wide (finer noise only produces speckle).
pub fn scatter_octaves(grain_km: f64, spacing_km: f64) -> u32 {
    let ratio = grain_km.max(10.0) / (2.5 * spacing_km.max(1e-6));
    (ratio.log2().floor() as i64 + 1).clamp(1, 5) as u32
}

/// Reach of a stroke beyond its radius (scatter noise can extend the edge).
pub fn stroke_reach(stroke: &Stroke) -> f64 {
    1.0 + stroke.scatter.clamp(0.0, 1.0)
}

/// Coverage of a stroke on the grid: per touched cell, the brush weight at its
/// distance to the nearest stroke point. Long gaps between points are filled in
/// along great circles.
pub fn stroke_coverage(grid: &Grid, stroke: &Stroke, radius_km: f64, scratch: &mut Vec<bool>) -> Vec<(u32, f32)> {
    let r = (stroke.radius_km / radius_km).max(grid.spacing * 0.5);
    let reach = r * stroke_reach(stroke);
    let pts: Vec<Vec3> = stroke.points.iter().map(|p| Vec3::from_lat_lon_deg(p[0], p[1])).collect();
    let mut dense: Vec<Vec3> = Vec::with_capacity(pts.len() * 2);
    for (k, &p) in pts.iter().enumerate() {
        if k > 0 {
            let q = pts[k - 1];
            let ang = q.angle_to(p);
            let steps = (ang / (r * 0.3)).ceil() as usize;
            for s in 1..steps {
                let t = s as f64 / steps as f64;
                dense.push((q * (1.0 - t) + p * t).normalized());
            }
        }
        dense.push(p);
    }
    // Nearest distance from each touched cell to the stroke.
    let mut dmin: crate::grid::FastMap<u32, f64> = Default::default();
    let mut hint = None;
    for p in dense {
        let start = grid.for_cells_within(p, reach, hint, scratch, |c, d| {
            let e = dmin.entry(c as u32).or_insert(f64::INFINITY);
            if d < *e {
                *e = d;
            }
        });
        hint = Some(start);
    }
    let noise = (stroke.scatter > 0.0).then(|| crate::noise::Noise::new(stroke.seed, crate::rng::stream::SCATTER_BRUSH));
    let freq = radius_km / stroke.scatter_scale_km.max(10.0);
    let octaves = scatter_octaves(stroke.scatter_scale_km, grid.spacing * radius_km);
    let mut v: Vec<(u32, f32)> = dmin
        .into_iter()
        .filter_map(|(c, d)| {
            let w = match &noise {
                Some(nz) => scatter_weight(stroke, d / r, nz.fbm(grid.pos[c as usize], freq, octaves)),
                None if d <= r => brush_weight(stroke, d, r),
                None => 0.0,
            };
            (w > 0.0).then_some((c, w as f32))
        })
        .collect();
    v.sort_unstable_by_key(|x| x.0);
    v
}
