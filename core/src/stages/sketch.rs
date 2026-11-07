//! Step 2 — continent sketch. Turns sketch strokes (or a generated base) into a
//! soft land field, a land/sea mask with natural coastline noise, and a
//! "mountains here" hint field. There are three bases: continents generated
//! from the seed (strokes paint over them), an empty ocean (strokes paint land
//! exactly), and continents grown from the drawing (strokes are a rough layout
//! that the generator follows, so a hand-drawn map gets natural coasts in one
//! step instead of a search for a seed that looks like it).

use super::{Ctx, StepOutput};
use crate::edits::{stroke_coverage, SketchBase, Tool};
use crate::fields::{Field, Fields};
use crate::graph;
use crate::noise::Noise;
use crate::rng::{stream, Rng};
use crate::vec3::Vec3;
use rayon::prelude::*;

pub fn run(ctx: &Ctx) -> StepOutput {
    let g = ctx.grid;
    let n = g.len();
    let pp = &ctx.params.planet;
    let sp = &ctx.params.sketch;
    let sketch = &ctx.edits.sketch;
    let coast = Noise::new(pp.seed, stream::COAST_NOISE);
    let coast_freq = pp.radius_km / sp.coast_scale_km.max(50.0);
    let rough = sp.coast_roughness.clamp(0.0, 2.0);

    // Base land field.
    let base = sketch.base();
    let mut land_f: Vec<f32> = if base == SketchBase::Generated {
        ctx.progress(0.05, "Generating continents");
        auto_base(ctx, &coast, coast_freq, rough)
    } else {
        vec![-1.0; n]
    };

    // Apply strokes in order.
    ctx.progress(0.5, "Applying sketch strokes");
    let mut hint = vec![0.0f32; n];
    let mut scratch = Vec::new();
    for s in &sketch.strokes {
        let cov = stroke_coverage(g, s, pp.radius_km, &mut scratch);
        for (c, w) in cov {
            let c = c as usize;
            match s.tool {
                Tool::Land => land_f[c] += (1.0 - land_f[c]) * w,
                Tool::Sea => land_f[c] += (-1.0 - land_f[c]) * w,
                Tool::Mountain => hint[c] += (1.0 - hint[c]) * w,
                Tool::EraseHint => hint[c] -= hint[c] * w,
                _ => {}
            }
        }
    }

    // From the drawing: the painted field is the layout the generator follows.
    let mut drawing = None;
    let mut drawn_fraction = None;
    if base == SketchBase::Drawing {
        ctx.progress(0.6, "Growing continents from the drawing");
        let grown = from_drawing(ctx, &land_f, &coast, coast_freq, rough);
        drawn_fraction = Some(grown.1);
        drawing = Some(std::mem::replace(&mut land_f, grown.0));
    }

    // Coastline: noise matters most where the sketch is undecided (soft edges).
    ctx.progress(0.8, "Shaping coastlines");
    let land: Vec<u8> = (0..n)
        .into_par_iter()
        .map(|i| {
            let l = land_f[i] as f64;
            let k = 1.0 - 0.6 * l * l;
            let v = l + rough * k * coast.fbm(g.pos[i], coast_freq, 7);
            (v > 0.0) as u8
        })
        .collect();

    let land_area: f64 = (0..n).filter(|&i| land[i] == 1).map(|i| g.area[i]).sum();
    let land_fraction = land_area / (4.0 * std::f64::consts::PI);
    let hint_cells = hint.iter().filter(|&&h| h > 0.05).count();

    let mut meta = serde_json::json!({
        "base": base,
        "land_fraction": land_fraction,
        "land_area_mkm2": land_area * pp.radius_km * pp.radius_km / 1e6,
        "strokes": sketch.strokes.len(),
        "hint_cells": hint_cells,
    });
    let mut f = Fields::default();
    if let Some(d) = drawing {
        // How well the result follows the drawing: the share of drawn land
        // that became land, and of the result's land that was drawn.
        let (mut both, mut drawn, mut got) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let (a, b) = (d[i] > 0.0, land[i] == 1);
            drawn += if a { g.area[i] } else { 0.0 };
            got += if b { g.area[i] } else { 0.0 };
            both += if a && b { g.area[i] } else { 0.0 };
        }
        meta["drawn_land_fraction"] = serde_json::json!(drawn_fraction.unwrap_or(0.0));
        meta["drawing_kept"] = serde_json::json!(if drawn > 0.0 { both / drawn } else { 0.0 });
        meta["drawing_precision"] = serde_json::json!(if got > 0.0 { both / got } else { 0.0 });
        f.put("drawing", Field::F32(d));
    }
    f.put("sketch", Field::F32(land_f));
    f.put("land", Field::U8(land));
    f.put("mountain_hint", Field::F32(hint));
    StepOutput { fields: f, meta }
}

/// Noise amplitude of the drawing-guided generator, in units of the freedom
/// distance: the typical coast shift is about half the freedom, and cells
/// more than about twice the freedom from a drawn coast rarely flip.
const DRAWING_NOISE: f64 = 3.5;
/// Size of the largest noise features, in freedom distances. Features about
/// four times larger than the coast shift bend coasts into peninsulas and
/// gulfs instead of fraying them (the generated continents have the same ratio).
const DRAWING_FEATURE: f64 = 6.0;
/// Domain warp of that noise, in freedom distances: curls coasts and
/// peninsulas instead of leaving them blobby.
const DRAWING_WARP: f64 = 5.0;
/// Weight of the coastline noise (the coast roughness and feature size
/// parameters), in freedom distances per unit of roughness: the same small
/// bays, headlands and islets as on generated continents.
const DRAWING_COAST: f64 = 3.0;

/// Continents grown from a drawing. The drawn coast (where the painted field
/// changes sign) becomes a signed distance in freedom units, seeded
/// domain-warped noise with features from several freedom distances down to a
/// few cells and the coastline noise are added, and each drawn landmass is given an offset so it keeps
/// its drawn area (offsets blend across the borders between landmasses' areas
/// of influence, so no seams appear). The layout and the sizes are the
/// drawing's; the coasts, peninsulas, gulfs and islands are the generator's,
/// and change with the seed. Returns the soft land field and the drawn land
/// fraction.
fn from_drawing(ctx: &Ctx, drawn: &[f32], coast: &Noise, coast_freq: f64, rough: f64) -> (Vec<f32>, f64) {
    let g = ctx.grid;
    let n = g.len();
    let pp = &ctx.params.planet;
    let r_km = pp.radius_km;
    let l_km = ctx.params.sketch.drawing_freedom_km;
    let total = 4.0 * std::f64::consts::PI;
    let land_area: f64 = (0..n).filter(|&i| drawn[i] > 0.0).map(|i| g.area[i]).sum();
    let fraction = land_area / total;
    let spacing_km = g.spacing * r_km;
    if l_km < 0.5 * spacing_km || land_area <= 0.0 || land_area >= total {
        return (drawn.to_vec(), fraction);
    }
    // Signed distance to the drawn coast, in km (positive on drawn land), and
    // the drawn landmass each sea cell is nearest to.
    let reach = 3.0 * l_km + spacing_km;
    let is_drawn = |i: usize| drawn[i] > 0.0;
    let to_sea = graph::distance_km(g, r_km, reach, |i| !is_drawn(i), |_| true);
    let (comp, masses) = graph::components(g, is_drawn);
    let src: Vec<(u32, f64, u32)> = (0..n).filter(|&i| is_drawn(i)).map(|i| (i as u32, 0.0, comp[i])).collect();
    let near = graph::multi_source(g, &src, reach, |_, _, _, len| Some(len * r_km));
    let half = 0.5 * spacing_km;

    let noise = Noise::new(pp.seed, stream::DRAWING);
    let freq = r_km / (DRAWING_FEATURE * l_km);
    let warp = DRAWING_WARP * l_km / r_km;
    let octaves = ((DRAWING_FEATURE * l_km / (2.0 * spacing_km)).max(1.0).log2().ceil() as u32 + 1).clamp(3, 8);
    let ck = DRAWING_COAST * rough;
    let raw: Vec<f64> = (0..n)
        .into_par_iter()
        .map(|i| {
            let d = if is_drawn(i) { to_sea[i] } else { near.cost[i] };
            let s = if is_drawn(i) { d.min(reach) - half } else { half - d.min(reach) };
            if d >= reach {
                // Three freedom distances from a drawn coast: the noise
                // cannot flip it (and is the costly part), so skip it.
                return s / l_km;
            }
            s / l_km + DRAWING_NOISE * noise.warped(g.pos[i], freq, octaves, warp) + ck * coast.fbm(g.pos[i], coast_freq, 6)
        })
        .collect();

    // Per landmass: the offset that gives its area of influence as much land
    // as was drawn for it.
    let mut region: Vec<Vec<u32>> = vec![Vec::new(); masses.len()];
    for i in 0..n {
        if near.label[i] != u32::MAX {
            region[near.label[i] as usize].push(i as u32);
        }
    }
    let offset: Vec<f64> = region
        .par_iter()
        .zip(masses.par_iter())
        .map(|(cells, mass)| {
            let want: f64 = mass.iter().map(|&c| g.area[c as usize]).sum();
            let mut v: Vec<(f64, u32)> = cells.iter().map(|&c| (raw[c as usize], c)).collect();
            v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(&b.1)));
            let mut acc = 0.0;
            for k in 0..v.len() {
                acc += g.area[v[k].1 as usize];
                if acc >= want {
                    let next = v.get(k + 1).map_or(v[k].0 - 1.0, |x| x.0);
                    return -0.5 * (v[k].0 + next);
                }
            }
            0.0
        })
        .collect();
    // Blend towards the neighbouring landmass's offset near the border
    // between their areas, reaching half and half on the border itself.
    let border: Vec<(u32, f64, u32)> = (0..n)
        .filter_map(|i| {
            let li = near.label[i];
            if li == u32::MAX {
                return None;
            }
            g.neighbors(i).iter().map(|&j| near.label[j as usize]).find(|&lj| lj != li && lj != u32::MAX).map(|lj| (i as u32, 0.0, lj))
        })
        .collect();
    let other = graph::multi_source(g, &border, l_km, |a, b, _, len| (near.label[a] == near.label[b]).then_some(len * r_km));
    let raw: Vec<f64> = (0..n)
        .map(|i| {
            let li = near.label[i];
            if li == u32::MAX {
                return raw[i];
            }
            let mut o = offset[li as usize];
            if other.label[i] != u32::MAX {
                let w = 0.5 * (1.0 - other.cost[i] / l_km).max(0.0);
                o += w * (offset[other.label[i] as usize] - o);
            }
            raw[i] + o
        })
        .collect();
    let thr = area_quantile(g, &raw, 1.0 - fraction);
    (raw.iter().map(|&v| ((v - thr) * 6.0).clamp(-1.0, 1.0) as f32).collect(), fraction)
}

/// The value below which `fraction` of the planet's area lies.
fn area_quantile(g: &crate::grid::Grid, raw: &[f64], fraction: f64) -> f64 {
    let mut idx: Vec<usize> = (0..g.len()).collect();
    idx.sort_by(|&a, &b| raw[a].partial_cmp(&raw[b]).unwrap().then(a.cmp(&b)));
    let target = fraction * 4.0 * std::f64::consts::PI;
    let mut acc = 0.0;
    let mut thr = raw[idx[idx.len() / 2]];
    for &i in &idx {
        acc += g.area[i];
        if acc >= target {
            thr = raw[i];
            break;
        }
    }
    thr
}

/// Generated continents: a few blobby cores plus domain-warped noise,
/// thresholded so the land fraction matches the target.
fn auto_base(ctx: &Ctx, coast: &Noise, coast_freq: f64, rough: f64) -> Vec<f32> {
    let g = ctx.grid;
    let pp = &ctx.params.planet;
    let k = ctx.params.sketch.auto_continents as usize;
    let mut rng = Rng::new(pp.seed, stream::SKETCH);
    let base = Noise::new(pp.seed, stream::SKETCH);

    // Continent cores, spread out with best-of-N sampling.
    let mut cores: Vec<(Vec3, f64)> = Vec::new();
    for _ in 0..k {
        let mut best = rng.unit_vector();
        let mut best_d = -1.0;
        for _ in 0..12 {
            let c = rng.unit_vector();
            let d = cores.iter().map(|(p, _)| p.angle_to(c)).fold(f64::INFINITY, f64::min);
            if d > best_d {
                best_d = d;
                best = c;
            }
        }
        let size = rng.range(0.35, 0.75);
        cores.push((best, size));
    }

    let raw: Vec<f64> = (0..g.len())
        .into_par_iter()
        .map(|i| {
            let p = g.pos[i];
            let noise = base.warped(p, 1.6, 6, 0.9);
            let core = cores
                .iter()
                .map(|&(c, s)| {
                    let t = (c.angle_to(p) / s).min(3.0);
                    (-t * t).exp()
                })
                .fold(0.0, f64::max);
            let v = if k > 0 { 0.9 * core + 0.55 * noise } else { noise };
            v + rough * 0.6 * coast.fbm(p, coast_freq, 5)
        })
        .collect();

    // Area-weighted quantile for the target ocean fraction.
    let thr = area_quantile(g, &raw, pp.ocean_fraction.clamp(0.02, 0.98));
    raw.iter().map(|&v| ((v - thr) * 6.0).clamp(-1.0, 1.0) as f32).collect()
}
