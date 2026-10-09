//! Step 4 — tectonic relief. Plate boundaries are classified by the relative
//! motion of the two sides; each boundary type contributes a cross-section
//! profile by distance from the boundary. Ocean floor deepens with age away
//! from ridges, hotspots leave volcano chains, ridged noise scaled by tectonic
//! stress adds detail, and the sketch-fidelity slider pulls the coastline back
//! to the sketch. Elevation override strokes are applied last.

use super::plates::{plates_from_meta, PlateInfo};
use super::{Ctx, StepOutput};
use crate::edits::{stroke_coverage, Tool};
use crate::fields::{Field, Fields};
use crate::graph;
use crate::noise::Noise;
use crate::rng::{hash_unit, stream, Rng};
use crate::vec3::Vec3;
use rayon::prelude::*;
use serde::Serialize;

/// Boundary codes stored in the `boundary` field.
pub mod kind {
    pub const NONE: u8 = 0;
    pub const CONT_CONT: u8 = 1;
    pub const OCEAN_CONT: u8 = 2;
    pub const OCEAN_OCEAN: u8 = 3;
    pub const OCEAN_RIFT: u8 = 4;
    pub const CONT_RIFT: u8 = 5;
    pub const TRANSFORM: u8 = 6;
}

/// Profile channels: a boundary type seen from one side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
enum Ch {
    CcOver = 0,
    CcUnder,
    OcTrench,
    OcArc,
    OoTrench,
    OoArc,
    ODiv,
    CDiv,
    Transform,
}
const NCH: usize = 9;

#[derive(Serialize)]
struct Hotspot {
    lat: f64,
    lon: f64,
    plate: u32,
    chain_len: usize,
}

#[inline]
fn gauss(d: f64, c: f64, w: f64) -> f64 {
    let t = (d - c) / w;
    (-t * t).exp()
}
#[inline]
fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let g = ctx.grid;
    let n = g.len();
    let pp = &ctx.params.planet;
    let tp = &ctx.params.tectonics;
    let r_km = pp.radius_km;
    let land = ctx.input.u8("land");
    let hint = ctx.input.f32("mountain_hint");
    let plate = ctx.input.u16("plate");
    let crust = ctx.input.u8("crust");
    let plates: Vec<PlateInfo> = plates_from_meta(ctx.input.meta("plates"));
    let vel: Vec<Vec3> = (0..n).map(|i| plates[plate[i] as usize].velocity(g.pos[i], r_km)).collect();

    // ---- 1. Classify boundary cells.
    ctx.progress(0.03, "Classifying plate boundaries");
    let mut boundary = vec![kind::NONE; n];
    let mut chan_src: Vec<Vec<(u32, f64)>> = vec![Vec::new(); NCH];
    let mut boundary_len = [0.0f64; 7];
    for i in 0..n {
        let pi = plate[i];
        let mut conv = 0.0;
        let mut shear = 0.0;
        let mut toward_i = 0.0;
        let mut toward_j = 0.0;
        let mut cnt = 0.0;
        let mut cont_nb = 0.0;
        let mut other = u16::MAX;
        for &nb in g.neighbors(i) {
            let j = nb as usize;
            if plate[j] == pi {
                continue;
            }
            other = other.min(plate[j]);
            let nvec = (g.pos[j] - g.pos[i]).tangent_at(g.pos[i]).normalized();
            let rel = vel[j] - vel[i];
            let rn = rel.dot(nvec);
            conv += -rn;
            shear += (rel - nvec * rn).len();
            toward_i += vel[i].dot(nvec);
            toward_j += -vel[j].dot(nvec);
            cont_nb += crust[j] as f64;
            cnt += 1.0;
        }
        if cnt == 0.0 {
            continue;
        }
        conv /= cnt;
        shear /= cnt;
        toward_i /= cnt;
        toward_j /= cnt;
        let ci = crust[i] == 1;
        let cj = cont_nb / cnt >= 0.5;
        let speed = (conv * conv + shear * shear).sqrt();
        // i is the "indenter"/subducting side when it drives into the boundary harder.
        let i_drives = toward_i > toward_j || (toward_i == toward_j && pi > other);
        let (code, ch, inten) = if conv > 0.45 * speed && speed > 1.0 {
            let inten = conv / 50.0;
            match (ci, cj) {
                (true, true) => (kind::CONT_CONT, if i_drives { Ch::CcUnder } else { Ch::CcOver }, inten),
                (false, true) => (kind::OCEAN_CONT, Ch::OcTrench, inten),
                (true, false) => (kind::OCEAN_CONT, Ch::OcArc, inten),
                (false, false) => (kind::OCEAN_OCEAN, if i_drives { Ch::OoTrench } else { Ch::OoArc }, inten),
            }
        } else if conv < -0.45 * speed && speed > 1.0 {
            let inten = -conv / 50.0;
            if ci && cj {
                (kind::CONT_RIFT, Ch::CDiv, inten)
            } else {
                (kind::OCEAN_RIFT, Ch::ODiv, inten)
            }
        } else {
            (kind::TRANSFORM, Ch::Transform, shear.max(speed) / 50.0)
        };
        boundary[i] = code;
        boundary_len[code as usize] += g.spacing * r_km / 2.0;
        chan_src[ch as usize].push((i as u32, inten.clamp(0.15, 3.0)));
    }

    // ---- 2. Distance from each boundary channel, staying on the same plate.
    ctx.progress(0.15, "Measuring distance to boundaries");
    let max_km = tp.max_influence_km.max(300.0);
    let chans: Vec<(Vec<f32>, Vec<f32>)> = (0..NCH)
        .into_par_iter()
        .map(|ch| {
            let src = &chan_src[ch];
            // Smooth intensity along the boundary to avoid streaks.
            let mut inten_at = vec![f64::NAN; n];
            for &(c, v) in src {
                inten_at[c as usize] = v;
            }
            let smoothed: Vec<f64> = src
                .iter()
                .map(|&(c, v)| {
                    let mut s = v;
                    let mut w = 1.0;
                    for &nb in g.neighbors(c as usize) {
                        let x = inten_at[nb as usize];
                        if x.is_finite() {
                            s += x;
                            w += 1.0;
                        }
                    }
                    s / w
                })
                .collect();
            let sources: Vec<(u32, f64, u32)> = src.iter().enumerate().map(|(k, &(c, _))| (c, 0.0, k as u32)).collect();
            let limit = if ch == Ch::ODiv as usize { f64::INFINITY } else { max_km };
            let res = graph::multi_source(g, &sources, limit, |a, b, _, edge_len| {
                if plate[a] == plate[b] {
                    Some(edge_len * r_km)
                } else {
                    None
                }
            });
            let dist: Vec<f32> = res.cost.iter().map(|&c| c as f32).collect();
            let inten: Vec<f32> = res.label.iter().map(|&l| if l == u32::MAX { 0.0 } else { smoothed[l as usize] as f32 }).collect();
            (dist, inten)
        })
        .collect();

    // Distances for the continental margin shape.
    ctx.progress(0.35, "Shaping margins");
    let d_sea = graph::distance_km(g, r_km, 3000.0, |i| land[i] == 0, |_| true); // land → sea distance
    let d_land = graph::distance_km(g, r_km, 3000.0, |i| land[i] == 1, |_| true);
    let d_cont = graph::distance_km(g, r_km, 3000.0, |i| crust[i] == 1, |_| true);

    // ---- 3. Hotspot chains.
    ctx.progress(0.45, "Tracing hotspot chains");
    let mut hrng = Rng::new(pp.seed, stream::HOTSPOTS);
    let mut volcano: Vec<(Vec3, f64, f64)> = Vec::new(); // (position, height m, width km)
    let mut hotspots = Vec::new();
    for _ in 0..tp.hotspots {
        let p0 = hrng.unit_vector();
        let c0 = g.nearest(p0, None);
        let pl = &plates[plate[c0] as usize];
        let axis = Vec3::new(pl.pole[0], pl.pole[1], pl.pole[2]);
        let w = pl.omega_deg_myr.to_radians();
        let strength = hrng.range(0.6, 1.2);
        let mut chain = 0;
        for k in 0..40 {
            let age = k as f64 * 2.0;
            let p = p0.rotate(axis, w * age);
            let c = g.nearest(p, Some(c0));
            if plate[c] != plate[c0] {
                break;
            }
            let on_cont = crust[c] == 1;
            let h = if on_cont { 1400.0 } else { 5200.0 } * strength * (-age / 22.0).exp() * hrng.range(0.6, 1.1);
            volcano.push((p, h, if on_cont { 160.0 } else { 55.0 }));
            chain += 1;
        }
        let (la, lo) = p0.lat_lon();
        hotspots.push(Hotspot { lat: la.to_degrees(), lon: lo.to_degrees(), plate: plate[c0] as u32, chain_len: chain });
    }
    // Rasterise volcanoes into a field.
    let mut volc = vec![0.0f32; n];
    let mut scratch = Vec::new();
    for &(p, h, wkm) in &volcano {
        g.for_cells_within(p, 2.5 * wkm / r_km, None, &mut scratch, |c, d| {
            let v = (h * gauss(d * r_km, 0.0, wkm)) as f32;
            volc[c] = volc[c].max(v);
        });
    }

    // ---- 4. Elevation.
    ctx.progress(0.55, "Raising mountains");
    let noise = Noise::new(pp.seed, stream::RELIEF_NOISE);
    let ms = tp.mountain_scale;
    let ds = tp.detail_scale;
    let f_ridge = r_km / 260.0;
    let f_hill = r_km / 700.0;
    let f_fine = r_km / 90.0;
    let fid = tp.sketch_fidelity.clamp(0.0, 1.0);
    let (mut elev, rest): (Vec<f32>, Vec<(f32, f32)>) = (0..n)
        .into_par_iter()
        .map(|i| {
            let p = g.pos[i];
            // Distances are measured from boundary cell centres; the boundary line
            // itself lies about half a cell further out.
            let half = 0.5 * g.spacing * r_km;
            let ch = |c: Ch| -> (f64, f64) {
                let (d, it) = (&chans[c as usize].0, &chans[c as usize].1);
                (d[i] as f64 + half, it[i] as f64)
            };
            // Ocean age from distance to the nearest ridge on this plate.
            let (d_ridge, i_ridge) = ch(Ch::ODiv);
            let age = if d_ridge.is_finite() {
                let half_rate = (i_ridge * 25.0).max(8.0); // km/Myr
                (d_ridge / half_rate).min(200.0)
            } else {
                150.0
            };

            // Base level.
            let mut e = if crust[i] == 1 {
                if land[i] == 1 {
                    let dc = if d_sea[i].is_finite() { d_sea[i] } else { 3000.0 };
                    180.0 + 420.0 * (1.0 - (-dc / 650.0).exp())
                } else {
                    let dl = if d_land[i].is_finite() { d_land[i] } else { 3000.0 };
                    -40.0 - 160.0 * smoothstep(0.0, ctx.params.plates.shelf_width_km.max(20.0), dl)
                }
            } else {
                let abyss = -(2600.0 + 350.0 * age.sqrt()).min(6400.0);
                let dc = if d_cont[i].is_finite() { d_cont[i] } else { 3000.0 };
                -250.0 + (abyss + 250.0) * smoothstep(0.0, 260.0, dc)
            };

            // Boundary profiles. Only major plates carry Tibet-style plateaus.
            let plateau = if plates[plate[i] as usize].major { 1.0 } else { 0.0 };
            let mut uplift = 0.0;
            // Rift flanks do not count as tectonic stress (no ridged detail on them).
            let mut add = |c: Ch, stress: bool, f: &dyn Fn(f64, f64) -> f64| {
                let (d, it) = ch(c);
                if d.is_finite() {
                    let v = f(d, it);
                    e += v;
                    if stress && v > 0.0 {
                        uplift += v;
                    }
                }
            };
            add(Ch::CcOver, true, &|d, it| {
                ms * it.sqrt() * (4300.0 * gauss(d, 50.0, 130.0) + plateau * 3000.0 * (it - 0.8).clamp(0.0, 1.0) * (1.0 - smoothstep(300.0, 700.0, d)))
            });
            add(Ch::CcUnder, true, &|d, it| ms * it.sqrt() * 3400.0 * gauss(d, 35.0, 110.0) - 250.0 * gauss(d, 280.0, 90.0));
            add(Ch::OcTrench, true, &|d, it| -3300.0 * it.sqrt() * gauss(d, 40.0, 55.0) + 250.0 * gauss(d, 170.0, 60.0));
            add(Ch::OcArc, true, &|d, it| ms * it.sqrt() * (3700.0 * gauss(d, 160.0, 70.0) + 800.0 * gauss(d, 80.0, 220.0)));
            add(Ch::OoTrench, true, &|d, it| -3800.0 * it.sqrt() * gauss(d, 40.0, 55.0));
            add(Ch::OoArc, true, &|d, it| ms * it.sqrt() * 4300.0 * gauss(d, 140.0, 55.0));
            add(Ch::ODiv, true, &|d, it| -350.0 * it.min(1.5) * gauss(d, 0.0, 18.0));
            add(Ch::CDiv, false, &|d, it| it.min(1.5) * (-1200.0 * gauss(d, 0.0, 40.0) + 450.0 * gauss(d, 95.0, 45.0)));
            add(Ch::Transform, true, &|d, it| (noise.fbm(p, f_fine, 3)) * 400.0 * it.min(1.5) * gauss(d, 0.0, 50.0));

            // Hotspots and mountain hints.
            e += volc[i] as f64;
            let h = hint[i] as f64;
            let ridged = noise.ridged(p, f_ridge, 6);
            e += tp.hint_height_m * h * (0.45 + 0.9 * ridged);

            // Detail noise scaled by tectonic stress.
            let stress = (uplift / 3500.0 + h + volc[i] as f64 / 4000.0).clamp(0.0, 1.0);
            if crust[i] == 1 {
                e += ds * ((ridged - 0.42) * 2400.0 * stress + noise.fbm(p, f_hill, 6) * 260.0);
            } else {
                e += ds * (noise.fbm(p, f_hill, 5) * 220.0 + (ridged - 0.42) * 900.0 * stress);
            }

            // Sketch fidelity: pull toward the sketched land/sea.
            // Soft limits: Earth-like extremes even where profiles overlap.
            if e > 0.0 {
                e = 9500.0 * (e / 9500.0).tanh();
            } else if e < -6000.0 {
                e = -6000.0 - 5000.0 * ((-e - 6000.0) / 5000.0).tanh();
            }
            let jitter = hash_unit(pp.seed, stream::RELIEF_NOISE, i as u64);
            if land[i] == 1 && e < 30.0 {
                let target = 30.0 + 50.0 * jitter;
                e += fid * (target - e);
            } else if land[i] == 0 && e > -25.0 {
                let target = -25.0 - 60.0 * jitter;
                e += fid * (target - e);
            }
            (e as f32, (stress as f32, age as f32))
        })
        .unzip();
    let (stress, age): (Vec<f32>, Vec<f32>) = rest.into_iter().unzip();

    // ---- 5. Imported heightmap replaces (or blends into) the tectonic relief.
    let mut import_status = serde_json::Value::Null;
    if let Some(imp) = &ctx.edits.imports.elevation {
        ctx.progress(0.85, "Sampling imported heightmap");
        match crate::import::load(imp) {
            Ok(hm) => {
                let b = imp.blend.clamp(0.0, 1.0) as f32;
                let sampled: Vec<Option<f32>> = (0..n).into_par_iter().map(|i| hm.sample(g.lat[i].to_degrees(), g.lon[i].to_degrees())).collect();
                for (e, s) in elev.iter_mut().zip(sampled) {
                    if let Some(v) = s {
                        *e += b * (v - *e);
                    }
                }
                import_status = serde_json::json!({ "path": imp.path, "width": hm.width, "height": hm.height });
            }
            Err(e) => import_status = serde_json::json!({ "error": e }),
        }
    }

    // ---- 5. Elevation overrides (user edits win).
    ctx.progress(0.9, "Applying elevation edits");
    apply_elevation_overrides(ctx, &mut elev);

    let land_now = elev.iter().filter(|&&e| e > 0.0).count();
    let max_e = elev.iter().cloned().fold(f32::MIN, f32::max);
    let min_e = elev.iter().cloned().fold(f32::MAX, f32::min);
    let mut f = Fields::default();
    f.put("elevation", Field::F32(elev));
    f.put("boundary", Field::U8(boundary));
    f.put("stress", Field::F32(stress));
    f.put("ocean_age", Field::F32(age));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "hotspots": hotspots,
            "boundary_km": {
                "continent_continent": boundary_len[1],
                "ocean_continent": boundary_len[2],
                "ocean_ocean": boundary_len[3],
                "ocean_rift": boundary_len[4],
                "continental_rift": boundary_len[5],
                "transform": boundary_len[6],
            },
            "land_cells": land_now,
            "max_elevation_m": max_e,
            "min_elevation_m": min_e,
            "import": import_status,
        }),
    }
}

fn apply_elevation_overrides(ctx: &Ctx, elev: &mut [f32]) {
    let g = ctx.grid;
    let mut scratch = Vec::new();
    for s in &ctx.edits.overrides.elevation {
        let cov = stroke_coverage(g, s, ctx.params.planet.radius_km, &mut scratch);
        match s.tool {
            Tool::Raise => cov.iter().for_each(|&(c, w)| elev[c as usize] += w * s.value as f32),
            Tool::Lower => cov.iter().for_each(|&(c, w)| elev[c as usize] -= w * s.value as f32),
            Tool::Flatten => cov.iter().for_each(|&(c, w)| {
                let e = &mut elev[c as usize];
                *e += w * (s.value as f32 - *e);
            }),
            Tool::Smooth => {
                for _ in 0..3 {
                    let avg: Vec<f32> = cov
                        .iter()
                        .map(|&(c, _)| {
                            let nb = g.neighbors(c as usize);
                            nb.iter().map(|&j| elev[j as usize]).sum::<f32>() / nb.len() as f32
                        })
                        .collect();
                    for (k, &(c, w)) in cov.iter().enumerate() {
                        let e = &mut elev[c as usize];
                        *e += w * (avg[k] - *e);
                    }
                }
            }
            _ => {}
        }
    }
}
