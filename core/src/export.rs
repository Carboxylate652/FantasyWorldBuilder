//! Export rasterisation: grid fields are resampled into an equirectangular
//! raster (barycentric for continuous fields, majority corner for classes).
//! Detail noise is added only here, so coastlines look sharper than the grid.
//!
//! Stage 1 layers of the Paradox-style package (neutral profile, CK3/Vic3-like):
//! heightmap.png (8-bit grey), heightmap16.png, terrain.png (indexed),
//! rivers.png (indexed, CK3 river palette), biomes.png (Köppen colours),
//! climate.png (annual mean temperature), precipitation.png, package.json.
//!
//! Stage 2 layers: provinces.png (one unique RGB colour per province, no
//! anti-aliasing), definition.csv (CK3 / Victoria 3 format), provinces.csv,
//! states.csv, regions.csv, continents.csv, adjacencies.csv (straits) and
//! province_adjacency.csv (every border, typed), plus
//! states.png, a reference map of states with province and state borders.

use crate::export_clean as clean;
use crate::fields::Field;
use crate::grid::Grid;
use crate::noise::Noise;
use crate::rng::stream;
use crate::stages::biomes::{KOPPEN, TERRAIN};
use crate::stages::Step;
use crate::vec3::Vec3;
use crate::world::World;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    pub width: u32,
    pub height: u32,
    /// Latitude band to export (Paradox maps usually leave out the poles).
    pub lat_min: f64,
    pub lat_max: f64,
    /// Heightmap value used for sea level.
    pub sea_level_value: u8,
    /// Elevation mapped to 255 in the 8-bit heightmap.
    pub max_elevation_m: f64,
    /// Depth mapped to 0 in the 8-bit heightmap.
    pub max_depth_m: f64,
    pub detail_noise: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            width: 8192,
            height: 4096,
            lat_min: -90.0,
            lat_max: 90.0,
            sea_level_value: 20,
            max_elevation_m: 6500.0,
            max_depth_m: 6000.0,
            detail_noise: true,
        }
    }
}

/// CK3-style river palette indices.
pub mod river_px {
    pub const SOURCE: u8 = 0;
    pub const MERGE: u8 = 1;
    pub const SPLIT: u8 = 2;
    pub const WIDTH0: u8 = 3; // 3..=11, narrow to wide
    pub const LAND: u8 = 254;
    pub const WATER: u8 = 255;
}

fn river_palette() -> Vec<u8> {
    let mut p = vec![0u8; 256 * 3];
    let set = |p: &mut Vec<u8>, i: usize, c: [u8; 3]| p[i * 3..i * 3 + 3].copy_from_slice(&c);
    set(&mut p, 0, [0, 255, 0]);
    set(&mut p, 1, [255, 0, 0]);
    set(&mut p, 2, [255, 252, 0]);
    let widths: [[u8; 3]; 9] = [
        [0, 225, 255], [0, 200, 255], [0, 150, 255], [0, 100, 255], [0, 0, 255],
        [0, 0, 225], [0, 0, 200], [0, 0, 150], [0, 0, 100],
    ];
    for (k, c) in widths.iter().enumerate() {
        set(&mut p, 3 + k, *c);
    }
    set(&mut p, 254, [255, 255, 255]);
    set(&mut p, 255, [255, 0, 128]);
    p
}

pub fn temperature_color(t: f32) -> [u8; 3] {
    ramp(
        t,
        &[
            (-40.0, [80, 0, 120]), (-25.0, [40, 40, 200]), (-10.0, [60, 140, 240]), (0.0, [200, 235, 255]),
            (10.0, [120, 200, 120]), (20.0, [250, 220, 80]), (30.0, [240, 100, 30]), (40.0, [150, 0, 0]),
        ],
    )
}

pub fn precip_color(p: f32) -> [u8; 3] {
    ramp(
        p,
        &[
            (0.0, [150, 90, 40]), (250.0, [220, 190, 110]), (500.0, [230, 230, 160]), (1000.0, [130, 200, 120]),
            (2000.0, [40, 140, 160]), (3000.0, [30, 70, 170]), (4500.0, [60, 20, 120]),
        ],
    )
}

fn ramp(v: f32, stops: &[(f32, [u8; 3])]) -> [u8; 3] {
    if v <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if v <= b {
            let t = (v - a) / (b - a);
            return std::array::from_fn(|k| (ca[k] as f32 + (cb[k] as f32 - ca[k] as f32) * t).round() as u8);
        }
    }
    stops[stops.len() - 1].1
}

#[derive(Serialize, Debug)]
pub struct ExportReport {
    pub dir: String,
    pub files: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub millis: u64,
}

struct Layers<'a> {
    elev: &'a [f32],
    stress: Option<&'a [f32]>,
    terrain: Option<&'a [u8]>,
    koppen: Option<&'a [u8]>,
    t_mean: Option<&'a [f32]>,
    p_ann: Option<&'a [f32]>,
    lake: Option<&'a [u8]>,
    water: Option<&'a [u8]>,
    /// Province id per cell, its surface class (0 land, 1 sea, 2 lake) and colours.
    province: Option<&'a [u32]>,
    prov_class: Option<Vec<u8>>,
    prov_rgb: std::collections::HashMap<u32, [u8; 3]>,
    border_noise: Noise,
}

fn f32_of<'a>(w: &'a World, name: &str) -> Option<&'a [f32]> {
    match w.field(name) {
        Some((Field::F32(v), _, _)) => Some(v),
        _ => None,
    }
}
fn u8_of<'a>(w: &'a World, name: &str) -> Option<&'a [u8]> {
    match w.field(name) {
        Some((Field::U8(v), _, _)) => Some(v),
        _ => None,
    }
}

pub fn export(world: &mut World, dir: &Path, opts: &ExportOptions, progress: &(dyn Fn(f32, &str) + Sync)) -> Result<ExportReport, String> {
    let t0 = std::time::Instant::now();
    let grid = world.grid();
    if !world.is_fresh(Step::Relief) {
        return Err("Run the steps up to Tectonic relief before exporting.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let w = world as &World;
    let fresh = |s: Step| w.is_fresh(s);
    let layers = Layers {
        elev: f32_of(w, "elevation").ok_or("no elevation")?,
        stress: f32_of(w, "stress"),
        terrain: if fresh(Step::Biomes) { u8_of(w, "terrain") } else { None },
        koppen: if fresh(Step::Biomes) { u8_of(w, "koppen") } else { None },
        t_mean: if fresh(Step::Climate) { f32_of(w, "t_mean") } else { None },
        p_ann: if fresh(Step::Climate) { f32_of(w, "p_ann") } else { None },
        lake: if fresh(Step::Hydrology) { u8_of(w, "lake") } else { None },
        water: if fresh(Step::Hydrology) { u8_of(w, "water") } else { None },
        province: if fresh(Step::Provinces) {
            match w.field("province") {
                Some((Field::U32(v), _, _)) => Some(v.as_slice()),
                _ => None,
            }
        } else {
            None
        },
        prov_class: if fresh(Step::Provinces) {
            u8_of(w, "province_kind").map(|k| k.iter().map(|&x| match x { 3 => 1, 2 => 2, _ => 0 }).collect())
        } else {
            None
        },
        prov_rgb: province_colors(w),
        border_noise: Noise::new(w.params.planet.seed, stream::BORDER_NOISE),
    };
    let (wpx, hpx) = (opts.width.clamp(64, 32768) as usize, opts.height.clamp(32, 16384) as usize);
    let noise = Noise::new(w.params.planet.seed, stream::EXPORT_NOISE);
    let r_km = w.params.planet.radius_km;

    let mut files = Vec::new();
    let mk = |name: &str| -> Result<BufWriter<File>, String> {
        File::create(dir.join(name)).map(BufWriter::new).map_err(|e| format!("{name}: {e}"))
    };
    let encoder = |out: BufWriter<File>, color: png::ColorType, depth: png::BitDepth, palette: Option<Vec<u8>>| {
        let mut e = png::Encoder::new(out, wpx as u32, hpx as u32);
        e.set_color(color);
        e.set_depth(depth);
        if let Some(p) = palette {
            e.set_palette(p);
        }
        e.set_compression(png::Compression::Fast);
        e
    };
    let mut height8 = encoder(mk("heightmap.png")?, png::ColorType::Grayscale, png::BitDepth::Eight, None).write_header().and_then(|w| w.into_stream_writer()).map_err(|e| e.to_string())?;
    let mut height16 = encoder(mk("heightmap16.png")?, png::ColorType::Grayscale, png::BitDepth::Sixteen, None).write_header().and_then(|w| w.into_stream_writer()).map_err(|e| e.to_string())?;
    files.extend(["heightmap.png".to_string(), "heightmap16.png".to_string()]);
    let terrain_pal: Vec<u8> = TERRAIN.iter().flat_map(|(_, c)| *c).collect();
    let mut terrain_w = match layers.terrain {
        Some(_) => {
            files.push("terrain.png".into());
            Some(encoder(mk("terrain.png")?, png::ColorType::Indexed, png::BitDepth::Eight, Some(terrain_pal)).write_header().and_then(|w| w.into_stream_writer()).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    let mut biomes_w = match layers.koppen {
        Some(_) => {
            files.push("biomes.png".into());
            Some(encoder(mk("biomes.png")?, png::ColorType::Rgb, png::BitDepth::Eight, None).write_header().and_then(|w| w.into_stream_writer()).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    // Provinces first: rasterise, clean up, write provinces.png and states.png.
    // The clean-up's surface fixes then feed the other layers.
    let lat_span = opts.lat_max - opts.lat_min;
    let mut clean_report = None;
    let raster = match (layers.province, layers.prov_class.as_deref()) {
        (Some(prov), Some(cls)) => {
            progress(0.0, "Rasterising provinces");
            let mut ids = vec![0u32; wpx * hpx];
            let mut pcls = vec![0u8; wpx * hpx];
            ids.par_chunks_mut(wpx).zip(pcls.par_chunks_mut(wpx)).enumerate().for_each(|(y, (ri, rc))| {
                let lat = (opts.lat_max - (y as f64 + 0.5) / hpx as f64 * lat_span).to_radians();
                let mut hint = None;
                for x in 0..wpx {
                    let lon = (-180.0 + (x as f64 + 0.5) / wpx as f64 * 360.0).to_radians();
                    let p = Vec3::from_lat_lon(lat, lon);
                    let (v, tri, wt) = grid.locate(p, hint);
                    hint = Some(v);
                    let (c, id) = province_pixel(&grid, &layers, prov, cls, &noise, opts, r_km, p, tri, wt);
                    ri[x] = id;
                    rc[x] = c;
                }
            });
            progress(0.25, "Cleaning up province pieces");
            let wm = layers.water.ok_or("no water map")?;
            let (land_body, _) = crate::graph::components(&grid, |i| wm[i] != 1);
            let (sea_body, _) = crate::graph::components(&grid, |i| wm[i] == 1);
            let anchors: Vec<clean::Anchor> = (0..grid.len())
                .filter_map(|i| {
                    let (lat, lon) = (grid.lat[i].to_degrees(), grid.lon[i].to_degrees());
                    if lat < opts.lat_min || lat > opts.lat_max {
                        return None;
                    }
                    let x = (((lon + 180.0) / 360.0 * wpx as f64).floor() as usize).min(wpx - 1);
                    let y = (((opts.lat_max - lat) / lat_span * hpx as f64).floor() as usize).min(hpx - 1);
                    let c = match wm[i] {
                        1 => clean::SEA,
                        2 => clean::LAKE,
                        _ => clean::LAND,
                    };
                    let body = if c == clean::SEA { sea_body[i] } else { land_body[i] };
                    Some(clean::Anchor { px: y * wpx + x, cls: c, id: prov[i], body })
                })
                .collect();
            let mut r = clean::ProvRaster { w: wpx, h: hpx, ids, cls: pcls };
            let scale = clean::row_scale(wpx, hpx, opts.lat_max, opts.lat_min, grid.len());
            let prov_info = province_info(w);
            let max_id = prov_info.keys().chain(r.ids.iter()).max().copied().unwrap_or(0) as usize;
            let mut kind_of = vec![clean::UNKNOWN; max_id + 1];
            for (&id, &(k, _)) in &prov_info {
                kind_of[id as usize] = match k {
                    3 => clean::SEA,
                    2 => clean::LAKE,
                    _ => clean::LAND,
                };
            }
            clean_report = Some(clean::clean(&mut r, &anchors, &scale, &kind_of));
            progress(0.3, "Writing provinces.png");
            let mut pw = encoder(mk("provinces.png")?, png::ColorType::Rgb, png::BitDepth::Eight, None).write_header().and_then(|w| w.into_stream_writer()).map_err(|e| e.to_string())?;
            let mut sw = encoder(mk("states.png")?, png::ColorType::Rgb, png::BitDepth::Eight, None).write_header().and_then(|w| w.into_stream_writer()).map_err(|e| e.to_string())?;
            let mut line = vec![0u8; wpx * 3];
            for y in 0..hpx {
                let row = &r.ids[y * wpx..(y + 1) * wpx];
                for (x, id) in row.iter().enumerate() {
                    line[x * 3..x * 3 + 3].copy_from_slice(&layers.prov_rgb.get(id).copied().unwrap_or([0, 0, 0]));
                }
                pw.write_all(&line).map_err(|e| e.to_string())?;
                let prev = if y > 0 { &r.ids[(y - 1) * wpx..y * wpx] } else { &[][..] };
                sw.write_all(&states_row(row, prev, &prov_info)).map_err(|e| e.to_string())?;
            }
            pw.finish().map_err(|e| e.to_string())?;
            sw.finish().map_err(|e| e.to_string())?;
            files.extend(["provinces.png".to_string(), "states.png".to_string()]);
            Some(r)
        }
        _ => None,
    };
    let mut climate_w = match layers.t_mean {
        Some(_) => {
            files.extend(["climate.png".to_string(), "precipitation.png".to_string()]);
            Some((
                encoder(mk("climate.png")?, png::ColorType::Rgb, png::BitDepth::Eight, None).write_header().and_then(|w| w.into_stream_writer()).map_err(|e| e.to_string())?,
                encoder(mk("precipitation.png")?, png::ColorType::Rgb, png::BitDepth::Eight, None).write_header().and_then(|w| w.into_stream_writer()).map_err(|e| e.to_string())?,
            ))
        }
        None => None,
    };

    let mut water = vec![false; wpx * hpx];
    let band = 128usize;
    let sea = opts.sea_level_value as f64;
    for y0 in (0..hpx).step_by(band) {
        progress(if raster.is_some() { 0.35 + 0.5 * y0 as f32 / hpx as f32 } else { 0.85 * y0 as f32 / hpx as f32 }, "Rasterising");
        let y1 = (y0 + band).min(hpx);
        let rows: Vec<RowOut> = (y0..y1)
            .into_par_iter()
            .map(|y| {
                let lat = (opts.lat_max - (y as f64 + 0.5) / hpx as f64 * lat_span).to_radians();
                let mut row = RowOut::new(wpx);
                let mut hint = None;
                for x in 0..wpx {
                    let lon = (-180.0 + (x as f64 + 0.5) / wpx as f64 * 360.0).to_radians();
                    let p = Vec3::from_lat_lon(lat, lon);
                    let (v, tri, wt) = grid.locate(p, hint);
                    hint = Some(v);
                    let surf = raster.as_ref().map(|r| r.cls[y * wpx + x]);
                    sample(&grid, &layers, &noise, opts, r_km, p, tri, wt, sea, &mut row, x, surf);
                }
                row
            })
            .collect();
        for (k, row) in rows.iter().enumerate() {
            let y = y0 + k;
            water[y * wpx..(y + 1) * wpx].copy_from_slice(&row.water);
            height8.write_all(&row.h8).map_err(|e| e.to_string())?;
            height16.write_all(&row.h16).map_err(|e| e.to_string())?;
            if let Some(tw) = terrain_w.as_mut() {
                tw.write_all(&row.terrain).map_err(|e| e.to_string())?;
            }
            if let Some(bw) = biomes_w.as_mut() {
                bw.write_all(&row.biome).map_err(|e| e.to_string())?;
            }
            if let Some((cw, pw)) = climate_w.as_mut() {
                cw.write_all(&row.temp).map_err(|e| e.to_string())?;
                pw.write_all(&row.precip).map_err(|e| e.to_string())?;
            }
        }
    }
    height8.finish().map_err(|e| e.to_string())?;
    height16.finish().map_err(|e| e.to_string())?;
    if let Some(tw) = terrain_w {
        tw.finish().map_err(|e| e.to_string())?;
    }
    if let Some(bw) = biomes_w {
        bw.finish().map_err(|e| e.to_string())?;
    }
    if let Some((cw, pw)) = climate_w {
        cw.finish().map_err(|e| e.to_string())?;
        pw.finish().map_err(|e| e.to_string())?;
    }

    // Rivers.
    if w.is_fresh(Step::Hydrology) {
        progress(0.9, "Drawing rivers");
        let river = u8_of(w, "river_rank").ok_or("no rivers")?;
        let recv = match w.field("receiver") {
            Some((Field::I32(v), _, _)) => v.as_slice(),
            _ => return Err("no receivers".into()),
        };
        let discharge = f32_of(w, "discharge").ok_or("no discharge")?;
        let img = draw_rivers(&grid, river, recv, discharge, &water, wpx, hpx, opts);
        let mut e = encoder(mk("rivers.png")?, png::ColorType::Indexed, png::BitDepth::Eight, Some(river_palette()))
            .write_header()
            .map_err(|e| e.to_string())?;
        e.write_image_data(&img).map_err(|e| e.to_string())?;
        e.finish().map_err(|e| e.to_string())?;
        files.push("rivers.png".into());

        // Vector rivers with per-vertex width and discharge, for spline-based
        // river renderers (e.g. Victoria 3 style) and GIS tools.
        let width = f32_of(w, "river_width").ok_or("no river widths")?;
        let mut feats = Vec::new();
        for (q, chain) in river_chains(grid.len(), river, recv, discharge) {
            let mut coords: Vec<[f64; 2]> = Vec::with_capacity(chain.len());
            let mut widths = Vec::with_capacity(chain.len());
            let mut flows = Vec::with_capacity(chain.len());
            for &c in &chain {
                let lon = grid.lon[c].to_degrees();
                // Keep the line continuous across the antimeridian (lon may leave ±180).
                let lon = match coords.last() {
                    Some(&[pl, _]) if lon - pl > 180.0 => lon - 360.0,
                    Some(&[pl, _]) if lon - pl < -180.0 => lon + 360.0,
                    _ => lon,
                };
                coords.push([(lon * 1e4).round() / 1e4, (grid.lat[c].to_degrees() * 1e4).round() / 1e4]);
                // The last vertex is the confluence (or the sea): keep this river's own values there.
                let idx = flows.len();
                let k = if idx + 1 == chain.len() && idx > 0 { chain[idx - 1] } else { c };
                widths.push((width[k] as f64).round());
                flows.push((discharge[k] as f64).round());
            }
            feats.push(serde_json::json!({
                "type": "Feature",
                "geometry": { "type": "LineString", "coordinates": coords },
                "properties": { "mouth_discharge_m3s": q.round(), "width_m": widths, "discharge_m3s": flows },
            }));
        }
        let gj = serde_json::json!({ "type": "FeatureCollection", "features": feats });
        std::fs::write(dir.join("rivers.geojson"), serde_json::to_vec(&gj).unwrap()).map_err(|e| e.to_string())?;
        files.push("rivers.geojson".into());
    }

    // Stage 2 tables.
    if layers.province.is_some() {
        progress(0.95, "Writing province tables");
        let table = &w.meta(Step::Provinces).ok_or("no provinces")?["table"];
        files.extend(write_tables(dir, table, wpx, hpx, opts)?);
    }

    progress(0.97, "Writing package description");
    let pkg = serde_json::json!({
        "generator": "Fantasy World Maker",
        "profile": "neutral (CK3 / Victoria 3 conventions)",
        "projection": "equirectangular",
        "width": wpx, "height": hpx,
        "lat_min": opts.lat_min, "lat_max": opts.lat_max,
        "heightmap": {
            "file": "heightmap.png", "sea_level_value": opts.sea_level_value,
            "max_elevation_m": opts.max_elevation_m, "max_depth_m": opts.max_depth_m,
            "heightmap16": "heightmap16.png: metres = value / 65535 * 20000 - 11000",
        },
        "terrain_palette": TERRAIN.iter().enumerate().map(|(i, (n, c))| serde_json::json!({"index": i, "name": n, "rgb": c})).collect::<Vec<_>>(),
        "koppen_colors": KOPPEN.iter().map(|(n, c)| serde_json::json!({"class": n, "rgb": c})).collect::<Vec<_>>(),
        "rivers": {"0": "source", "1": "merge (tributary end)", "2": "split", "3-11": "width class: each step is ×√2 in width, from the threshold river (3) up", "254": "land", "255": "water"},
        "rivers_geojson": "rivers.geojson: one LineString per river from source to confluence or mouth, with width_m and discharge_m3s per vertex (width = a·Q^b)",
        "river_width": { "coeff": w.params.hydrology.width_coeff, "exponent": w.params.hydrology.width_exponent },
        "provinces": {
            "provinces.png": "one unique RGB colour per province, nearest-cell sampling (no anti-aliasing); borders are noise-warped by up to a third of a grid cell; islets and ponds made only by detail noise are removed, stray pieces merged and X-crossings broken up (see province_cleanup)",
            "definition.csv": "id;r;g;b;name;x; (CK3 / Victoria 3), first row 0;0;0;0;x;x;",
            "provinces.csv": "id;name;kind (land, wasteland, lake, sea);band (sea: coastal, shelf, open);state;region;continent;terrain;area_km2;habitability;coastal;lat;lon;neighbors",
            "states.csv": "id;key;name;region;continent;capital_province;area_km2;habitability;provinces;province_colors (Victoria 3 style xRRGGBB)",
            "regions.csv": "id;key;name;continent;states",
            "continents.csv": "id;name;area_km2;states;regions",
            "adjacencies.csv": "CK3 layout: From;To;Type;Through;start_x;start_y;stop_x;stop_y;Comment — sea crossings between provinces on different landmasses",
            "province_adjacency.csv": "from;to;type;border_km;barrier;crossing_km — every border between two provinces; type: land, river (along a border river), impassable (wasteland), coast (land–sea), lake, sea, strait (crossing_km = width); barrier = mean crossing cost of the border (0 = open)",
        },
        "province_cleanup": clean_report,
        "params": w.params,
        "files": files,
    });
    std::fs::write(dir.join("package.json"), serde_json::to_string_pretty(&pkg).unwrap()).map_err(|e| e.to_string())?;
    files.push("package.json".into());
    progress(1.0, "Done");
    Ok(ExportReport { dir: dir.display().to_string(), files, width: wpx as u32, height: hpx as u32, millis: t0.elapsed().as_millis() as u64 })
}

struct RowOut {
    water: Vec<bool>,
    h8: Vec<u8>,
    h16: Vec<u8>,
    terrain: Vec<u8>,
    biome: Vec<u8>,
    temp: Vec<u8>,
    precip: Vec<u8>,
}
impl RowOut {
    fn new(w: usize) -> RowOut {
        RowOut {
            water: vec![false; w],
            h8: vec![0; w],
            h16: vec![0; w * 2],
            terrain: vec![0; w],
            biome: vec![0; w * 3],
            temp: vec![0; w * 3],
            precip: vec![0; w * 3],
        }
    }
}

/// Elevation with detail noise, land/sea state, lake flag and heaviest corner of a pixel.
fn surface(l: &Layers, noise: &Noise, o: &ExportOptions, r_km: f64, p: Vec3, c: [usize; 3], wt: [f64; 3]) -> (f64, bool, Option<bool>, usize) {
    let interp = |v: &[f32]| -> f64 { v[c[0]] as f64 * wt[0] + v[c[1]] as f64 * wt[1] + v[c[2]] as f64 * wt[2] };
    let mut e = interp(l.elev);
    if o.detail_noise {
        let stress = l.stress.map(|s| interp(s)).unwrap_or(0.0);
        let amp = 35.0 + 650.0 * stress;
        e += noise.fbm(p, r_km / 25.0, 5) * amp;
    }
    let kmax = (0..3).max_by(|&a, &b| wt[a].partial_cmp(&wt[b]).unwrap()).unwrap();
    let is_lake = l.lake.map(|lk| lk[c[kmax]] == 1 || lk[c[kmax]] == 2);
    // Dry basins below sea level (all corners land in the water map) stay land.
    let dry_basin = l.water.is_some_and(|wm| c.iter().all(|&k| wm[k] == 0));
    (e, e > 0.0 || dry_basin, is_lake, kmax)
}

/// One pixel of every layer. `surf` is the cleaned province raster's surface
/// class for this pixel: it can turn a noise islet into sea or a pond into land.
#[allow(clippy::too_many_arguments)]
fn sample(g: &Grid, l: &Layers, noise: &Noise, o: &ExportOptions, r_km: f64, p: Vec3, tri: [u32; 3], wt: [f64; 3], sea: f64, row: &mut RowOut, x: usize, surf: Option<u8>) {
    let c = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
    let interp = |v: &[f32]| -> f64 { v[c[0]] as f64 * wt[0] + v[c[1]] as f64 * wt[1] + v[c[2]] as f64 * wt[2] };
    let (mut e, mut land, mut is_lake, _) = surface(l, noise, o, r_km, p, c, wt);
    match surf {
        Some(clean::LAND) => {
            if !land {
                land = true;
                e = e.max(1.0);
            }
            is_lake = is_lake.map(|_| false);
        }
        Some(clean::SEA) => {
            if land {
                land = false;
                e = e.min(-1.0);
            }
            is_lake = is_lake.map(|_| false);
        }
        Some(_) => is_lake = Some(true),
        None => {}
    }
    row.water[x] = !land || is_lake == Some(true);
    let h8 = if land {
        sea + 1.0 + (e / o.max_elevation_m).clamp(0.0, 1.0) * (254.0 - sea)
    } else {
        sea * (1.0 + e / o.max_depth_m).clamp(0.0, 1.0)
    };
    row.h8[x] = h8.round().clamp(0.0, 255.0) as u8;
    let h16 = ((e + 11000.0) / 20000.0 * 65535.0).round().clamp(0.0, 65535.0) as u16;
    row.h16[x * 2..x * 2 + 2].copy_from_slice(&h16.to_be_bytes());

    // Classes: the heaviest corner that agrees with this pixel's land/sea state.
    let pick = |cls: &[u8]| -> u8 {
        let mut best: Option<usize> = None;
        for k in 0..3 {
            let corner_land = match l.water {
                Some(wm) => wm[c[k]] != 1,
                None => l.elev[c[k]] > 0.0,
            };
            if corner_land == land && best.map_or(true, |b| wt[k] > wt[b]) {
                best = Some(k);
            }
        }
        match best {
            Some(k) => cls[c[k]],
            None if land => {
                // Land pixel between sea cells: borrow from the nearest land neighbour.
                let k = (0..3).max_by(|&a, &b| wt[a].partial_cmp(&wt[b]).unwrap()).unwrap();
                g.neighbors(c[k]).iter().map(|&j| j as usize).find(|&j| l.elev[j] > 0.0).map(|j| cls[j]).unwrap_or(cls[c[k]])
            }
            None => 0,
        }
    };
    if let Some(t) = l.terrain {
        row.terrain[x] = if is_lake == Some(true) { 1 } else if land { pick(t).max(2) } else { 0 };
    }
    if let Some(k) = l.koppen {
        let cls = if is_lake == Some(true) { 31 } else if land { pick(k).max(1) } else { 0 };
        row.biome[x * 3..x * 3 + 3].copy_from_slice(&KOPPEN[cls as usize].1);
    }
    if let (Some(t), Some(pa)) = (l.t_mean, l.p_ann) {
        row.temp[x * 3..x * 3 + 3].copy_from_slice(&temperature_color(interp(t) as f32));
        row.precip[x * 3..x * 3 + 3].copy_from_slice(&precip_color(interp(pa) as f32));
    }
}

/// Surface class and province of a pixel, before clean-up.
#[allow(clippy::too_many_arguments)]
fn province_pixel(g: &Grid, l: &Layers, prov: &[u32], cls: &[u8], noise: &Noise, o: &ExportOptions, r_km: f64, p: Vec3, tri: [u32; 3], wt: [f64; 3]) -> (u8, u32) {
    let c = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
    let (_, land, is_lake, kmax) = surface(l, noise, o, r_km, p, c, wt);
    // Surface class of this pixel, consistent with the heightmap coastline.
    let want: u8 = if is_lake == Some(true) { clean::LAKE } else if land { clean::LAND } else { clean::SEA };
    let id = province_at(g, l, prov, cls, p, c, wt, want).or_else(|| if want == clean::LAKE { province_at(g, l, prov, cls, p, c, wt, clean::LAND) } else { None });
    (want, id.unwrap_or(prov[c[kmax]]))
}

/// Province for a pixel of surface class `want`: the heaviest matching corner
/// of the triangle under a noise-warped position (so borders do not follow
/// the hexagonal cell outlines), else of the unwarped triangle, else a
/// matching neighbour of the nearest corner.
#[allow(clippy::too_many_arguments)]
fn province_at(g: &Grid, l: &Layers, prov: &[u32], cls: &[u8], p: Vec3, c: [usize; 3], wt: [f64; 3], want: u8) -> Option<u32> {
    let amp = g.spacing * 0.35;
    let f = 1.0 / (g.spacing * 3.0);
    let warp = Vec3::new(
        l.border_noise.fbm(p, f, 2),
        l.border_noise.fbm(p + Vec3::new(5.2, 1.3, 2.8), f, 2),
        l.border_noise.fbm(p + Vec3::new(1.7, 9.2, 3.4), f, 2),
    );
    let q = (p + warp * amp).normalized();
    let (_, tq, wq) = g.locate(q, Some(c[0]));
    let best = |tri: [usize; 3], w: [f64; 3]| -> Option<u32> {
        (0..3).filter(|&k| cls[tri[k]] == want).max_by(|&a, &b| w[a].partial_cmp(&w[b]).unwrap()).map(|k| prov[tri[k]])
    };
    best([tq[0] as usize, tq[1] as usize, tq[2] as usize], wq).or_else(|| best(c, wt)).or_else(|| {
        let k = (0..3).max_by(|&a, &b| wt[a].partial_cmp(&wt[b]).unwrap()).unwrap();
        g.neighbors(c[k]).iter().map(|&j| j as usize).find(|&j| cls[j] == want).map(|j| prov[j])
    })
}

/// (kind, state) per province id, for states.png.
fn province_info(w: &World) -> std::collections::HashMap<u32, (u8, u16)> {
    let mut m = std::collections::HashMap::new();
    if let Some(meta) = w.meta(Step::Provinces) {
        for p in meta["table"]["provinces"].as_array().into_iter().flatten() {
            let k = match p["kind"].as_str() {
                Some("wasteland") => 1,
                Some("lake") => 2,
                Some("sea") => 3,
                _ => 0,
            };
            m.insert(p["id"].as_u64().unwrap_or(0) as u32, (k, p["state"].as_u64().unwrap_or(0) as u16));
        }
    }
    m
}

/// Distinct colour per state (golden-angle hues, alternating lightness).
pub fn state_color(id: u16) -> [u8; 3] {
    let h = (id as f64 * 137.508) % 360.0;
    let l = if id % 3 == 0 { 0.48 } else if id % 3 == 1 { 0.6 } else { 0.7 };
    let s = 0.5;
    let c = (1.0 - (2.0 * l - 1.0f64).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8]
}

/// One row of states.png: states in colour, wasteland grey, water blue;
/// province borders darkened, state borders black (streamed: compares with
/// the pixel to the left and the row above).
fn states_row(ids: &[u32], prev: &[u32], info: &std::collections::HashMap<u32, (u8, u16)>) -> Vec<u8> {
    let w = ids.len();
    let get = |id: u32| info.get(&id).copied().unwrap_or((3, 0));
    let mut out = vec![0u8; w * 3];
    for x in 0..w {
        let id = ids[x];
        let (k, s) = get(id);
        let mut c = match k {
            0 => state_color(s),
            1 => {
                let b = state_color(s);
                [(b[0] as u16 / 3 + 110) as u8, (b[1] as u16 / 3 + 110) as u8, (b[2] as u16 / 3 + 110) as u8]
            }
            2 => [110, 160, 215],
            _ => [40, 70, 120],
        };
        let mut border = 0u8; // 1 province, 2 state
        for nb in [if x > 0 { Some(ids[x - 1]) } else { None }, prev.get(x).copied()].into_iter().flatten() {
            if nb != id {
                let (nk, ns) = get(nb);
                let political = |k: u8| k <= 1;
                let state_edge = (political(k) || political(nk)) && (ns != s || political(k) != political(nk));
                border = border.max(if state_edge { 2 } else { 1 });
            }
        }
        if border == 2 {
            c = [20, 20, 20];
        } else if border == 1 {
            let f = if k == 3 { 1.25 } else { 0.72 };
            c = c.map(|v| ((v as f64) * f).min(255.0) as u8);
        }
        out[x * 3..x * 3 + 3].copy_from_slice(&c);
    }
    out
}

fn province_colors(w: &World) -> std::collections::HashMap<u32, [u8; 3]> {
    let mut m = std::collections::HashMap::new();
    if let Some(meta) = w.meta(Step::Provinces) {
        for p in meta["table"]["provinces"].as_array().into_iter().flatten() {
            let id = p["id"].as_u64().unwrap_or(0) as u32;
            let c = &p["color"];
            let ch = |k: usize| c[k].as_u64().unwrap_or(0) as u8;
            m.insert(id, [ch(0), ch(1), ch(2)]);
        }
    }
    m
}

fn csv_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.replace([';', '\n', '\r'], ","),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Array(a) => a.iter().map(csv_text).collect::<Vec<_>>().join(" "),
        other => other.to_string(),
    }
}

/// definition.csv, provinces.csv, states.csv, regions.csv, continents.csv,
/// adjacencies.csv, province_adjacency.csv.
fn write_tables(dir: &Path, t: &serde_json::Value, wpx: usize, hpx: usize, o: &ExportOptions) -> Result<Vec<String>, String> {
    let empty = vec![];
    let arr = |k: &str| t[k].as_array().unwrap_or(&empty);
    let write = |name: &str, text: String| std::fs::write(dir.join(name), text).map_err(|e| format!("{name}: {e}"));
    let mut colors = std::collections::HashMap::new();

    let mut def = String::from("0;0;0;0;x;x;\n");
    let mut prov = String::from("id;name;kind;band;state;region;continent;terrain;area_km2;habitability;coastal;lat;lon;neighbors\n");
    for p in arr("provinces") {
        let c = &p["color"];
        let rgb = [c[0].as_u64().unwrap_or(0), c[1].as_u64().unwrap_or(0), c[2].as_u64().unwrap_or(0)];
        colors.insert(p["id"].as_u64().unwrap_or(0), format!("x{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
        def += &format!("{};{};{};{};{};x;\n", p["id"], rgb[0], rgb[1], rgb[2], csv_text(&p["name"]));
        prov += &format!(
            "{};{};{};{};{};{};{};{};{};{};{};{};{};{}\n",
            p["id"], csv_text(&p["name"]), csv_text(&p["kind"]), csv_text(&p["band"]), p["state"], p["region"], p["continent"],
            csv_text(&p["terrain"]), p["area_km2"], p["habitability"], p["coastal"], p["center"][0], p["center"][1], csv_text(&p["neighbors"]),
        );
    }
    write("definition.csv", def)?;
    write("provinces.csv", prov)?;

    let mut st = String::from("id;key;name;region;continent;capital_province;area_km2;habitability;provinces;province_colors\n");
    for s in arr("states") {
        let ids: Vec<u64> = s["provinces"].as_array().map(|a| a.iter().filter_map(|v| v.as_u64()).collect()).unwrap_or_default();
        let cols: Vec<String> = ids.iter().filter_map(|i| colors.get(i).cloned()).collect();
        st += &format!(
            "{};{};{};{};{};{};{};{};{};{}\n",
            s["id"], csv_text(&s["key"]), csv_text(&s["name"]), s["region"], s["continent"], s["capital_province"], s["area_km2"], s["habitability"],
            ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(" "), cols.join(" "),
        );
    }
    write("states.csv", st)?;

    let mut rg = String::from("id;key;name;continent;states\n");
    for r in arr("regions") {
        rg += &format!("{};{};{};{};{}\n", r["id"], csv_text(&r["key"]), csv_text(&r["name"]), r["continent"], csv_text(&r["states"]));
    }
    write("regions.csv", rg)?;
    let mut ct = String::from("id;name;area_km2;states;regions\n");
    for c in arr("continents") {
        ct += &format!("{};{};{};{};{}\n", c["id"], csv_text(&c["name"]), c["area_km2"], c["states"], c["regions"]);
    }
    write("continents.csv", ct)?;

    let to_px = |ll: &serde_json::Value| -> Option<(i64, i64)> {
        let (lat, lon) = (ll[0].as_f64()?, ll[1].as_f64()?);
        if lat < o.lat_min || lat > o.lat_max {
            return None;
        }
        let x = ((lon + 180.0) / 360.0 * wpx as f64).floor() as i64;
        let y = ((o.lat_max - lat) / (o.lat_max - o.lat_min) * hpx as f64).floor() as i64;
        Some((x.clamp(0, wpx as i64 - 1), y.clamp(0, hpx as i64 - 1)))
    };
    let mut adj = String::from("From;To;Type;Through;start_x;start_y;stop_x;stop_y;Comment\n");
    for a in arr("adjacencies") {
        let (Some(s), Some(e)) = (to_px(&a["from_ll"]), to_px(&a["to_ll"])) else { continue };
        adj += &format!("{};{};sea;{};{};{};{};{};{} km strait\n", a["from"], a["to"], a["through"], s.0, s.1, e.0, e.1, a["km"]);
    }
    write("adjacencies.csv", adj)?;
    let mut pa = String::from("from;to;type;border_km;barrier;crossing_km\n");
    for a in arr("adjacency") {
        pa += &format!("{};{};{};{};{};{}\n", a["from"], a["to"], csv_text(&a["type"]), a["border_km"], a["barrier"], a.get("crossing_km").map_or(String::new(), |v| v.to_string()));
    }
    write("province_adjacency.csv", pa)?;
    Ok(["definition.csv", "provinces.csv", "states.csv", "regions.csv", "continents.csv", "adjacencies.csv", "province_adjacency.csv"].map(String::from).to_vec())
}

/// River chains, biggest first: each runs from a source down its main stem to
/// where it joins a bigger river (inclusive) or reaches water.
fn river_chains(n: usize, rank: &[u8], recv: &[i32], discharge: &[f32]) -> Vec<(f32, Vec<usize>)> {
    // Main upstream of each river cell = the river donor with the most discharge.
    let mut main_up = vec![-1i32; n];
    for i in 0..n {
        if rank[i] == 0 {
            continue;
        }
        let r = recv[i];
        if r >= 0 && rank[r as usize] > 0 {
            let r = r as usize;
            if main_up[r] < 0 || discharge[i] > discharge[main_up[r] as usize] {
                main_up[r] = i as i32;
            }
        }
    }
    // Chains: from each source down to where it joins a bigger river or water.
    let mut chains: Vec<(f32, Vec<usize>)> = Vec::new();
    for s in 0..n {
        if rank[s] == 0 || main_up[s] >= 0 {
            continue;
        }
        let mut chain = vec![s];
        let mut c = s;
        loop {
            let r = recv[c];
            if r < 0 {
                break;
            }
            let r = r as usize;
            chain.push(r);
            if rank[r] == 0 || main_up[r] != c as i32 {
                break;
            }
            c = r;
        }
        // The last cell is the confluence or the sea: rank by this river's own flow.
        let q = discharge[chain[chain.len().saturating_sub(2)]];
        chains.push((q, chain));
    }
    chains.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1[0].cmp(&b.1[0])));

    chains
}

#[allow(clippy::too_many_arguments)]
fn draw_rivers(g: &Grid, rank: &[u8], recv: &[i32], discharge: &[f32], water: &[bool], w: usize, h: usize, o: &ExportOptions) -> Vec<u8> {
    use river_px::*;
    let mut img: Vec<u8> = water.iter().map(|&wt| if wt { WATER } else { LAND }).collect();
    let n = g.len();
    let to_px = |i: usize| -> Option<(i64, i64)> {
        let lat = g.lat[i].to_degrees();
        let lon = g.lon[i].to_degrees();
        if lat < o.lat_min || lat > o.lat_max {
            return None;
        }
        let x = ((lon + 180.0) / 360.0 * w as f64).floor() as i64;
        let y = ((o.lat_max - lat) / (o.lat_max - o.lat_min) * h as f64).floor() as i64;
        Some((x.clamp(0, w as i64 - 1), y.clamp(0, h as i64 - 1)))
    };
    let chains = river_chains(n, rank, recv, discharge);

    for (_, chain) in &chains {
        let mut first = true;
        let mut last: Option<(i64, i64)> = None;
        'chain: for win in chain.windows(2) {
            let (a, b) = (win[0], win[1]);
            let (Some(pa), Some(pb)) = (to_px(a), to_px(b)) else { break };
            if (pa.0 - pb.0).abs() > w as i64 / 2 {
                break;
            }
            let color = WIDTH0 + rank[a].saturating_sub(1).min(8);
            // 4-connected line: step in x or y, whichever the ideal line crosses first.
            let (mut x, mut y) = pa;
            let (dx, dy) = ((pb.0 - pa.0).abs(), (pb.1 - pa.1).abs());
            let (sx, sy) = (if pa.0 < pb.0 { 1 } else { -1 }, if pa.1 < pb.1 { 1 } else { -1 });
            let (mut ix, mut iy) = (0i64, 0i64);
            loop {
                if Some((x, y)) != last {
                    let idx = y as usize * w + x as usize;
                    let px = img[idx];
                    if px == WATER {
                        break 'chain;
                    }
                    if px != LAND {
                        // Joined an existing river: mark the tributary's last pixel as a merge.
                        if let Some((lx, ly)) = last {
                            let li = ly as usize * w + lx as usize;
                            if img[li] >= WIDTH0 {
                                img[li] = MERGE;
                            }
                        }
                        break 'chain;
                    }
                    img[idx] = if first { SOURCE } else { color };
                    first = false;
                    last = Some((x, y));
                }
                if ix >= dx && iy >= dy {
                    break;
                }
                if iy >= dy || (ix < dx && (1 + 2 * ix) * dy < (1 + 2 * iy) * dx) {
                    ix += 1;
                    x += sx;
                } else {
                    iy += 1;
                    y += sy;
                }
            }
        }
    }
    let _ = SPLIT;
    img
}
