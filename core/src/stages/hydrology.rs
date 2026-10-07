//! Step 6 — hydrology and erosion.
//!
//! 1. Ocean = sea-level water bodies connected to the world ocean. Small
//!    isolated basins below sea level (Caspian/Dead Sea style) are land
//!    depressions whose water level is set by their water balance.
//! 2. Priority-flood routing on the filled surface; implicit stream-power and
//!    thermal erosion that keeps the sketched coastline.
//! 3. Lakes by water balance: each closed depression fills from its deepest
//!    point until open-water evaporation (minus rain on the lake) balances the
//!    inflow. If the basin fills to its spill point the lake overflows (fresh);
//!    otherwise it is terminal (salt) and the basin floor above it stays dry, or
//!    only a playa remains when there is almost no inflow.
//! 4. Lake–climate feedback: the climate is re-solved with lakes as open water
//!    (evaporation source, maritime influence), then lakes are re-balanced.
//! 5. Discharge, rivers ranked by Strahler order.

use super::climate;
use super::{Ctx, StepOutput, Upstream};
use crate::fields::{Field, Fields};
use crate::grid::Grid;
use crate::params::HydroParams;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(PartialEq)]
struct Item(f64, u32);
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal).then_with(|| o.1.cmp(&self.1))
    }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Lake classes in the `lake` field.
pub mod lake {
    pub const NONE: u8 = 0;
    /// Overflowing lake: drains on downstream.
    pub const FRESH: u8 = 1;
    /// Terminal lake: evaporation balances inflow below the spill point.
    pub const SALT: u8 = 2;
    /// Playa / salt flat: a closed basin with too little water for a lake.
    pub const DRY: u8 = 3;
}

/// Surface classes in the `water` field.
pub mod water {
    pub const LAND: u8 = 0;
    pub const OCEAN: u8 = 1;
    pub const LAKE: u8 = 2;
}

const EPS: f64 = 1e-3;
/// mm·km²/yr → m³/s
const TO_M3S: f64 = 1e-3 * 1e6 / 31_557_600.0;

struct Flow {
    filled: Vec<f64>,
    receiver: Vec<i32>,
    /// Cells in ascending filled order: every receiver precedes its donors.
    order: Vec<u32>,
}

/// Priority-flood + ε (Barnes et al. 2014). Ocean cells are outlets.
fn route(g: &Grid, h: &[f64], ocean: &[bool]) -> Flow {
    let n = g.len();
    let mut filled = vec![f64::NAN; n];
    let mut done = vec![false; n];
    let mut heap = BinaryHeap::new();
    for i in 0..n {
        if ocean[i] {
            filled[i] = h[i];
            done[i] = true;
            if g.neighbors(i).iter().any(|&j| !ocean[j as usize]) {
                heap.push(Item(h[i].min(0.0), i as u32));
            }
        }
    }
    if heap.is_empty() {
        // No ocean at all: drain to the lowest cell.
        let low = (0..n).min_by(|&a, &b| h[a].partial_cmp(&h[b]).unwrap()).unwrap_or(0);
        filled[low] = h[low];
        done[low] = true;
        heap.push(Item(h[low], low as u32));
    }
    let mut parent = vec![-1i32; n];
    while let Some(Item(level, c)) = heap.pop() {
        let c = c as usize;
        for &nb in g.neighbors(c) {
            let j = nb as usize;
            if done[j] {
                continue;
            }
            done[j] = true;
            filled[j] = h[j].max(level + EPS);
            parent[j] = c as i32;
            heap.push(Item(filled[j], j as u32));
        }
    }
    // Steepest descent on the filled surface.
    let receiver: Vec<i32> = (0..n)
        .into_par_iter()
        .map(|i| {
            if ocean[i] {
                return -1;
            }
            let mut best = parent[i];
            let mut best_s = 0.0;
            for &nb in g.neighbors(i) {
                let j = nb as usize;
                let drop = filled[i] - filled[j];
                if drop > 0.0 {
                    let s = drop / g.pos[i].angle_to(g.pos[j]);
                    if s > best_s {
                        best_s = s;
                        best = j as i32;
                    }
                }
            }
            best
        })
        .collect();
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by(|&a, &b| filled[a as usize].partial_cmp(&filled[b as usize]).unwrap_or(Ordering::Equal).then(a.cmp(&b)));
    Flow { filled, receiver, order }
}

/// Donors-before-receivers order for an arbitrary receiver forest (Kahn).
fn upstream_first(receiver: &[i32]) -> Vec<u32> {
    let n = receiver.len();
    let mut indeg = vec![0u32; n];
    for &r in receiver {
        if r >= 0 {
            indeg[r as usize] += 1;
        }
    }
    let mut order: Vec<u32> = (0..n as u32).filter(|&i| indeg[i as usize] == 0).collect();
    let mut k = 0;
    while k < order.len() {
        let r = receiver[order[k] as usize];
        if r >= 0 {
            let r = r as usize;
            indeg[r] -= 1;
            if indeg[r] == 0 {
                order.push(r as u32);
            }
        }
        k += 1;
    }
    if order.len() < n {
        // Should not happen; keep any cells caught in a cycle so nothing is lost.
        let mut seen = vec![false; n];
        order.iter().for_each(|&c| seen[c as usize] = true);
        order.extend((0..n as u32).filter(|&c| !seen[c as usize]));
    }
    order
}

/// Accumulate `local` down the receivers. `sink` is removed at a cell before it
/// passes water on (a negative sink adds water, e.g. rain on a lake).
fn accumulate(order: &[u32], receiver: &[i32], local: &[f64], sink: &[f64]) -> Vec<f64> {
    let mut q = local.to_vec();
    for &c in order {
        let c = c as usize;
        if sink[c] != 0.0 {
            q[c] = (q[c] - sink[c]).max(0.0);
        }
        let r = receiver[c];
        if r >= 0 {
            q[r as usize] += q[c];
        }
    }
    q
}

/// Like `accumulate`, but cells already carrying a river (q ≥ q_min) lose the
/// fraction `loss[c]` of their flow to evaporation and seepage before passing it
/// on. Returns the discharge and the total lost.
fn accumulate_channel(order: &[u32], receiver: &[i32], local: &[f64], sink: &[f64], loss: &[f64], q_min: f64) -> (Vec<f64>, f64) {
    let mut q = local.to_vec();
    let mut lost = 0.0;
    for &c in order {
        let c = c as usize;
        if sink[c] != 0.0 {
            q[c] = (q[c] - sink[c]).max(0.0);
        }
        if loss[c] > 0.0 && q[c] >= q_min {
            let l = q[c] * loss[c];
            q[c] -= l;
            lost += l;
        }
        let r = receiver[c];
        if r >= 0 {
            q[r as usize] += q[c];
        }
    }
    (q, lost)
}

/// Per-cell water budget from monthly climate (all in mm/yr).
struct Budget {
    /// Land runoff Σ max(0, P − PET).
    runoff: Vec<f64>,
    /// Open-water evaporation.
    evap_water: Vec<f64>,
    /// Land potential evapotranspiration.
    pet: Vec<f64>,
    precip: Vec<f64>,
}

fn budget(n: usize, temp: &[f32], adj: Option<&[f32]>, precip: &[f32], open_water: f64) -> Budget {
    let rows: Vec<(f64, f64, f64)> = (0..n)
        .into_par_iter()
        .map(|i| {
            let (mut ro, mut pet_sum, mut p_sum) = (0.0, 0.0, 0.0);
            let a = adj.map_or(0.0, |a| a[i] as f64);
            for m in 0..12 {
                let t = (temp[m * n + i] as f64 + a).max(0.0);
                let pet = 4.6 * t + 0.06 * t * t;
                let p = precip[m * n + i] as f64;
                pet_sum += pet;
                p_sum += p;
                ro += (p - pet).max(0.0);
            }
            (ro, pet_sum * open_water, p_sum)
        })
        .collect();
    Budget {
        runoff: rows.iter().map(|r| r.0).collect(),
        evap_water: rows.iter().map(|r| r.1).collect(),
        pet: rows.iter().map(|r| r.1 / open_water.max(1e-9)).collect(),
        precip: rows.iter().map(|r| r.2).collect(),
    }
}

struct LakeInfo {
    cells: usize,
    area_km2: f64,
    level_m: f64,
    inflow_m3s: f64,
    evaporation_m3s: f64,
    outflow_m3s: f64,
    class: u8,
    centroid: crate::vec3::Vec3,
}

struct Lakes {
    receiver: Vec<i32>,
    order: Vec<u32>,
    local: Vec<f64>,
    sink: Vec<f64>,
    class: Vec<u8>,
    level: Vec<f32>,
    water: Vec<u8>,
    info: Vec<LakeInfo>,
}

/// Fill each closed depression to the level its water balance supports and
/// re-route the water inside it to the lake.
fn solve_lakes(g: &Grid, h: &[f64], flow: &Flow, ocean: &[bool], b: &Budget, area_km2: &[f64], hp: &HydroParams) -> Lakes {
    let n = g.len();
    let mut receiver = flow.receiver.clone();
    let mut local: Vec<f64> = (0..n).map(|i| if ocean[i] { 0.0 } else { b.runoff[i] * area_km2[i] }).collect();
    let mut sink = vec![0.0f64; n];
    let mut class = vec![lake::NONE; n];
    let mut level = vec![f32::NAN; n];
    let mut water: Vec<u8> = ocean.iter().map(|&o| if o { water::OCEAN } else { water::LAND }).collect();
    let mut info = Vec::new();

    // Inflow at each cell with no lakes (the filled surface routes through depressions).
    let q0 = accumulate(&flow.order.iter().rev().copied().collect::<Vec<_>>(), &flow.receiver, &local, &vec![0.0; n]);
    let depth: Vec<f64> = (0..n).map(|i| if ocean[i] { 0.0 } else { flow.filled[i] - h[i] }).collect();
    let (comp, depressions) = crate::graph::components(g, |i| depth[i] > 0.5);

    for (cid, cells) in depressions.iter().enumerate() {
        // Rivers incise the basin outlet, lowering the spill level by up to
        // breach_depth_m; only the part of the basin below that can hold a lake.
        let spill = cells.iter().map(|&c| flow.filled[c as usize]).fold(f64::MIN, f64::max);
        let cap = spill - hp.breach_depth_m.max(0.0);
        let bottom = cells.iter().map(|&c| h[c as usize]).fold(f64::MAX, f64::min);
        let max_d = cap - bottom;
        if cells.len() < 3 || max_d < hp.lake_min_depth_m {
            continue; // shallow or breached basins are simply routed through
        }
        let inside = |c: usize| comp[c] == cid as u32;
        // Spill point: the depression cell that hands most water out of the basin.
        let outlet = cells
            .iter()
            .map(|&c| c as usize)
            .filter(|&c| flow.receiver[c] >= 0 && !inside(flow.receiver[c] as usize))
            .max_by(|&a, &b| q0[a].partial_cmp(&q0[b]).unwrap().then(b.cmp(&a)));
        let Some(outlet) = outlet else { continue };
        let exit = flow.receiver[outlet];
        let own_runoff: f64 = cells.iter().map(|&c| local[c as usize]).sum();
        let inflow_ext = (q0[outlet] - own_runoff).max(0.0);

        // Fill from the bottom until evaporation balances supply.
        let mut all_bed: Vec<usize> = cells.iter().map(|&c| c as usize).collect();
        all_bed.sort_by(|&a, &b| h[a].partial_cmp(&h[b]).unwrap().then(a.cmp(&b)));
        // Cells that can be under water at the (incised) spill level.
        let by_bed: Vec<usize> = all_bed.iter().copied().filter(|&c| h[c] < cap).collect();
        let mut land_rest = own_runoff;
        let mut gain = 0.0; // rain on the lake minus open-water evaporation
        let mut k_lake = by_bed.len();
        let mut supply = inflow_ext + land_rest;
        for (k, &c) in by_bed.iter().enumerate() {
            let g_c = (b.precip[c] - b.evap_water[c]) * area_km2[c];
            let s = inflow_ext + (land_rest - local[c]) + gain + g_c;
            if s < 0.0 {
                k_lake = k;
                break;
            }
            land_rest -= local[c];
            gain += g_c;
            supply = s;
        }
        let full = k_lake == by_bed.len();
        let lake_cells = &by_bed[..k_lake];
        let lake_class = if full {
            lake::FRESH
        } else if k_lake >= 1 {
            lake::SALT
        } else {
            lake::DRY
        };

        // Re-route inside the basin so all its water drains to the lake rather than
        // to the spill point: priority-flood from the deepest cell gives a surface
        // that falls monotonically toward it, then each cell drains by steepest
        // descent on that surface (branching channels, not radial flood paths).
        let root = by_bed[0];
        let mut flood = crate::grid::FastMap::<u32, f64>::default();
        let mut heap = BinaryHeap::new();
        heap.push(Item(h[root], root as u32));
        flood.insert(root as u32, h[root]);
        let member: std::collections::HashSet<u32> = cells.iter().copied().collect();
        while let Some(Item(lv, c)) = heap.pop() {
            for &nb in g.neighbors(c as usize) {
                if member.contains(&nb) && !flood.contains_key(&nb) {
                    let l = h[nb as usize].max(lv + EPS);
                    flood.insert(nb, l);
                    heap.push(Item(l, nb));
                }
            }
        }
        for &c in cells {
            if c as usize == root {
                continue;
            }
            let lc = flood[&c];
            let mut best = -1i32;
            let mut best_s = 0.0;
            for &nb in g.neighbors(c as usize) {
                if let Some(&ln) = flood.get(&nb) {
                    let drop = lc - ln;
                    if drop > 0.0 {
                        let s = drop / g.pos[c as usize].angle_to(g.pos[nb as usize]);
                        if s > best_s {
                            best_s = s;
                            best = nb as i32;
                        }
                    }
                }
            }
            receiver[c as usize] = best;
        }

        let water_level = if full {
            cap
        } else if k_lake >= 1 {
            0.5 * (h[by_bed[k_lake - 1]] + h[by_bed[k_lake.min(by_bed.len() - 1)]])
        } else {
            h[root]
        };
        let lake_area: f64 = lake_cells.iter().map(|&c| area_km2[c]).sum();
        let evap: f64 = lake_cells.iter().map(|&c| b.evap_water[c] * area_km2[c]).sum();
        let mut centroid = crate::vec3::Vec3::ZERO;
        for &c in lake_cells {
            local[c] = 0.0; // rain on the lake is in the root's balance
            class[c] = lake_class;
            level[c] = water_level as f32;
            water[c] = water::LAKE;
            centroid += g.pos[c] * area_km2[c];
        }
        if full {
            // Overflow: the basin passes on what evaporation leaves (negative gain = net loss).
            receiver[root] = exit;
            sink[root] = -gain;
        } else {
            receiver[root] = -1; // terminal basin
            if k_lake == 0 {
                // Playa: the floor cells within a few metres of the bottom.
                for &c in &all_bed {
                    if h[c] <= h[root] + 0.1 * max_d {
                        class[c] = lake::DRY;
                    }
                }
                centroid = g.pos[root];
            }
        }
        info.push(LakeInfo {
            cells: k_lake,
            area_km2: lake_area,
            level_m: water_level,
            inflow_m3s: (inflow_ext + own_runoff) * TO_M3S,
            evaporation_m3s: evap * TO_M3S,
            outflow_m3s: if full { supply.max(0.0) * TO_M3S } else { 0.0 },
            class: lake_class,
            centroid: centroid.normalized(),
        });
    }
    let order = upstream_first(&receiver);
    Lakes { receiver, order, local, sink, class, level, water, info }
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let g = ctx.grid;
    let n = g.len();
    let p = ctx.params;
    let hp = &p.hydrology;
    let r_km = p.planet.radius_km;
    let elev0 = ctx.input.f32("elevation");
    let spacing_km = g.spacing * r_km;
    let area_km2: Vec<f64> = g.area.iter().map(|a| a * r_km * r_km).collect();

    // Ocean: water bodies at or below sea level that are large or the largest.
    ctx.progress(0.02, "Separating ocean from inland basins");
    let (_, seas) = crate::graph::components(g, |i| elev0[i] <= 0.0);
    let sea_area: Vec<f64> = seas.iter().map(|c| c.iter().map(|&i| area_km2[i as usize]).sum()).collect();
    let biggest = sea_area.iter().cloned().fold(0.0, f64::max);
    let mut ocean = vec![false; n];
    let mut inland_seas = 0;
    for (k, cells) in seas.iter().enumerate() {
        if sea_area[k] >= biggest || sea_area[k] >= hp.inland_sea_max_mkm2 * 1e6 {
            cells.iter().for_each(|&c| ocean[c as usize] = true);
        } else {
            inland_seas += 1;
        }
    }
    // Only land above sea level erodes (inland basin floors keep their depth).
    let erodible: Vec<bool> = (0..n).map(|i| !ocean[i] && elev0[i] > 0.0).collect();
    // Micro-relief for routing only: real flats are never perfectly flat, and on
    // smooth slopes and flats water runs in straight parallel channels. Tens of metres of
    // seeded noise at 50–150 km scales lets rivers wander and merge naturally.
    let micro: Vec<f64> = {
        let nz = crate::noise::Noise::new(p.planet.seed, crate::rng::stream::ROUTING);
        (0..n)
            .into_par_iter()
            .map(|i| if ocean[i] { 0.0 } else { 35.0 * nz.fbm(g.pos[i], r_km / 150.0, 3) + 10.0 * nz.fbm(g.pos[i], r_km / 45.0, 2) + 1.5 * crate::rng::hash_unit(p.planet.seed, crate::rng::stream::ROUTING, i as u64) })
            .collect()
    };
    let rough = |h: &[f64]| -> Vec<f64> { h.iter().zip(&micro).map(|(a, b)| a + b).collect() };

    let temp0 = ctx.input.f32("temp");
    let precip0 = ctx.input.f32("precip");
    let mut bud = budget(n, temp0, None, precip0, hp.open_water_evap);

    // ---- Erosion passes.
    let mut h: Vec<f64> = elev0.iter().map(|&e| e as f64).collect();
    let k_sp = 0.012 * hp.erosion_strength.max(0.0);
    let talus = hp.talus_m_per_km.max(1.0);
    let passes = hp.erosion_passes as usize;
    let local0: Vec<f64> = (0..n).map(|i| if ocean[i] { 0.0 } else { bud.runoff[i] * area_km2[i] }).collect();
    let zero = vec![0.0; n];
    for pass in 0..passes {
        ctx.progress(0.06 + 0.4 * pass as f32 / passes.max(1) as f32, &format!("Erosion pass {}/{}", pass + 1, passes));
        let flow = route(g, &rough(&h), &ocean);
        let up: Vec<u32> = flow.order.iter().rev().copied().collect();
        let q = accumulate(&up, &flow.receiver, &local0, &zero);
        // Implicit stream power, receivers first.
        for &c in &flow.order {
            let i = c as usize;
            let r = flow.receiver[i];
            if !erodible[i] || r < 0 {
                continue;
            }
            let hr = h[r as usize];
            if h[i] <= hr {
                continue;
            }
            let a_eff = q[i] / 500.0; // km² of "wet" catchment
            let f = k_sp * a_eff.sqrt() / spacing_km;
            h[i] = ((h[i] + f * hr) / (1.0 + f)).max(1.0);
        }
        // Thermal erosion: move material down slopes steeper than talus.
        for _ in 0..hp.thermal_passes {
            let delta: Vec<f64> = (0..n)
                .into_par_iter()
                .map(|i| {
                    if !erodible[i] {
                        return 0.0;
                    }
                    let mut d = 0.0;
                    for &nb in g.neighbors(i) {
                        let j = nb as usize;
                        let lim = talus * g.pos[i].angle_to(g.pos[j]) * r_km;
                        d += 0.12 * ((h[j] - h[i] - lim).max(0.0) - (h[i] - h[j] - lim).max(0.0));
                    }
                    d
                })
                .collect();
            for i in 0..n {
                if erodible[i] {
                    h[i] = (h[i] + delta[i]).max(1.0);
                }
            }
        }
    }

    // ---- Lakes, with lake–climate feedback.
    ctx.progress(0.5, "Balancing lakes");
    let h_route = rough(&h);
    let flow = route(g, &h_route, &ocean);
    let mut lakes = solve_lakes(g, &h_route, &flow, &ocean, &bud, &area_km2, hp);
    let mut climate_out: Option<StepOutput> = None;
    for it in 0..hp.lake_climate_feedback {
        // Only needed where the open-water map differs from what the climate step
        // assumed (sea level = water): lakes, and inland seas that shrank.
        let differs = (0..n).any(|i| (lakes.water[i] != water::LAND) != (elev0[i] <= 0.0));
        if !differs {
            break;
        }
        let iters = hp.lake_climate_feedback.max(1) as f32;
        let pf = |fr: f32, m: &str| ctx.progress(0.55 + 0.3 * (it as f32 + fr) / iters, &format!("Lake feedback: {m}"));
        let sub = Ctx { grid: ctx.grid, params: ctx.params, edits: ctx.edits, input: Upstream { steps: ctx.input.steps.clone() }, progress_fn: &pf };
        let surface: Vec<f32> = (0..n).map(|i| if lakes.level[i].is_finite() { lakes.level[i].max(h[i] as f32) } else { h[i] as f32 }).collect();
        let out = climate::solve(&sub, &surface, &lakes.water);
        bud = budget(n, out.fields.f32("temp"), None, out.fields.f32("precip"), hp.open_water_evap);
        lakes = solve_lakes(g, &h_route, &flow, &ocean, &bud, &area_km2, hp);
        climate_out = Some(out);
    }

    // ---- Discharge and rivers.
    ctx.progress(0.85, "Accumulating flow");
    // Flux down the network: tributaries add up at every confluence, and rivers
    // crossing dry land lose water (Nile-style), scaled by aridity and reach length.
    let thr = hp.river_threshold_m3s.max(1.0);
    let loss: Vec<f64> = (0..n)
        .map(|i| {
            let r = lakes.receiver[i];
            if ocean[i] || r < 0 || lakes.water[i] == water::LAKE || bud.pet[i] <= 0.0 {
                return 0.0;
            }
            let aridity = (1.0 - bud.precip[i] / bud.pet[i]).clamp(0.0, 1.0);
            let reach_km = g.pos[i].angle_to(g.pos[r as usize]) * r_km;
            (hp.channel_loss * aridity * reach_km / 100.0).clamp(0.0, 0.5)
        })
        .collect();
    let (q, lost) = accumulate_channel(&lakes.order, &lakes.receiver, &lakes.local, &lakes.sink, &loss, 0.5 * thr / TO_M3S);
    let discharge: Vec<f32> = q.iter().map(|&x| (x * TO_M3S) as f32).collect();
    let mut drainage: Vec<f64> = (0..n).map(|i| if ocean[i] { 0.0 } else { area_km2[i] }).collect();
    for &c in &lakes.order {
        let r = lakes.receiver[c as usize];
        if r >= 0 {
            drainage[r as usize] += drainage[c as usize];
        }
    }

    ctx.progress(0.9, "Ranking rivers");
    let is_river: Vec<bool> = (0..n).map(|i| !ocean[i] && lakes.water[i] != water::LAKE && discharge[i] as f64 >= thr).collect();
    // Drop river systems whose longest course is shorter than the minimum length
    // (one- or two-cell coastal stubs in wet regions).
    let mut is_river = is_river;
    {
        let mut up_len = vec![0.0f64; n];
        for &c in &lakes.order {
            let i = c as usize;
            if !is_river[i] {
                continue;
            }
            let r = lakes.receiver[i];
            if r >= 0 && is_river[r as usize] {
                let l = up_len[i] + g.pos[i].angle_to(g.pos[r as usize]) * r_km;
                if l > up_len[r as usize] {
                    up_len[r as usize] = l;
                }
            }
        }
        let mut mouth = vec![u32::MAX; n];
        for &c in lakes.order.iter().rev() {
            let i = c as usize;
            if !is_river[i] {
                continue;
            }
            let r = lakes.receiver[i];
            mouth[i] = if r >= 0 && is_river[r as usize] { mouth[r as usize] } else { i as u32 };
        }
        for i in 0..n {
            if is_river[i] && up_len[mouth[i] as usize] + spacing_km < hp.min_river_length_km {
                is_river[i] = false;
            }
        }
    }
    let mut strahler = vec![0u8; n];
    let mut in_max = vec![0u8; n];
    let mut in_cnt = vec![0u8; n];
    let mut length_km = vec![0.0f64; n];
    for &c in &lakes.order {
        let i = c as usize;
        if !is_river[i] {
            continue;
        }
        strahler[i] = if in_max[i] == 0 { 1 } else if in_cnt[i] >= 2 { in_max[i].saturating_add(1) } else { in_max[i] };
        let r = lakes.receiver[i];
        if r >= 0 {
            let r = r as usize;
            if strahler[i] > in_max[r] {
                in_max[r] = strahler[i];
                in_cnt[r] = 1;
            } else if strahler[i] == in_max[r] {
                in_cnt[r] = in_cnt[r].saturating_add(1);
            }
            let l = length_km[i] + g.pos[i].angle_to(g.pos[r]) * r_km;
            if l > length_km[r] {
                length_km[r] = l;
            }
        }
    }
    // Hydraulic geometry (Leopold & Maddock): bankfull width w = a·Q^b.
    let width_of = |q: f64| hp.width_coeff * q.max(0.0).powf(hp.width_exponent);
    let river_width: Vec<f32> = (0..n).map(|i| if is_river[i] { width_of(discharge[i] as f64) as f32 } else { 0.0 }).collect();
    // Width classes 1–9 (export palette), each a factor √2 in width.
    let w_thr = width_of(thr);
    let river_rank: Vec<u8> = (0..n)
        .map(|i| if is_river[i] { (1.0 + 2.0 * (river_width[i] as f64 / w_thr).log2().max(0.0)).min(9.0) as u8 } else { 0 })
        .collect();
    let mut mouths: Vec<(usize, f64)> = (0..n)
        .filter(|&i| is_river[i] && (lakes.receiver[i] < 0 || !is_river[lakes.receiver[i] as usize]))
        .map(|i| (i, discharge[i] as f64))
        .collect();
    mouths.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    let major: Vec<serde_json::Value> = mouths
        .iter()
        .take(8)
        .map(|&(i, qd)| {
            serde_json::json!({
                "lat": g.lat[i].to_degrees(), "lon": g.lon[i].to_degrees(),
                "discharge_m3s": qd, "length_km": length_km[i], "order": strahler[i],
                "basin_mkm2": drainage[i] / 1e6, "width_m": river_width[i],
            })
        })
        .collect();

    // Lake summary, largest first.
    let mut info = lakes.info;
    info.sort_by(|a, b| b.area_km2.partial_cmp(&a.area_km2).unwrap());
    let lake_list: Vec<serde_json::Value> = info
        .iter()
        .filter(|l| l.cells > 0 || l.class == lake::DRY)
        .take(10)
        .map(|l| {
            let (la, lo) = l.centroid.lat_lon();
            serde_json::json!({
                "lat": la.to_degrees(), "lon": lo.to_degrees(), "cells": l.cells, "area_km2": l.area_km2,
                "level_m": l.level_m, "inflow_m3s": l.inflow_m3s, "evaporation_m3s": l.evaporation_m3s,
                "outflow_m3s": l.outflow_m3s,
                "class": match l.class { lake::FRESH => "fresh", lake::SALT => "terminal (salt)", _ => "dry (playa)" },
            })
        })
        .collect();
    let n_lakes = info.iter().filter(|l| l.cells > 0).count();
    let n_closed = info.iter().filter(|l| l.class != lake::FRESH).count();
    let lake_area: f64 = info.iter().map(|l| l.area_km2).sum();

    let lapse = (p.climate.lapse_rate_c_per_km / 1000.0) as f32;
    let elevation: Vec<f32> = h.iter().map(|&x| x as f32).collect();
    // If the climate was re-solved it already used the eroded surface.
    let temp_adjust: Vec<f32> = if climate_out.is_some() {
        vec![0.0; n]
    } else {
        (0..n).map(|i| -lapse * (elevation[i].max(0.0) - elev0[i].max(0.0))).collect()
    };
    let erosion: Vec<f32> = (0..n).map(|i| elevation[i] - elev0[i]).collect();
    let eroded_km3: f64 = (0..n).map(|i| (-erosion[i] as f64).max(0.0) / 1000.0 * area_km2[i]).sum();
    let n_river = is_river.iter().filter(|&&r| r).count();

    let mut f = Fields::default();
    let mut feedback = serde_json::Value::Null;
    if let Some(out) = climate_out {
        // The re-solved climate shadows the first-pass climate for later steps.
        feedback = serde_json::json!({
            "global_mean_temp_c": out.meta["global_mean_temp_c"],
            "global_precip_mm": out.meta["global_precip_mm"],
        });
        for (name, field) in out.fields.0 {
            f.put(&name, field);
        }
    }
    f.put("elevation", Field::F32(elevation));
    f.put("receiver", Field::I32(lakes.receiver));
    f.put("discharge", Field::F32(discharge));
    f.put("drainage_area", Field::F32(drainage.iter().map(|&x| x as f32).collect()));
    f.put("river", Field::U8(strahler));
    f.put("river_rank", Field::U8(river_rank));
    f.put("river_width", Field::F32(river_width));
    f.put("lake", Field::U8(lakes.class));
    f.put("water", Field::U8(lakes.water));
    f.put("water_level", Field::F32(lakes.level));
    f.put("temp_adjust", Field::F32(temp_adjust));
    f.put("erosion", Field::F32(erosion));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "river_cells": n_river,
            "channel_loss_m3s": lost * TO_M3S,
            "lakes": n_lakes,
            "endorheic_lakes": n_closed,
            "lake_area_km2": lake_area,
            "inland_seas": inland_seas,
            "eroded_km3": eroded_km3,
            "major_rivers": major,
            "largest_lakes": lake_list,
            "climate_feedback": feedback,
        }),
    }
}
