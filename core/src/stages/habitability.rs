//! Step 8 — habitability and barriers (Stage 2 inputs).
//!
//! Habitability (0–1) combines growing season, water (rain or a nearby river),
//! heat, slope, altitude, terrain and nearness to the coast. It sets how many
//! provinces a state gets and, later, where people live.
//!
//! Springs: rain that soaks in becomes groundwater, which flows downhill
//! underground (losing water with distance) and surfaces at the foot of
//! mountains and on basin floors. In dry land away from rivers such a spring
//! is an oasis (Las Vegas was named for its spring-fed meadows): its site
//! value raises habitability in a few cells, so Stage 2 cuts a small, dense
//! province around it. Site pins place towns no model explains.
//!
//! Fertility paint: the user's own fertile (+) or barren (−) land, added to
//! habitability (0–1) wherever it is painted. Without paint, habitability is
//! the climate's alone. It shapes state and province seeding and sizes, and
//! where people live.
//!
//! The barrier field is the extra cost of crossing a cell when states and
//! provinces grow: ridge crests, high ground, border rivers, deep desert, ice and
//! marsh. Rivers play one of two roles: a wide river through fertile land is a
//! border (Rhine); a river through dry land holds its valley together (Nile),
//! so it carries no barrier and states grow cheaply along it. Barrier paint
//! strokes add or remove barriers by hand.

use super::biomes::terrain;
use super::hydrology::water;
use super::{Ctx, StepOutput};
use crate::edits::{stroke_coverage, Tool};
use crate::fields::{Field, Fields};
use crate::graph;
use rayon::prelude::*;

/// `river_role` values.
pub mod role {
    pub const NONE: u8 = 0;
    /// Divides land: a barrier for states and provinces.
    pub const BORDER: u8 = 1;
    /// Holds its valley together: cheap to travel along.
    pub const BACKBONE: u8 = 2;
}

/// Base suitability of each game terrain (index = terrain code).
const TERRAIN_SCORE: [f64; 18] = [
    0.0,  // ocean
    0.0,  // lake
    0.0,  // glacier
    0.35, // tundra
    0.55, // taiga
    0.85, // forest
    1.0,  // plains
    0.75, // steppe
    0.4,  // desert
    0.7,  // drylands
    1.0,  // mediterranean
    0.8,  // savanna
    0.6,  // jungle
    0.6,  // wetlands
    1.0,  // floodplains
    0.8,  // hills
    0.45, // mountains
    0.7,  // highlands
];

fn smooth01(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let g = ctx.grid;
    let n = g.len();
    let hp = &ctx.params.habitability;
    let r_km = ctx.params.planet.radius_km;
    let elev = ctx.input.f32("elevation");
    let wat = ctx.input.u8("water");
    let ter = ctx.input.u8("terrain");
    let relief = ctx.input.f32("relief");
    let temp = ctx.input.f32("temp");
    let adj = ctx.input.f32("temp_adjust");
    let p_ann = ctx.input.f32("p_ann");
    let width = ctx.input.f32("river_width");
    let land = |i: usize| wat[i] == water::LAND;

    ctx.progress(0.05, "Measuring distance to rivers and coasts");
    let d_river = graph::distance_km(g, r_km, hp.river_reach_km * 4.0, |i| land(i) && width[i] > 0.0, land);
    let d_coast = graph::distance_km(
        g,
        r_km,
        hp.coast_reach_km * 4.0,
        |i| land(i) && g.neighbors(i).iter().any(|&j| wat[j as usize] == water::OCEAN),
        land,
    );

    ctx.progress(0.3, "Scoring habitability");
    // (habitability, habitability without rivers or coast) per cell.
    let scores: Vec<(f32, f32)> = (0..n)
        .into_par_iter()
        .map(|i| {
            if !land(i) {
                return (0.0, 0.0);
            }
            let t: [f64; 12] = std::array::from_fn(|m| (temp[m * n + i] + adj[i]) as f64);
            let t_mean = t.iter().sum::<f64>() / 12.0;
            // Growing season: months above ~6 °C, saturating at six.
            let grow: f64 = t.iter().map(|&x| smooth01((x - 3.0) / 6.0)).sum();
            let f_grow = smooth01(grow / 6.0);
            let f_heat = 1.0 - 0.3 * ((t_mean - 25.0) / 8.0).clamp(0.0, 1.0);
            let p = p_ann[i] as f64;
            let mut f_rain = (p / 700.0).clamp(0.0, 1.0).powf(0.7);
            f_rain *= 1.0 - 0.25 * ((p - 2500.0) / 2500.0).clamp(0.0, 1.0);
            let a_river = if d_river[i].is_finite() { (-d_river[i] / hp.river_reach_km.max(1.0)).exp() } else { 0.0 };
            let a_coast = if d_coast[i].is_finite() { (-d_coast[i] / hp.coast_reach_km.max(1.0)).exp() } else { 0.0 };
            let f_slope = 1.0 / (1.0 + (relief[i] as f64 / 700.0).powi(2));
            let f_alt = 1.0 - ((elev[i] as f64 - 2500.0) / 2500.0).clamp(0.0, 0.9);
            let base = TERRAIN_SCORE[ter[i] as usize % TERRAIN_SCORE.len()] * f_grow * f_heat * f_slope * f_alt;
            let dry = base * f_rain;
            // Rivers water dry land (irrigation) and, with the coast, give transport.
            let mut h = base * f_rain.max(0.85 * a_river);
            h += (1.0 - h) * 0.2 * a_river * f_grow;
            h += (1.0 - h) * 0.2 * a_coast * f_grow;
            (h.clamp(0.0, 1.0) as f32, dry.clamp(0.0, 1.0) as f32)
        })
        .collect();
    let mut hab: Vec<f32> = scores.iter().map(|s| s.0).collect();
    let dry: Vec<f32> = scores.iter().map(|s| s.1).collect();

    // ---- groundwater and springs
    ctx.progress(0.45, "Tracing groundwater to springs");
    let recv = ctx.input.i32("receiver");
    let lake = ctx.input.u8("lake");
    let order = super::hydrology::upstream_first(recv);
    let r2 = r_km * r_km;
    let mut gw = vec![0.0f64; n];
    for &c in &order {
        let c = c as usize;
        if !land(c) {
            continue;
        }
        let area_m2 = g.area[c] * r2 * 1e6;
        gw[c] += hp.groundwater_recharge * (p_ann[c] as f64 - 150.0).max(0.0) / 1000.0 * area_m2 / 3.156e7;
        let r = recv[c];
        if r >= 0 && land(r as usize) {
            let km = g.pos[c].angle_to(g.pos[r as usize]) * r_km;
            gw[r as usize] += gw[c] * (-km / hp.groundwater_reach_km.max(1.0)).exp();
        }
    }
    let mut site = vec![0.0f32; n];
    let mut kind_of = vec![0u8; n]; // 1 mountain foot, 2 basin floor
    for i in 0..n {
        if !land(i) {
            continue;
        }
        // Where groundwater surfaces: below a steep rise, or at a closed basin's floor.
        let rise = g.neighbors(i).iter().map(|&j| elev[j as usize] - elev[i]).fold(0.0f32, f32::max) as f64;
        let foot = ((rise - 150.0) / 450.0).clamp(0.0, 1.0);
        let basin = if recv[i] < 0 || lake[i] == super::hydrology::lake::DRY || g.neighbors(i).iter().any(|&j| lake[j as usize] == super::hydrology::lake::SALT) { 1.0 } else { 0.0 };
        let surfacing = foot.max(basin);
        let dryness = ((hp.spring_max_precip_mm - p_ann[i] as f64) / hp.spring_max_precip_mm.max(1.0)).clamp(0.0, 1.0);
        let no_river = if d_river[i].is_finite() { 1.0 - (-d_river[i] / hp.river_reach_km.max(1.0)).exp() } else { 1.0 };
        let s = (1.0 - (-gw[i] * surfacing / hp.spring_flux_m3s.max(1e-6)).exp()) * dryness.sqrt() * no_river;
        if s > 0.3 {
            site[i] = s as f32;
            kind_of[i] = if basin >= foot { 2 } else { 1 };
        }
    }
    // Site pins: a town that reaches the stroke's value in people.
    let mut pins = Vec::new();
    for s in &ctx.edits.overrides.sites {
        if s.tool != Tool::SitePin || s.points.is_empty() {
            continue;
        }
        let c = g.nearest(crate::vec3::Vec3::from_lat_lon_deg(s.points[0][0], s.points[0][1]), None);
        if !land(c) {
            continue;
        }
        for k in std::iter::once(c).chain(g.neighbors(c).iter().map(|&j| j as usize)) {
            if land(k) {
                site[k] = 1.0;
                kind_of[k] = 3;
            }
        }
        pins.push(serde_json::json!({ "lat": s.points[0][0], "lon": s.points[0][1], "cell": c, "population": s.value.max(0.0) }));
    }
    let mut springs = Vec::new();
    for i in 0..n {
        if site[i] <= 0.0 {
            continue;
        }
        // An oasis: habitable whatever the rain, as long as crops can grow at all.
        let t_mean = (0..12).map(|m| (temp[m * n + i] + adj[i]) as f64).sum::<f64>() / 12.0;
        let warmth = smooth01((t_mean + 5.0) / 10.0);
        let h = (hp.spring_habitability * site[i] as f64 * warmth).max(if kind_of[i] == 3 { 0.9 } else { 0.0 });
        hab[i] = hab[i].max(h as f32);
        // List each spring warm enough to farm once: the strongest cell among its neighbours.
        if kind_of[i] < 3 && warmth > 0.3 && g.neighbors(i).iter().all(|&j| site[j as usize] < site[i] || (site[j as usize] == site[i] && (j as usize) > i)) {
            let (la, lo) = g.pos[i].lat_lon();
            springs.push(serde_json::json!({
                "lat": (la.to_degrees() * 100.0).round() / 100.0, "lon": (lo.to_degrees() * 100.0).round() / 100.0,
                "kind": if kind_of[i] == 2 { "basin" } else { "mountain foot" },
                "flow_m3s": (gw[i] * 100.0).round() / 100.0, "site": (site[i] * 100.0).round() / 100.0, "cell": i,
            }));
        }
    }
    springs.sort_by(|a, b| b["site"].as_f64().partial_cmp(&a["site"].as_f64()).unwrap());

    // ---- fertility paint
    let mut fert = vec![0.0f32; n];
    let mut fert_scratch = Vec::new();
    for s in &ctx.edits.overrides.fertility {
        for (c, w) in stroke_coverage(g, s, r_km, &mut fert_scratch) {
            let c = c as usize;
            match s.tool {
                Tool::FertilityPaint => fert[c] = (fert[c] + (s.value.clamp(-1.0, 1.0) * w as f64) as f32).clamp(-1.0, 1.0),
                Tool::FertilityErase => fert[c] *= 1.0 - w,
                _ => {}
            }
        }
    }
    let mut fertility_cells = 0usize;
    for i in 0..n {
        if land(i) && fert[i] != 0.0 {
            hab[i] = (hab[i] + fert[i]).clamp(0.0, 1.0);
            fertility_cells += 1;
        }
    }

    // Regional dry habitability and rainfall (a few rings), used to decide each river's role.
    let mut regional = dry.clone();
    let mut regional_p: Vec<f32> = (0..n).map(|i| if land(i) { p_ann[i] } else { 0.0 }).collect();
    for _ in 0..4 {
        regional = (0..n)
            .into_par_iter()
            .map(|i| {
                if !land(i) {
                    return 0.0;
                }
                let (mut s, mut k) = (regional[i], 1.0f32);
                for &j in g.neighbors(i) {
                    if land(j as usize) {
                        s += regional[j as usize];
                        k += 1.0;
                    }
                }
                s / k
            })
            .collect();
        regional_p = (0..n)
            .into_par_iter()
            .map(|i| {
                if !land(i) {
                    return 0.0;
                }
                let (mut s, mut k) = (regional_p[i], 1.0f32);
                for &j in g.neighbors(i) {
                    if land(j as usize) {
                        s += regional_p[j as usize];
                        k += 1.0;
                    }
                }
                s / k
            })
            .collect();
    }

    ctx.progress(0.55, "Finding barriers");
    let w_border = hp.river_border_width_m.max(1.0);
    let mut river_role = vec![role::NONE; n];
    for i in 0..n {
        let w = width[i] as f64;
        if !land(i) || w <= 0.0 {
            continue;
        }
        let fertile = regional[i] as f64 >= hp.river_border_habitability;
        let arid = (regional_p[i] as f64) < hp.backbone_max_precip_mm;
        river_role[i] = if w >= w_border && fertile && !arid {
            role::BORDER
        } else if w >= w_border / 3.0 && arid {
            role::BACKBONE
        } else {
            role::NONE
        };
    }
    let mut barrier: Vec<f32> = (0..n)
        .into_par_iter()
        .map(|i| {
            if !land(i) {
                return 0.0;
            }
            let e = elev[i].max(0.0) as f64;
            // Ridge crest: higher than the surrounding two rings.
            let (mut s, mut k) = (0.0, 0.0);
            for &a in g.neighbors(i) {
                for &b in g.neighbors(a as usize) {
                    s += elev[b as usize].max(0.0) as f64;
                    k += 1.0;
                }
            }
            let crest = ((e - s / k) / 250.0).clamp(0.0, 1.0) * (e / 1500.0).clamp(0.0, 1.0);
            let high = ((e - 800.0) / 2500.0).clamp(0.0, 1.0);
            let mut b = hp.barrier_ridge * crest + hp.barrier_mountain * high;
            // Deep desert away from rivers.
            let arid = (1.0 - p_ann[i] as f64 / 250.0).clamp(0.0, 1.0);
            let a_river = if d_river[i].is_finite() { (-d_river[i] / hp.river_reach_km.max(1.0)).exp() } else { 0.0 };
            b += hp.barrier_desert * arid * (1.0 - a_river);
            b += match ter[i] {
                terrain::GLACIER => hp.barrier_ice,
                terrain::WETLANDS => hp.barrier_marsh,
                terrain::JUNGLE => 0.6 * hp.barrier_marsh,
                _ => 0.0,
            };
            if river_role[i] == role::BORDER {
                let w = width[i] as f64;
                b += hp.barrier_river * (0.5 + 0.5 * (w / w_border).log2()).clamp(0.5, 1.5);
            }
            b as f32
        })
        .collect();

    // Barrier paint.
    let mut scratch = Vec::new();
    let mut painted = 0usize;
    for s in &ctx.edits.overrides.barriers {
        for (c, w) in stroke_coverage(g, s, r_km, &mut scratch) {
            let c = c as usize;
            if !land(c) {
                continue;
            }
            match s.tool {
                Tool::BarrierPaint => barrier[c] += (s.value.max(0.0) * w as f64) as f32,
                Tool::BarrierErase => {
                    barrier[c] *= 1.0 - w;
                    if w >= 0.5 && river_role[c] == role::BORDER {
                        river_role[c] = role::NONE;
                    }
                }
                _ => continue,
            }
            painted += 1;
        }
    }

    // Summary.
    let (mut land_a, mut hab_a, mut cap, mut good) = (0.0, 0.0, 0.0, 0.0);
    let (mut border_km, mut backbone_km) = (0.0, 0.0);
    let spacing_km = g.spacing * r_km;
    for i in 0..n {
        if !land(i) {
            continue;
        }
        let a = g.area[i] * r2;
        land_a += a;
        hab_a += a * hab[i] as f64;
        cap += a * hab[i] as f64;
        if hab[i] >= 0.4 {
            good += a;
        }
        match river_role[i] {
            role::BORDER => border_km += spacing_km,
            role::BACKBONE => backbone_km += spacing_km,
            _ => {}
        }
    }
    hab.iter_mut().for_each(|h| *h = h.clamp(0.0, 1.0));

    let mut f = Fields::default();
    f.put("habitability", Field::F32(hab));
    f.put("barrier", Field::F32(barrier));
    f.put("river_role", Field::U8(river_role));
    f.put("groundwater", Field::F32(gw.iter().map(|&x| x as f32).collect()));
    f.put("site", Field::F32(site));
    f.put("site_kind", Field::U8(kind_of));
    f.put("fertility", Field::F32(fert));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "mean_habitability": hab_a / land_a.max(1.0),
            "habitable_share": good / land_a.max(1.0),
            "capacity_mkm2": cap / 1e6,
            "border_river_km": border_km,
            "backbone_river_km": backbone_km,
            "painted_cells": painted,
            "springs": springs.len(),
            "fertility_cells": fertility_cells,
            "sites": { "springs": springs, "pins": pins },
        }),
    }
}
