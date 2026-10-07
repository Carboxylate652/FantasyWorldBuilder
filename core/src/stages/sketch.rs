//! Step 2 — continent sketch. Turns sketch strokes (or a generated base) into a
//! soft land field, a land/sea mask with natural coastline noise, and a
//! "mountains here" hint field.

use super::{Ctx, StepOutput};
use crate::edits::{stroke_coverage, Tool};
use crate::fields::{Field, Fields};
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
    let mut land_f: Vec<f32> = if sketch.auto_base {
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

    let mut f = Fields::default();
    f.put("sketch", Field::F32(land_f));
    f.put("land", Field::U8(land));
    f.put("mountain_hint", Field::F32(hint));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "land_fraction": land_fraction,
            "land_area_mkm2": land_area * pp.radius_km * pp.radius_km / 1e6,
            "strokes": sketch.strokes.len(),
            "hint_cells": hint_cells,
        }),
    }
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
    let mut idx: Vec<usize> = (0..g.len()).collect();
    idx.sort_by(|&a, &b| raw[a].partial_cmp(&raw[b]).unwrap().then(a.cmp(&b)));
    let target = pp.ocean_fraction.clamp(0.02, 0.98) * 4.0 * std::f64::consts::PI;
    let mut acc = 0.0;
    let mut thr = raw[idx[idx.len() / 2]];
    for &i in &idx {
        acc += g.area[i];
        if acc >= target {
            thr = raw[i];
            break;
        }
    }
    raw.iter().map(|&v| ((v - thr) * 6.0).clamp(-1.0, 1.0) as f32).collect()
}
