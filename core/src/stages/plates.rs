//! Step 3 — plates. Continental plates are seeded inside sketched landmasses,
//! oceanic plates fill the oceans, and all plates grow together with a weighted
//! random flood fill so their edges are irregular. Each plate rotates about an
//! Euler pole; motion arrows from the sketch pin those poles.

use super::{Ctx, StepOutput};
use crate::edits::{stroke_coverage, PinKind};
use crate::fields::{Field, Fields};
use crate::graph;
use crate::noise::Noise;
use crate::rng::{hash_unit, stream, Rng};
use crate::vec3::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlateInfo {
    pub id: u32,
    pub major: bool,
    /// Seeded inside a landmass.
    pub continental: bool,
    pub seed_cell: u32,
    pub pole: [f64; 3],
    /// Angular speed, degrees per million years.
    pub omega_deg_myr: f64,
    pub area_mkm2: f64,
    pub continental_fraction: f64,
    pub growth_rate: f64,
    pub user_motion: bool,
}

impl PlateInfo {
    /// Surface velocity (km/Myr = mm/yr) at unit position `p`.
    pub fn velocity(&self, p: Vec3, radius_km: f64) -> Vec3 {
        let axis = Vec3::new(self.pole[0], self.pole[1], self.pole[2]);
        axis.cross(p) * (self.omega_deg_myr.to_radians() * radius_km)
    }
}

struct Seed {
    cell: usize,
    major: bool,
    continental: bool,
    rate: f64,
    /// Cost multiplier for a continental plate growing into ocean. Low values give
    /// passive margins (Atlantic-style), high values leave the coast as a boundary.
    ocean_reach: f64,
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let g = ctx.grid;
    let n = g.len();
    let pp = &ctx.params.planet;
    let tp = &ctx.params.plates;
    let r_km = pp.radius_km;
    let land = ctx.input.u8("land");
    let mut rng = Rng::new(pp.seed, stream::PLATES);

    ctx.progress(0.02, "Finding landmasses");
    let (_comp, mut masses) = graph::components(g, |i| land[i] == 1);
    masses.iter_mut().for_each(|m| m.sort_unstable());
    let mass_area = |cells: &Vec<u32>| cells.iter().map(|&c| g.area[c as usize]).sum::<f64>() * r_km * r_km;

    let mut seeds: Vec<Seed> = Vec::new();

    // User pins come first so their plate ids are stable.
    for pin in &ctx.edits.sketch.pins {
        let c = g.nearest(Vec3::from_lat_lon_deg(pin.lat, pin.lon), None);
        let continental = match pin.kind {
            PinKind::Continental => true,
            PinKind::Oceanic => false,
            PinKind::Auto => land[c] == 1,
        };
        seeds.push(Seed { cell: c, major: true, continental, rate: if continental { 1.0 } else { 1.1 }, ocean_reach: 1.5 });
    }

    // Continental plates: one or more per sizeable landmass.
    let cont_area = tp.continental_plate_area_mkm2.max(1.0) * 1e6;
    let mut order: Vec<usize> = (0..masses.len()).collect();
    order.sort_by(|&a, &b| masses[b].len().cmp(&masses[a].len()).then(a.cmp(&b)));
    for &m in &order {
        let cells = &masses[m];
        let area = mass_area(cells);
        if area < cont_area * 0.12 {
            continue;
        }
        let pinned = seeds.iter().filter(|s| s.continental && cells.binary_search(&(s.cell as u32)).is_ok()).count();
        let want = ((area / cont_area).round() as usize).max(1).saturating_sub(pinned);
        for _ in 0..want {
            let c = farthest_candidate(g, cells, &seeds, &mut rng, 300);
            let reach = if rng.f64() < 0.45 { rng.range(0.8, 1.2) } else { rng.range(2.5, 4.0) };
            seeds.push(Seed { cell: c, major: true, continental: true, rate: rng.range(0.85, 1.15), ocean_reach: reach });
        }
    }

    // Oceanic majors fill the rest of the major-plate budget.
    let ocean_cells: Vec<u32> = (0..n as u32).filter(|&i| land[i as usize] == 0).collect();
    let n_major = seeds.len();
    let n_ocean = (tp.major_plates as usize).saturating_sub(n_major).max(if ocean_cells.is_empty() { 0 } else { 2 });
    for _ in 0..n_ocean {
        if ocean_cells.is_empty() {
            break;
        }
        let c = farthest_candidate(g, &ocean_cells, &seeds, &mut rng, 400);
        seeds.push(Seed { cell: c, major: true, continental: false, rate: rng.range(0.95, 1.35), ocean_reach: 1.0 });
    }

    // Weighted random flood fill.
    ctx.progress(0.25, "Growing plates");
    let noise = Noise::new(pp.seed, stream::PLATE_GROWTH);
    let rough = tp.edge_roughness.clamp(0.0, 2.0);
    let cell_cost: Vec<f64> = (0..n)
        .map(|i| {
            // Multi-scale, high-contrast cost field: breaks up the hexagonal
            // balls that graph distance on a hex grid would otherwise produce.
            let large = noise.fbm(g.pos[i], 2.5, 3);
            let mid = noise.fbm(g.pos[i] + Vec3::new(5.2, 1.3, 7.7), 9.0, 4);
            let small = hash_unit(pp.seed, stream::PLATE_GROWTH, i as u64);
            (rough * (1.4 * large + 1.1 * mid) + 0.35 * rough * small).exp()
        })
        .collect();
    let sources: Vec<(u32, f64, u32)> = seeds.iter().enumerate().map(|(k, s)| (s.cell as u32, 0.0, k as u32)).collect();
    let res = graph::multi_source(g, &sources, f64::INFINITY, |_a, b, l, edge_len| {
        let s = &seeds[l as usize];
        let len = edge_len / g.spacing;
        let terrain = len * match (s.continental, land[b] == 1) {
            (true, true) => 0.55,
            (true, false) => s.ocean_reach,
            (false, true) => 1.9,
            (false, false) => 1.0,
        };
        Some(cell_cost[b] * terrain / s.rate)
    });
    let mut plate: Vec<u16> = res.label.iter().map(|&l| if l == u32::MAX { 0 } else { l as u16 }).collect();

    // Minor plates form along major-plate boundaries (like Cocos or Juan de Fuca)
    // and grow to a random size through the same noisy cost field. A fixed-budget
    // growth keeps them irregular instead of the circles a slower growth rate gives.
    ctx.progress(0.4, "Placing minor plates");
    let boundary: Vec<u32> = (0..n as u32)
        .filter(|&i| g.neighbors(i as usize).iter().any(|&j| plate[j as usize] != plate[i as usize]))
        .collect();
    let mean_cost = cell_cost.iter().sum::<f64>() / n as f64;
    let spacing_km = g.spacing * r_km;
    let first_minor = seeds.len();
    let mut budgets = Vec::new();
    if !boundary.is_empty() {
        for _ in 0..tp.minor_plates {
            let c = farthest_candidate(g, &boundary, &seeds, &mut rng, 8);
            seeds.push(Seed { cell: c, major: false, continental: land[c] == 1, rate: 1.0, ocean_reach: 1.0 });
            budgets.push(rng.range(600.0, 1800.0) / spacing_km * mean_cost);
        }
    }
    let minor_src: Vec<(u32, f64, u32)> = seeds[first_minor..].iter().enumerate().map(|(k, s)| (s.cell as u32, 0.0, k as u32)).collect();
    // Growing away from the boundary costs more, so minor plates are elongated
    // along it rather than round.
    let is_b: Vec<bool> = { let mut v = vec![false; n]; for &c in &boundary { v[c as usize] = true; } v };
    let d_bound = graph::distance_km(g, r_km, 3000.0, |i| is_b[i], |_| true);
    let minor = graph::multi_source(g, &minor_src, 1.0, |_a, b, l, edge_len| {
        let len = edge_len / g.spacing;
        let away = (d_bound[b] / 250.0).min(12.0);
        Some(len * cell_cost[b] * (1.0 + away * away) / budgets[l as usize])
    });
    for i in 0..n {
        if minor.label[i] != u32::MAX {
            plate[i] = (first_minor as u32 + minor.label[i]) as u16;
        }
    }

    // Plate paint overrides.
    let mut scratch = Vec::new();
    for s in &ctx.edits.overrides.plates {
        let id = s.value.round();
        if id < 0.0 || id as usize >= seeds.len() {
            continue;
        }
        for (c, w) in stroke_coverage(g, s, r_km, &mut scratch) {
            if w >= 0.5 {
                plate[c as usize] = id as u16;
            }
        }
    }

    // Crust: continental under land and the shelf around it.
    ctx.progress(0.6, "Classifying crust");
    // Shelves belong to the plate that carries the land, so a coast that is also
    // a plate boundary keeps oceanic crust on the far side (Andes-type margins).
    let land_src: Vec<(u32, f64, u32)> = (0..n).filter(|&i| land[i] == 1).map(|i| (i as u32, 0.0, 0)).collect();
    let shelf = graph::multi_source(g, &land_src, tp.shelf_width_km.max(0.0), |a, b, _, edge_len| {
        (plate[a] == plate[b]).then_some(edge_len * r_km)
    })
    .cost;
    let crust: Vec<u8> = (0..n).map(|i| (shelf[i].is_finite()) as u8).collect();

    // Plate statistics and motion.
    ctx.progress(0.75, "Setting plate motion");
    let np = seeds.len();
    let mut area = vec![0.0f64; np];
    let mut cont_area_p = vec![0.0f64; np];
    for i in 0..n {
        let p = plate[i] as usize;
        area[p] += g.area[i];
        if crust[i] == 1 {
            cont_area_p[p] += g.area[i];
        }
    }
    let mut mrng = Rng::new(pp.seed, stream::PLATE_MOTION);
    let mut plates: Vec<PlateInfo> = seeds
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let cf = if area[k] > 0.0 { cont_area_p[k] / area[k] } else { 0.0 };
            let pole = mrng.unit_vector();
            let omega = if !s.major {
                mrng.range(0.3, 0.9)
            } else if cf > 0.5 {
                mrng.range(0.15, 0.45)
            } else {
                mrng.range(0.4, 1.0)
            } * tp.speed_scale;
            PlateInfo {
                id: k as u32,
                major: s.major,
                continental: s.continental,
                seed_cell: s.cell as u32,
                pole: [pole.x, pole.y, pole.z],
                omega_deg_myr: omega,
                area_mkm2: area[k] * r_km * r_km / 1e6,
                continental_fraction: cf,
                growth_rate: s.rate,
                user_motion: false,
            }
        })
        .collect();

    // Motion arrows: velocity v = ω × p, so the pole for a tangent direction d is p × d.
    let mut arrow_sum: Vec<(Vec3, f64, usize)> = vec![(Vec3::ZERO, 0.0, 0); np];
    for a in &ctx.edits.sketch.arrows {
        let p = Vec3::from_lat_lon_deg(a.lat, a.lon);
        let c = g.nearest(p, None);
        let (east, north) = p.east_north();
        let b = a.bearing_deg.to_radians();
        let d = (north * b.cos() + east * b.sin()).normalized();
        let axis = p.cross(d).normalized();
        let omega_deg = (a.speed_mm_yr / r_km).to_degrees();
        let e = &mut arrow_sum[plate[c] as usize];
        e.0 += axis * omega_deg;
        e.1 += omega_deg;
        e.2 += 1;
    }
    for (k, (ax, w, cnt)) in arrow_sum.into_iter().enumerate() {
        if cnt > 0 && ax.len() > 1e-12 {
            let axis = ax.normalized();
            plates[k].pole = [axis.x, axis.y, axis.z];
            plates[k].omega_deg_myr = ax.len().max(w / cnt as f64 * 0.5);
            plates[k].user_motion = true;
        }
    }

    let mut vel_e = vec![0.0f32; n];
    let mut vel_n = vec![0.0f32; n];
    for i in 0..n {
        let v = plates[plate[i] as usize].velocity(g.pos[i], r_km);
        let (e, nn) = g.pos[i].east_north();
        vel_e[i] = v.dot(e) as f32;
        vel_n[i] = v.dot(nn) as f32;
    }

    let n_cont = plates.iter().filter(|p| p.continental && p.major).count();
    let mut f = Fields::default();
    f.put("plate", Field::U16(plate));
    f.put("crust", Field::U8(crust));
    f.put("vel_e", Field::F32(vel_e));
    f.put("vel_n", Field::F32(vel_n));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "plates": plates,
            "landmasses": masses.len(),
            "continental_plates": n_cont,
            "plate_count": np,
        }),
    }
}

/// Best-of-k sampling: the candidate farthest from all existing seeds.
fn farthest_candidate(g: &crate::grid::Grid, cells: &[u32], seeds: &[Seed], rng: &mut Rng, k: usize) -> usize {
    let mut best = cells[rng.below(cells.len())] as usize;
    let mut best_d = -1.0;
    for _ in 0..k {
        let c = cells[rng.below(cells.len())] as usize;
        let d = seeds.iter().map(|s| g.pos[s.cell].angle_to(g.pos[c])).fold(f64::INFINITY, f64::min);
        if d > best_d {
            best_d = d;
            best = c;
        }
    }
    best
}

/// Parse the `plates` array from the plates step metadata.
pub fn plates_from_meta(meta: &serde_json::Value) -> Vec<PlateInfo> {
    serde_json::from_value(meta.clone()).unwrap_or_default()
}
