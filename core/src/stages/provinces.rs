//! Step 10 — provinces, wasteland, lakes and sea zones.
//!
//! - Land: each state is cut into provinces. Seeds are placed by weighted
//!   Poisson-disc sampling whose radius follows habitability (small provinces
//!   on fertile land, large ones in the steppe), grown with the barrier-aware
//!   Dijkstra search at a lower barrier weight, and relaxed by Lloyd passes.
//! - Peaks, ice caps and deep desert (in large patches) become impassable
//!   wasteland provinces, still inside their state.
//! - Sea is split by depth band (coastal, shelf, open ocean) into sea zones of
//!   band-specific size. Large lakes become lake provinces; small lakes join
//!   the land province around them.
//! - Province paint moves cells to the province under the stroke's start.
//! - An imported provinces.png replaces all of this: each cell takes its
//!   province by majority pixel vote, and provinces join the state they overlap most.
//!
//! The step also lists strait crossings between provinces on different
//! landmasses (adjacencies.csv) and every border between two provinces
//! (province_adjacency.csv): its type (land, river, impassable, coast, lake,
//! sea or strait), its length and how hard it is to cross.

use super::biomes::{terrain, TERRAIN};
use super::habitability::role;
use super::hydrology::water;
use super::partition::{self, NONE};
use super::{Ctx, StepOutput};
use crate::edits::{stroke_coverage, Tool};
use crate::fields::{Field, Fields};
use crate::graph;
use crate::names::{self, Lang};
use crate::rng::{hash_unit, stream, Rng};
use crate::vec3::Vec3;
use std::collections::{BTreeMap, HashSet};

pub mod kind {
    pub const LAND: u8 = 0;
    pub const WASTELAND: u8 = 1;
    pub const LAKE: u8 = 2;
    pub const SEA: u8 = 3;
}
pub const KIND_NAMES: [&str; 4] = ["land", "wasteland", "lake", "sea"];
pub const BAND_NAMES: [&str; 3] = ["coastal", "shelf", "open"];

// Partition groups: land provinces grow inside their state (group = state id);
// the other kinds use these offsets.
const G_WASTE: u32 = 1 << 20;
const G_LAKE: u32 = 2 << 20;
const G_SEA: u32 = 3 << 20;

fn group_kind(gr: u32) -> u8 {
    match gr {
        NONE => kind::LAND, // small lakes, absorbed into land provinces
        x if x >= G_SEA => kind::SEA,
        x if x >= G_LAKE => kind::LAKE,
        x if x >= G_WASTE => kind::WASTELAND,
        _ => kind::LAND,
    }
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let g = ctx.grid;
    let n = g.len();
    let pp = &ctx.params.provinces;
    let seed = ctx.params.planet.seed;
    let r_km = ctx.params.planet.radius_km;
    let r2 = r_km * r_km;
    let wat = ctx.input.u8("water");
    let ter = ctx.input.u8("terrain");
    let elev = ctx.input.f32("elevation");
    let hab = ctx.input.f32("habitability");
    let bar = ctx.input.f32("barrier");
    let state = ctx.input.u16("state");
    let river_role = ctx.input.u8("river_role");
    let region = ctx.input.u16("region");
    let cont = ctx.input.u8("continent");
    let area: Vec<f64> = g.area.iter().map(|a| a * r2).collect();
    let land = |i: usize| wat[i] == water::LAND;
    let ocean = |i: usize| wat[i] == water::OCEAN;

    // ------------------------------------------------------------ cell groups
    ctx.progress(0.05, "Classifying land, wasteland, lakes and sea");
    let (lk_comp, lk_list) = graph::components(g, |i| wat[i] == water::LAKE);
    let lk_area: Vec<f64> = lk_list.iter().map(|c| c.iter().map(|&i| area[i as usize]).sum()).collect();
    let waste_cand = |i: usize| {
        land(i)
            && (ter[i] == terrain::GLACIER
                || (ter[i] == terrain::MOUNTAINS && elev[i] as f64 > pp.wasteland_elev_m)
                || (hab[i] as f64) < pp.wasteland_habitability)
    };
    let (wc, wl) = graph::components(g, waste_cand);
    let waste_ok: Vec<bool> = wl.iter().map(|c| c.iter().map(|&i| area[i as usize]).sum::<f64>() >= 3.0 * pp.min_province_area_km2.max(1.0)).collect();
    let d_land = graph::distance_km(g, r_km, pp.coastal_band_km, |i| ocean(i) && g.neighbors(i).iter().any(|&j| !ocean(j as usize)), ocean);
    let band: Vec<u8> = (0..n)
        .map(|i| {
            if !ocean(i) || d_land[i].is_finite() {
                0
            } else if elev[i] as f64 > -pp.shelf_depth_m {
                1
            } else {
                2
            }
        })
        .collect();
    let group: Vec<u32> = (0..n)
        .map(|i| {
            if ocean(i) {
                G_SEA + band[i] as u32
            } else if wat[i] == water::LAKE {
                if lk_area[lk_comp[i] as usize] >= pp.lake_province_km2 {
                    G_LAKE
                } else {
                    NONE
                }
            } else if waste_cand(i) && waste_ok[wc[i] as usize] {
                G_WASTE + state[i] as u32
            } else {
                state[i] as u32
            }
        })
        .collect();

    // ------------------------------------------------------------ provinces
    let mut import_info = serde_json::Value::Null;
    let mut defs: Option<Vec<crate::province_import::Def>> = None;
    let mut painted = 0usize;
    let (label, count) = 'part: {
        if let Some(imp) = &ctx.edits.imports.provinces {
            ctx.progress(0.1, "Reading imported provinces.png");
            match crate::province_import::rasterize(imp, g) {
                Ok(v) => {
                    let changed = crate::province_import::content_hash(&imp.png, imp.csv.as_deref()).ok().is_some_and(|h| h != imp.content_hash);
                    import_info = serde_json::json!({
                        "png": imp.png, "csv": imp.csv, "provinces": v.defs.len(),
                        "warnings": v.report.warnings, "changed_since_import": changed,
                    });
                    let count = v.defs.len();
                    defs = Some(v.defs);
                    break 'part (v.cell, count);
                }
                Err(e) => import_info = serde_json::json!({ "png": imp.png, "error": e }),
            }
        }
        ctx.progress(0.1, "Seeding provinces");
        let a_land = |h: f64| pp.sparse_province_area_km2 * (pp.province_area_km2 / pp.sparse_province_area_km2.max(1.0)).powf(h.clamp(0.0, 1.0));
        let target = |i: usize| -> f64 {
            let a = match group_kind(group[i]) {
                kind::LAND => a_land(hab[i] as f64),
                kind::WASTELAND => pp.wasteland_area_km2,
                kind::LAKE => pp.coastal_sea_km2,
                _ => [pp.coastal_sea_km2, pp.shelf_sea_km2, pp.open_sea_km2][band[i] as usize],
            };
            a.max(500.0)
        };
        let weight = |i: usize| 1.0 / target(i);
        let order = partition::race_order((0..n).filter(|&i| group[i] != NONE), weight, seed, stream::PROVINCES);
        let mut seeds = partition::poisson(g, &order, &group, |i| (target(i) / 1.2).sqrt() / r_km);
        partition::seed_every_component(g, &group, &order, &mut seeds);
        let bw = pp.barrier_weight.max(0.0);
        let cost = |a: usize, b: usize| if group[a] >= G_LAKE { 1.0 } else { 1.0 + bw * 0.5 * (bar[a] + bar[b]) as f64 };
        ctx.progress(0.25, "Growing provinces");
        let mut label = partition::grow(g, r_km, &group, &seeds, None, cost);
        for k in 0..pp.lloyd_passes.min(8) {
            ctx.progress(0.25 + 0.08 * k as f32, "Relaxing province shapes");
            partition::recentre(g, &label, &mut seeds, weight);
            label = partition::grow(g, r_km, &group, &seeds, None, cost);
        }
        // Province paint.
        let mut scratch = Vec::new();
        for s in &ctx.edits.overrides.provinces {
            if s.tool != Tool::ProvincePaint || s.points.is_empty() {
                continue;
            }
            let anchor = g.nearest(Vec3::from_lat_lon_deg(s.points[0][0], s.points[0][1]), None);
            let (l, gl) = (label[anchor], group[anchor]);
            if l == NONE {
                continue;
            }
            for (c, w) in stroke_coverage(g, s, r_km, &mut scratch) {
                let c = c as usize;
                if w >= 0.5 && group[c] == gl && label[c] != l {
                    label[c] = l;
                    painted += 1;
                }
            }
        }
        ctx.progress(0.5, "Merging small provinces");
        partition::absorb_fragments(g, &mut label, &area, |a, b| group[a] == group[b]);
        let min: Vec<f64> = seeds
            .iter()
            .map(|&c| match group_kind(group[c as usize]) {
                kind::LAND | kind::WASTELAND => pp.min_province_area_km2,
                kind::LAKE => 0.5 * pp.lake_province_km2,
                _ => 0.2 * pp.coastal_sea_km2,
            })
            .collect();
        let sea_cell = |i: usize| group[i] != NONE && group[i] >= G_SEA;
        let count = partition::merge_small(g, &mut label, seeds.len(), &area, &min, |a, b| group[a] == group[b] || (sea_cell(a) && sea_cell(b)));
        (label, count)
    };
    let mut label = label;
    let from_import = defs.is_some();

    // Small lakes join the land province around them.
    for _ in 0..64 {
        let mut changed = false;
        for i in 0..n {
            if label[i] != NONE {
                continue;
            }
            let pick = g
                .neighbors(i)
                .iter()
                .map(|&j| j as usize)
                .filter(|&j| label[j] != NONE && !ocean(j) && group[j] != NONE && group[j] < G_LAKE)
                .chain(g.neighbors(i).iter().map(|&j| j as usize).filter(|&j| label[j] != NONE))
                .next();
            if let Some(j) = pick {
                label[i] = label[j];
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // ------------------------------------------------------------ aggregate
    ctx.progress(0.6, "Describing provinces");
    let cell_kind = |i: usize| -> u8 {
        if from_import {
            if ocean(i) {
                kind::SEA
            } else if wat[i] == water::LAKE {
                kind::LAKE
            } else if waste_cand(i) {
                kind::WASTELAND
            } else {
                kind::LAND
            }
        } else {
            group_kind(group[i])
        }
    };
    // Nearest continent for every cell (names for seas and lakes).
    let cont_src: Vec<(u32, f64, u32)> = (0..n).filter(|&i| cont[i] > 0).map(|i| (i as u32, 0.0, cont[i] as u32 - 1)).collect();
    let near_cont = if cont_src.is_empty() {
        vec![0u32; n]
    } else {
        graph::multi_source(g, &cont_src, f64::INFINITY, |a, b, _| Some(g.pos[a].angle_to(g.pos[b]))).label
    };

    #[derive(Clone)]
    struct Agg {
        area: f64,
        hab: f64,
        pos: Vec3,
        kinds: [f64; 4],
        bands: [f64; 3],
        states: BTreeMap<u16, f64>,
        regions: BTreeMap<u16, f64>,
        terrain: [f64; 18],
        cont: BTreeMap<u32, f64>,
        coastal: bool,
    }
    let mut agg = vec![
        Agg {
            area: 0.0,
            hab: 0.0,
            pos: Vec3::new(0.0, 0.0, 0.0),
            kinds: [0.0; 4],
            bands: [0.0; 3],
            states: BTreeMap::new(),
            regions: BTreeMap::new(),
            terrain: [0.0; 18],
            cont: BTreeMap::new(),
            coastal: false,
        };
        count
    ];
    for i in 0..n {
        let l = label[i];
        if l == NONE {
            continue;
        }
        let a = &mut agg[l as usize];
        let ar = area[i];
        a.area += ar;
        a.hab += ar * hab[i] as f64;
        a.pos += g.pos[i] * ar;
        let k = cell_kind(i);
        a.kinds[k as usize] += ar;
        if ocean(i) {
            a.bands[band[i] as usize] += ar;
        }
        if !ocean(i) && wat[i] != water::LAKE {
            *a.states.entry(state[i]).or_insert(0.0) += ar;
            *a.regions.entry(region[i]).or_insert(0.0) += ar;
            a.terrain[ter[i] as usize % 18] += ar;
            if g.neighbors(i).iter().any(|&j| ocean(j as usize)) {
                a.coastal = true;
            }
        }
        *a.cont.entry(near_cont[i]).or_insert(0.0) += ar;
    }
    let argmax_f = |v: &[f64]| (0..v.len()).max_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap().then(b.cmp(&a))).unwrap_or(0);
    let argmax_m = |m: &BTreeMap<u16, f64>| m.iter().filter(|(&k, _)| k > 0).max_by(|a, b| a.1.partial_cmp(b.1).unwrap().then(b.0.cmp(a.0))).map_or(0, |(&k, _)| k);
    struct Prov {
        kind: u8,
        band: u8,
        state: u16,
        region: u16,
        cont: u32,
        center: Vec3,
        terrain: u8,
    }
    let provs: Vec<Prov> = agg
        .iter()
        .map(|a| {
            let k = argmax_f(&a.kinds) as u8;
            let political = k == kind::LAND || k == kind::WASTELAND;
            Prov {
                kind: k,
                band: argmax_f(&a.bands) as u8,
                state: if political { argmax_m(&a.states) } else { 0 },
                region: if political { argmax_m(&a.regions) } else { 0 },
                cont: a.cont.iter().max_by(|x, y| x.1.partial_cmp(y.1).unwrap().then(y.0.cmp(x.0))).map_or(0, |(&k, _)| k),
                center: a.pos.normalized(),
                terrain: if k == kind::SEA { terrain::OCEAN } else if k == kind::LAKE { terrain::LAKE } else { argmax_f(&a.terrain) as u8 },
            }
        })
        .collect();

    // ------------------------------------------------------------ ids, colours, names
    let mut ids = vec![0u32; count];
    let mut colors = vec![[0u8; 3]; count];
    let mut pnames = vec![String::new(); count];
    let mut order: Vec<usize> = (0..count).collect();
    if let Some(defs) = &defs {
        for (k, d) in defs.iter().enumerate() {
            ids[k] = d.id;
            colors[k] = d.rgb;
            pnames[k] = d.name.clone();
        }
        order.sort_by_key(|&k| ids[k]);
    } else {
        let lat = |k: usize| provs[k].center.lat_lon().0;
        let lon = |k: usize| provs[k].center.lat_lon().1;
        order.sort_by(|&a, &b| {
            let (pa, pb) = (&provs[a], &provs[b]);
            pa.kind
                .cmp(&pb.kind)
                .then(pa.state.cmp(&pb.state))
                .then(pa.band.cmp(&pb.band))
                .then(lat(b).partial_cmp(&lat(a)).unwrap())
                .then(lon(a).partial_cmp(&lon(b)).unwrap())
                .then(a.cmp(&b))
        });
        let mut used: HashSet<[u8; 3]> = HashSet::new();
        for (k, &p) in order.iter().enumerate() {
            let id = k as u32 + 1;
            ids[p] = id;
            let mut t = 0u64;
            colors[p] = loop {
                let h = (hash_unit(seed, stream::PROVINCE_COLORS, id as u64 + (t << 32)) * 16_777_216.0) as u32;
                let c = [(h >> 16) as u8, (h >> 8) as u8, h as u8];
                t += 1;
                if c != [0, 0, 0] && c != [255, 255, 255] && used.insert(c) {
                    break c;
                }
            };
        }
    }
    // Capital province of each state: the land province holding the state's
    // capital cell, else its most habitable land province (or wasteland).
    let upstream = ctx.input.meta("table");
    let mut states_json: Vec<serde_json::Value> = upstream["states"].as_array().cloned().unwrap_or_default();
    let mut capital_of: BTreeMap<u16, usize> = BTreeMap::new();
    for s in &states_json {
        let sid = s["id"].as_u64().unwrap_or(0) as u16;
        let by_cell = s["capital_cell"].as_u64().map(|c| c as usize).filter(|&c| c < n && label[c] != NONE).map(|c| label[c] as usize).filter(|&p| provs[p].state == sid && provs[p].kind == kind::LAND);
        let most_habitable = |land_only: bool| {
            order.iter().copied().filter(|&p| provs[p].state == sid && (!land_only || provs[p].kind == kind::LAND)).max_by(|&a, &b| {
                (agg[a].hab / agg[a].area.max(1.0)).partial_cmp(&(agg[b].hab / agg[b].area.max(1.0))).unwrap().then(b.cmp(&a))
            })
        };
        // States of only ice or desert fall back to their most habitable wasteland.
        let cap = by_cell.or_else(|| most_habitable(true)).or_else(|| most_habitable(false));
        if let Some(p) = cap {
            capital_of.insert(sid, p);
        }
    }

    // Generated names (kept from definition.csv on import): a state's capital
    // takes the state's name; other land provinces speak their region's dialect,
    // seas and lakes the nearest continent's language. No name is used twice.
    let n_lang = provs.iter().map(|p| p.cont + 1).max().unwrap_or(1);
    let langs: Vec<Lang> = (0..n_lang).map(|c| names::continent_lang(seed, c)).collect();
    let mut dialects: BTreeMap<u16, Lang> = BTreeMap::new();
    // The continent each region belongs to (0-based), as the states step used for its dialect.
    let region_cont: BTreeMap<u16, u32> = upstream["regions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r["id"].as_u64()? as u16, (r["continent"].as_u64()? as u32).checked_sub(1)?)))
        .collect();
    let mut rng = Rng::new(seed, stream::NAMES + 1);
    let mut used_names: HashSet<String> = HashSet::new();
    for key in ["continents", "regions", "states"] {
        for t in upstream[key].as_array().into_iter().flatten() {
            if let Some(name) = t["name"].as_str() {
                used_names.insert(name.to_string());
            }
        }
    }
    let state_name: BTreeMap<u16, String> =
        states_json.iter().filter_map(|s| Some((s["id"].as_u64()? as u16, s["name"].as_str()?.to_string()))).collect();
    for (&sid, &p) in &capital_of {
        if pnames[p].is_empty() {
            if let Some(name) = state_name.get(&sid) {
                pnames[p] = name.clone();
            }
        }
    }
    for &p in &order {
        if !pnames[p].is_empty() {
            continue;
        }
        let pr = &provs[p];
        let cont = pr.cont.min(n_lang - 1);
        let lang = if pr.region > 0 && (pr.kind == kind::LAND || pr.kind == kind::WASTELAND) {
            let rc = region_cont.get(&pr.region).copied().unwrap_or(cont);
            dialects.entry(pr.region).or_insert_with(|| names::region_lang(seed, rc, pr.region))
        } else {
            &langs[cont as usize]
        };
        let w = lang.unique(&mut rng, &mut used_names);
        pnames[p] = match pr.kind {
            kind::SEA => match pr.band {
                0 => ["{} Bay", "Gulf of {}", "{} Sound", "{} Coast"][rng.below(4)].replace("{}", &w),
                1 => format!("{w} Sea"),
                _ => format!("{w} Deep"),
            },
            kind::LAKE => format!("Lake {w}"),
            kind::WASTELAND => match pr.terrain {
                terrain::GLACIER => format!("{w} Ice"),
                terrain::DESERT | terrain::DRYLANDS => format!("{w} Desert"),
                _ => format!("{w} Peaks"),
            },
            _ => w,
        };
    }

    // ------------------------------------------------------------ fields
    let mut f_prov = vec![0u32; n];
    let mut f_kind = vec![kind::SEA; n];
    let mut f_state = vec![0u16; n];
    for i in 0..n {
        let l = label[i];
        if l == NONE {
            continue;
        }
        let p = &provs[l as usize];
        f_prov[i] = ids[l as usize];
        f_kind[i] = p.kind;
        f_state[i] = p.state;
    }

    // Provinces whose cells are not connected on the grid (islands, imports).
    let (_, pieces) = partition::group_components(g, &label);
    let mut piece_count = vec![0u32; count];
    for c in &pieces {
        piece_count[label[c[0] as usize] as usize] += 1;
    }
    let split_on_grid = piece_count.iter().filter(|&&k| k > 1).count();

    // ------------------------------------------------------------ adjacency
    ctx.progress(0.75, "Building the adjacency graph");
    // Per border: shared cell edges, edges along a border river, summed barrier.
    #[derive(Default)]
    struct Border {
        edges: u32,
        river: u32,
        barrier: f64,
    }
    let mut nb_set: BTreeMap<(u32, u32), Border> = BTreeMap::new();
    for i in 0..n {
        let a = label[i];
        if a == NONE {
            continue;
        }
        for &j in g.neighbors(i) {
            let j = j as usize;
            let b = label[j];
            if b != NONE && a != b {
                let e = nb_set.entry((a.min(b), a.max(b))).or_default();
                e.edges += 1;
                e.barrier += 0.5 * (bar[i] + bar[j]) as f64;
                if river_role[i] == role::BORDER || river_role[j] == role::BORDER {
                    e.river += 1;
                }
            }
        }
    }
    let mut neighbors: Vec<Vec<u32>> = vec![Vec::new(); count];
    for &(a, b) in nb_set.keys() {
        neighbors[a as usize].push(ids[b as usize]);
        neighbors[b as usize].push(ids[a as usize]);
    }
    for v in neighbors.iter_mut() {
        v.sort_unstable();
    }

    // Strait crossings: from every coastal land cell, search outward over the
    // sea; where searches from different landmasses meet within the maximum
    // strait width, the two provinces get a crossing.
    ctx.progress(0.85, "Finding strait crossings");
    let (lm, _) = graph::components(g, land);
    let max_km = pp.max_strait_km.max(0.0);
    let passable_land = |i: usize| land(i) && label[i] != NONE && provs[label[i] as usize].kind == kind::LAND;
    let coast: Vec<(u32, f64, u32)> =
        (0..n).filter(|&i| passable_land(i) && g.neighbors(i).iter().any(|&j| ocean(j as usize))).map(|i| (i as u32, 0.0, i as u32)).collect();
    let reach = graph::multi_source(g, &coast, max_km, |a, b, _| if ocean(b) { Some(g.pos[a].angle_to(g.pos[b]) * r_km) } else { None });
    let mut straits: BTreeMap<(u32, u32), (f64, usize, usize, usize)> = BTreeMap::new();
    let consider = |from: usize, to: usize, via: usize, km: f64, straits: &mut BTreeMap<(u32, u32), (f64, usize, usize, usize)>| {
        if km > max_km || lm[from] == lm[to] {
            return;
        }
        let (pa, pb) = (label[from], label[to]);
        if pa == NONE || pb == NONE || pa == pb {
            return;
        }
        let key = (pa.min(pb), pa.max(pb));
        let (f, t) = if pa < pb { (from, to) } else { (to, from) };
        let e = straits.entry(key).or_insert((f64::INFINITY, 0, 0, 0));
        if km < e.0 {
            *e = (km, f, t, via);
        }
    };
    for x in 0..n {
        if !ocean(x) || reach.label[x] == NONE {
            continue;
        }
        let sx = reach.label[x] as usize;
        for &y in g.neighbors(x) {
            let y = y as usize;
            let step = g.pos[x].angle_to(g.pos[y]) * r_km;
            if ocean(y) && reach.label[y] != NONE && x < y {
                let sy = reach.label[y] as usize;
                consider(sx, sy, x, reach.cost[x] + step + reach.cost[y], &mut straits);
            } else if passable_land(y) {
                consider(sx, y, x, reach.cost[x] + step, &mut straits);
            }
        }
    }
    let deg = |v: Vec3| {
        let (la, lo) = v.lat_lon();
        [(la.to_degrees() * 1000.0).round() / 1000.0, (lo.to_degrees() * 1000.0).round() / 1000.0]
    };
    let mut straits_json = Vec::new();
    for ((a, b), (km, f, t, via)) in &straits {
        let through = label[*via];
        straits_json.push(serde_json::json!({
            "from": ids[*a as usize], "to": ids[*b as usize], "type": "sea",
            "through": if through == NONE { 0 } else { ids[through as usize] },
            "from_ll": deg(g.pos[*f]), "to_ll": deg(g.pos[*t]), "km": km.round(),
        }));
    }

    // Every border, typed. Each shared cell edge was counted from both sides;
    // a hexagon edge is spacing / √3 long.
    let edge_km = g.spacing * r_km / 3f64.sqrt();
    let political = |k: u8| k == kind::LAND || k == kind::WASTELAND;
    let mut adjacency_json = Vec::with_capacity(nb_set.len() + straits_json.len());
    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for (&(a, b), e) in &nb_set {
        let (ka, kb) = (provs[a as usize].kind, provs[b as usize].kind);
        let ty = if political(ka) && political(kb) {
            if ka == kind::WASTELAND || kb == kind::WASTELAND {
                "impassable"
            } else if e.river * 5 >= e.edges * 2 {
                "river"
            } else {
                "land"
            }
        } else if political(ka) || political(kb) {
            if ka == kind::LAKE || kb == kind::LAKE { "lake" } else { "coast" }
        } else if ka == kind::LAKE || kb == kind::LAKE {
            "lake"
        } else {
            "sea"
        };
        *by_type.entry(ty).or_insert(0) += 1;
        let (fa, fb) = (ids[a as usize].min(ids[b as usize]), ids[a as usize].max(ids[b as usize]));
        adjacency_json.push(serde_json::json!({
            "from": fa, "to": fb, "type": ty,
            "border_km": (e.edges as f64 / 2.0 * edge_km).round(),
            "barrier": (e.barrier / e.edges as f64 * 100.0).round() / 100.0,
        }));
    }
    for s in &straits_json {
        *by_type.entry("strait").or_insert(0) += 1;
        adjacency_json.push(serde_json::json!({ "from": s["from"], "to": s["to"], "type": "strait", "border_km": 0, "barrier": 0, "crossing_km": s["km"] }));
    }
    adjacency_json.sort_by_key(|a| (a["from"].as_u64().unwrap_or(0), a["to"].as_u64().unwrap_or(0)));

    // ------------------------------------------------------------ tables
    ctx.progress(0.92, "Writing tables");
    let mut provinces_json = Vec::with_capacity(count);
    let mut by_kind = [0usize; 4];
    let mut land_area = Vec::new();
    for &p in &order {
        let (a, pr) = (&agg[p], &provs[p]);
        by_kind[pr.kind as usize] += 1;
        if pr.kind == kind::LAND {
            land_area.push(a.area);
        }
        let mut j = serde_json::json!({
            "id": ids[p],
            "name": pnames[p],
            "kind": KIND_NAMES[pr.kind as usize],
            "color": colors[p],
            "state": pr.state,
            "region": pr.region,
            "continent": if pr.kind == kind::LAND || pr.kind == kind::WASTELAND { pr.cont + 1 } else { 0 },
            "area_km2": a.area.round(),
            "habitability": (a.hab / a.area.max(1.0) * 1000.0).round() / 1000.0,
            "terrain": TERRAIN[pr.terrain as usize].0,
            "coastal": a.coastal,
            "center": deg(pr.center),
            "neighbors": neighbors[p],
        });
        if pr.kind == kind::SEA {
            j["band"] = serde_json::json!(BAND_NAMES[pr.band as usize]);
        }
        provinces_json.push(j);
    }
    // States: add their provinces and capital province.
    let mut state_provs: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for &p in &order {
        if provs[p].state > 0 {
            state_provs.entry(provs[p].state).or_default().push(ids[p]);
        }
    }
    for s in states_json.iter_mut() {
        let sid = s["id"].as_u64().unwrap_or(0) as u16;
        let list = state_provs.get(&sid).cloned().unwrap_or_default();
        s["capital_province"] = serde_json::json!(capital_of.get(&sid).map_or(0, |&p| ids[p]));
        s["province_count"] = serde_json::json!(list.len());
        s["provinces"] = serde_json::json!(list);
    }
    land_area.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = land_area.get(land_area.len() / 2).copied().unwrap_or(0.0);
    let mean = land_area.iter().sum::<f64>() / land_area.len().max(1) as f64;

    let mut f = Fields::default();
    f.put("province", Field::U32(f_prov));
    f.put("province_kind", Field::U8(f_kind));
    f.put("state", Field::U16(f_state));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "provinces": count,
            "land": by_kind[kind::LAND as usize],
            "wasteland": by_kind[kind::WASTELAND as usize],
            "lakes": by_kind[kind::LAKE as usize],
            "sea": by_kind[kind::SEA as usize],
            "mean_land_km2": mean.round(),
            "median_land_km2": median.round(),
            "straits": straits_json.len(),
            "split_on_grid": split_on_grid,
            "borders": by_type,
            "painted_cells": painted,
            "import": import_info,
            "table": {
                "provinces": provinces_json,
                "states": states_json,
                "regions": upstream["regions"].clone(),
                "continents": upstream["continents"].clone(),
                "adjacencies": straits_json,
                "adjacency": adjacency_json,
            },
        }),
    }
}
