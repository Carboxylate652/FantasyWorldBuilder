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
}

impl Tool {
    pub fn layer(self) -> EditLayer {
        match self {
            Tool::Land | Tool::Sea | Tool::Mountain | Tool::EraseHint => EditLayer::Sketch,
            Tool::PlatePaint => EditLayer::Plates,
            Tool::Raise | Tool::Lower | Tool::Smooth | Tool::Flatten => EditLayer::Elevation,
            Tool::BiomePaint | Tool::BiomeErase => EditLayer::Biomes,
            Tool::BarrierPaint | Tool::BarrierErase => EditLayer::Barriers,
            Tool::StatePaint => EditLayer::States,
            Tool::ProvincePaint => EditLayer::Provinces,
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
}

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
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke { tool: Tool::Land, radius_km: 400.0, strength: 1.0, hardness: 0.5, value: 0.0, points: vec![], scatter: 0.0, scatter_scale_km: 250.0, seed: 0 }
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
        }
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
