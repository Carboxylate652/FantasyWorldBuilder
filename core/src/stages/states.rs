//! Step 9 — states, regions and continents.
//!
//! 1. Seeds are placed by Poisson-disc sampling weighted by habitability. The
//!    disc radius shrinks on fertile land (more, smaller states there) and is
//!    tuned by bisection so the count matches land area / target state area.
//!    Every landmass large enough for a state of its own gets a seed.
//! 2. All seeds grow together (multi-source Dijkstra). A step costs its length
//!    times 1 + barrier, so neighbouring states meet on ridges, border rivers and
//!    deserts, like watershed segmentation. Water can be crossed at a high cost,
//!    so small islands join the state across the shortest strait. Travel along a
//!    backbone river (Nile) is cheap, so its valley stays in one state.
//! 3. State paint strokes move cells to the state under the stroke's start.
//! 4. Clean-up: disconnected fragments join the neighbour they touch most, and
//!    states below the minimum size merge into their longest-border neighbour.
//!    Map editor merges then join the state under each drag's start to the
//!    one under its end (any distance apart, e.g. an island and the mainland),
//!    and renames replace generated names.
//! 5. States are grouped into regions (a few neighbouring states each, by
//!    farthest-point seeding and growth over the state graph) and regions into
//!    continents (from landmasses).

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

pub fn run(ctx: &Ctx) -> StepOutput {
    let g = ctx.grid;
    let n = g.len();
    let sp = &ctx.params.states;
    let seed = ctx.params.planet.seed;
    let r_km = ctx.params.planet.radius_km;
    let r2 = r_km * r_km;
    let wat = ctx.input.u8("water");
    let hab = ctx.input.f32("habitability");
    let bar = ctx.input.f32("barrier");
    let river_role = ctx.input.u8("river_role");
    let recv = ctx.input.i32("receiver");
    let land = |i: usize| wat[i] == water::LAND;
    let area: Vec<f64> = g.area.iter().map(|a| a * r2).collect();

    ctx.progress(0.05, "Finding landmasses");
    let (lm, lm_cells) = graph::components(g, land);
    let lm_area: Vec<f64> = lm_cells.iter().map(|c| c.iter().map(|&i| area[i as usize]).sum()).collect();
    let land_area: f64 = lm_area.iter().sum();
    if lm_cells.is_empty() {
        return empty_output(n);
    }
    let largest = (0..lm_area.len()).max_by(|&a, &b| lm_area[a].partial_cmp(&lm_area[b]).unwrap().then(b.cmp(&a))).unwrap();

    // ------------------------------------------------------------ seeds
    ctx.progress(0.1, "Placing state seeds");
    let eligible = |i: usize| land(i) && (lm_area[lm[i] as usize] >= sp.island_state_km2 || lm[i] as usize == largest);
    let order = partition::race_order((0..n).filter(|&i| eligible(i)), |i| 0.05 + hab[i] as f64, seed, stream::STATES);
    let group: Vec<u32> = (0..n).map(|i| if eligible(i) { lm[i] } else { NONE }).collect();
    let target = (land_area / sp.state_area_km2.max(1000.0)).round().max(1.0) as usize;
    let dens = sp.habitability_density.clamp(0.0, 1.0);
    // Disc radius: about 1.6x on barren land and 0.55x on the most fertile (dens = 0.6),
    // so states in the steppe or tundra cover ~8x the area of those in river valleys.
    let rad_factor = |i: usize| (dens * 1.8 * (0.45 - hab[i] as f64)).exp().clamp(0.3, 3.0);
    let r_nom = (sp.state_area_km2.max(1000.0) / 1.2).sqrt();
    let place = |r0: f64| partition::poisson(g, &order, &group, |i| r0 * rad_factor(i) / r_km);
    let (mut lo, mut hi) = (0.25 * r_nom, 4.0 * r_nom);
    let (mut best_r, mut best_err) = (r_nom, usize::MAX);
    for _ in 0..14 {
        let mid = (lo * hi).sqrt();
        let k = place(mid).len();
        let err = k.abs_diff(target);
        if err < best_err {
            best_err = err;
            best_r = mid;
        }
        if k > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let seeds = place(best_r);

    // ------------------------------------------------------------ growth
    ctx.progress(0.35, "Growing states");
    let offsets: Vec<f64> = seeds.iter().map(|&c| sp.size_variation.max(0.0) * best_r * hash_unit(seed, stream::STATES + 100, c as u64)).collect();
    let bw = sp.barrier_weight.max(0.0);
    let sea = sp.sea_crossing.max(1.0);
    let all = vec![0u32; n];
    let mut label = partition::grow(g, r_km, &all, &seeds, Some(&offsets), |a, b| {
        if !land(a) || !land(b) {
            return sea;
        }
        if river_role[a] == role::BACKBONE && river_role[b] == role::BACKBONE && (recv[a] == b as i32 || recv[b] == a as i32) {
            return 0.35;
        }
        1.0 + bw * 0.5 * (bar[a] + bar[b]) as f64
    });

    // State paint: cells under the stroke join the state under its first point.
    let mut scratch = Vec::new();
    let mut painted = 0usize;
    for s in &ctx.edits.overrides.states {
        if s.tool != Tool::StatePaint || s.points.is_empty() {
            continue;
        }
        let anchor = g.nearest(Vec3::from_lat_lon_deg(s.points[0][0], s.points[0][1]), None);
        let l = label[anchor];
        if l == NONE {
            continue;
        }
        for (c, w) in stroke_coverage(g, s, r_km, &mut scratch) {
            if w >= 0.5 && land(c as usize) && label[c as usize] != l {
                label[c as usize] = l;
                painted += 1;
            }
        }
    }

    // ------------------------------------------------------------ clean-up
    ctx.progress(0.55, "Cleaning up borders");
    let mut st: Vec<u32> = (0..n).map(|i| if land(i) { label[i] } else { NONE }).collect();
    let moved = partition::absorb_fragments(g, &mut st, &area, |a, b| land(a) && land(b));
    let before = seeds.len();
    let mut count = partition::merge_small(g, &mut st, seeds.len(), &area, &vec![sp.min_state_area_km2; seeds.len()], |_, _| true);
    // Map editor: merge the state under each drag's start into the one under its end.
    // Edits that no longer fit this map (after a seed or sketch change) are
    // listed in `unapplied_edits`, by their index in the states layer.
    let mut merged = 0usize;
    let mut unapplied = Vec::new();
    for (k, s) in ctx.edits.overrides.states.iter().enumerate() {
        if s.tool != Tool::StateMerge || s.points.len() < 2 {
            continue;
        }
        let at = |p: [f64; 2]| st[g.nearest(Vec3::from_lat_lon_deg(p[0], p[1]), None)];
        let (a, b) = (at(s.points[0]), at(*s.points.last().unwrap()));
        if a != NONE && b != NONE && a != b {
            st.iter_mut().filter(|x| **x == a).for_each(|x| *x = b);
            merged += 1;
        } else {
            unapplied.push(super::unapplied("states", k, s, if a == NONE || b == NONE { "an end is not on land" } else { "already one state" }));
        }
    }
    if merged > 0 {
        let mut map = vec![NONE; count];
        let mut next = 0u32;
        for x in st.iter_mut().filter(|x| **x != NONE) {
            let m = &mut map[*x as usize];
            if *m == NONE {
                *m = next;
                next += 1;
            }
            *x = *m;
        }
        count = next as usize;
    }
    // Seed index -> merged state (via the seed cell), for the water labels.
    let old_to_new: Vec<u32> = seeds.iter().map(|&c| st[c as usize]).collect();
    let water_label: Vec<u32> = (0..n)
        .map(|i| if land(i) { st[i] } else if label[i] != NONE { old_to_new[label[i] as usize] } else { NONE })
        .collect();

    // ------------------------------------------------------------ continents
    ctx.progress(0.65, "Grouping regions and continents");
    let mut cont_lms: Vec<usize> = (0..lm_area.len()).filter(|&k| lm_area[k] >= sp.continent_min_mkm2 * 1e6).collect();
    if cont_lms.is_empty() {
        cont_lms.push(largest);
    }
    cont_lms.sort_by(|&a, &b| lm_area[b].partial_cmp(&lm_area[a]).unwrap().then(a.cmp(&b)));
    // Every landmass belongs to the nearest continent (its own if it is one).
    let sources: Vec<(u32, f64, u32)> =
        cont_lms.iter().enumerate().flat_map(|(k, &l)| lm_cells[l].iter().map(move |&c| (c, 0.0, k as u32))).collect();
    let near = graph::multi_source(g, &sources, f64::INFINITY, |_a, _b, _, edge_len| Some(edge_len)).label;
    let lm_cont: Vec<u32> = lm_cells.iter().map(|c| near[c[0] as usize]).collect();

    // Per-state aggregates.
    let mut s_area = vec![0.0f64; count];
    let mut s_cap = vec![0.0f64; count];
    let mut s_sum = vec![Vec3::new(0.0, 0.0, 0.0); count];
    let mut s_cont_votes: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); count];
    let mut s_capital = vec![(f32::NEG_INFINITY, NONE); count];
    let mut s_cells = vec![0usize; count];
    for i in 0..n {
        let s = st[i];
        if s == NONE {
            continue;
        }
        let s = s as usize;
        s_area[s] += area[i];
        s_cap[s] += area[i] * hab[i] as f64;
        s_sum[s] += g.pos[i] * area[i];
        s_cells[s] += 1;
        *s_cont_votes[s].entry(lm_cont[lm[i] as usize]).or_insert(0.0) += area[i];
        if hab[i] > s_capital[s].0 {
            s_capital[s] = (hab[i], i as u32);
        }
    }
    let s_centroid: Vec<Vec3> = s_sum.iter().map(|v| v.normalized()).collect();
    let s_cont: Vec<u32> = s_cont_votes
        .iter()
        .map(|m| m.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap().then(b.0.cmp(a.0))).map(|(&k, _)| k).unwrap_or(0))
        .collect();

    // State graph: land borders, plus sea links where the water reached by two
    // states meets (islands, straits).
    let mut links: BTreeMap<(u32, u32), f64> = BTreeMap::new();
    for i in 0..n {
        let a = water_label[i];
        if a == NONE {
            continue;
        }
        for &j in g.neighbors(i) {
            let b = water_label[j as usize];
            if b == NONE || b <= a {
                continue;
            }
            let land_edge = land(i) && land(j as usize);
            let w = if land_edge { 1.0 } else { 1.6 };
            let e = links.entry((a, b)).or_insert(w);
            *e = e.min(w);
        }
    }
    let mut adj: Vec<Vec<(u32, f64)>> = vec![Vec::new(); count];
    for (&(a, b), &w) in &links {
        let d = s_centroid[a as usize].angle_to(s_centroid[b as usize]).max(1e-4) * w;
        adj[a as usize].push((b, d));
        adj[b as usize].push((a, d));
    }

    // Regions per continent.
    let n_cont = cont_lms.len();
    let mut s_region = vec![NONE; count];
    let mut region_cont: Vec<u32> = Vec::new();
    let mut region_seed: Vec<u32> = Vec::new();
    for c in 0..n_cont as u32 {
        let members: Vec<u32> = (0..count as u32).filter(|&s| s_cont[s as usize] == c).collect();
        if members.is_empty() {
            continue;
        }
        let k = ((members.len() as f64 / sp.states_per_region.max(1.0)).round() as usize).clamp(1, members.len());
        // Farthest-point seeding from the most populous state.
        let first = *members.iter().max_by(|&&a, &&b| s_cap[a as usize].partial_cmp(&s_cap[b as usize]).unwrap().then(b.cmp(&a))).unwrap();
        let mut rs = vec![first];
        while rs.len() < k {
            let next = members
                .iter()
                .copied()
                .filter(|s| !rs.contains(s))
                .max_by(|&a, &b| {
                    let da = rs.iter().map(|&r| s_centroid[a as usize].angle_to(s_centroid[r as usize])).fold(f64::MAX, f64::min);
                    let db = rs.iter().map(|&r| s_centroid[b as usize].angle_to(s_centroid[r as usize])).fold(f64::MAX, f64::min);
                    da.partial_cmp(&db).unwrap().then(b.cmp(&a))
                })
                .unwrap();
            rs.push(next);
        }
        let mut assign = BTreeMap::new();
        for _ in 0..4 {
            assign = grow_regions(&members, &rs, &adj, &s_centroid);
            // Move each region seed to the member nearest its area-weighted centroid.
            for (r, seed_state) in rs.iter_mut().enumerate() {
                let mut sum = Vec3::new(0.0, 0.0, 0.0);
                for (&s, &rr) in &assign {
                    if rr == r as u32 {
                        sum += s_centroid[s as usize] * s_area[s as usize];
                    }
                }
                if let Some((&s, _)) = assign
                    .iter()
                    .filter(|(_, &rr)| rr == r as u32)
                    .max_by(|a, b| s_centroid[*a.0 as usize].dot(sum).partial_cmp(&s_centroid[*b.0 as usize].dot(sum)).unwrap().then(b.0.cmp(a.0)))
                {
                    *seed_state = s;
                }
            }
        }
        let base = region_cont.len() as u32;
        for (&s, &r) in &assign {
            s_region[s as usize] = base + r;
        }
        for &r in &rs {
            region_cont.push(c);
            region_seed.push(r);
        }
    }

    // ------------------------------------------------------------ numbering
    // Regions by continent, then north to south; states by region, then north to south.
    let lat_of = |v: Vec3| v.lat_lon().0;
    let mut region_order: Vec<u32> = (0..region_cont.len() as u32).collect();
    region_order.sort_by(|&a, &b| {
        region_cont[a as usize].cmp(&region_cont[b as usize]).then(
            lat_of(s_centroid[region_seed[b as usize] as usize]).partial_cmp(&lat_of(s_centroid[region_seed[a as usize] as usize])).unwrap(),
        )
    });
    let mut region_id = vec![0u16; region_cont.len()];
    for (k, &r) in region_order.iter().enumerate() {
        region_id[r as usize] = k as u16 + 1;
    }
    let mut state_order: Vec<u32> = (0..count as u32).collect();
    state_order.sort_by(|&a, &b| {
        let (ra, rb) = (s_region[a as usize], s_region[b as usize]);
        let ka = if ra == NONE { u16::MAX } else { region_id[ra as usize] };
        let kb = if rb == NONE { u16::MAX } else { region_id[rb as usize] };
        ka.cmp(&kb).then(lat_of(s_centroid[b as usize]).partial_cmp(&lat_of(s_centroid[a as usize])).unwrap()).then(a.cmp(&b))
    });
    let mut state_id = vec![0u16; count];
    for (k, &s) in state_order.iter().enumerate() {
        state_id[s as usize] = k as u16 + 1;
    }

    // ------------------------------------------------------------ names & fields
    let langs: Vec<Lang> = (0..n_cont as u32).map(|c| names::continent_lang(seed, c)).collect();
    let mut rng = Rng::new(seed, stream::NAMES);
    let mut used = HashSet::new();
    let cont_names: Vec<String> = langs.iter().map(|l| l.unique(&mut rng, &mut used)).collect();
    let mut region_names = vec![String::new(); region_cont.len()];
    for &r in &region_order {
        region_names[r as usize] = langs[region_cont[r as usize] as usize].unique(&mut rng, &mut used);
    }
    // States speak their region's dialect.
    let dialects: Vec<Lang> = (0..region_cont.len()).map(|r| names::region_lang(seed, region_cont[r], region_id[r])).collect();
    let mut state_names = vec![String::new(); count];
    for &s in &state_order {
        let r = s_region[s as usize];
        let lang = if r == NONE { &langs[s_cont[s as usize] as usize] } else { &dialects[r as usize] };
        state_names[s as usize] = lang.unique(&mut rng, &mut used);
    }
    // Map editor renames.
    let mut renamed = vec![false; count];
    for (e, s) in ctx.edits.overrides.states.iter().enumerate() {
        if s.tool != Tool::RenameState || s.points.is_empty() || s.name.trim().is_empty() {
            continue;
        }
        let k = st[g.nearest(Vec3::from_lat_lon_deg(s.points[0][0], s.points[0][1]), None)];
        if k != NONE {
            state_names[k as usize] = s.name.trim().to_string();
            renamed[k as usize] = true;
        } else {
            unapplied.push(super::unapplied("states", e, s, "not on land"));
        }
    }

    let mut f_state = vec![0u16; n];
    let mut f_region = vec![0u16; n];
    let mut f_cont = vec![0u8; n];
    for i in 0..n {
        let s = st[i];
        if s == NONE {
            continue;
        }
        f_state[i] = state_id[s as usize];
        let r = s_region[s as usize];
        if r != NONE {
            f_region[i] = region_id[r as usize];
        }
        f_cont[i] = (s_cont[s as usize] + 1).min(255) as u8;
    }

    let deg = |v: Vec3| {
        let (la, lo) = v.lat_lon();
        [(la.to_degrees() * 100.0).round() / 100.0, (lo.to_degrees() * 100.0).round() / 100.0]
    };
    let mut states_json = Vec::with_capacity(count);
    for &s in &state_order {
        let su = s as usize;
        let r = s_region[su];
        states_json.push(serde_json::json!({
            "id": state_id[su],
            "key": names::key("STATE", &state_names[su]),
            "name": state_names[su],
            "region": if r == NONE { 0 } else { region_id[r as usize] },
            "continent": s_cont[su] + 1,
            "area_km2": s_area[su].round(),
            "habitability": (s_cap[su] / s_area[su].max(1.0) * 1000.0).round() / 1000.0,
            "capacity_km2": s_cap[su].round(),
            "cells": s_cells[su],
            "center": deg(s_centroid[su]),
            "capital": if s_capital[su].1 != NONE { deg(g.pos[s_capital[su].1 as usize]) } else { deg(s_centroid[su]) },
            "capital_cell": s_capital[su].1,
            "renamed": renamed[su],
        }));
    }
    let mut regions_json = Vec::new();
    for &r in &region_order {
        let ru = r as usize;
        let mut members: Vec<u16> = (0..count).filter(|&s| s_region[s] == r).map(|s| state_id[s]).collect();
        members.sort_unstable();
        let a: f64 = (0..count).filter(|&s| s_region[s] == r).map(|s| s_area[s]).sum();
        regions_json.push(serde_json::json!({
            "id": region_id[ru],
            "key": names::key("REGION", &region_names[ru]),
            "name": region_names[ru],
            "continent": region_cont[ru] + 1,
            "states": members,
            "area_km2": a.round(),
        }));
    }
    let mut conts_json = Vec::new();
    for (c, name) in cont_names.iter().enumerate() {
        let a: f64 = (0..count).filter(|&s| s_cont[s] == c as u32).map(|s| s_area[s]).sum();
        conts_json.push(serde_json::json!({
            "id": c + 1,
            "name": name,
            "area_km2": a.round(),
            "states": (0..count).filter(|&s| s_cont[s] == c as u32).count(),
            "regions": region_cont.iter().filter(|&&rc| rc == c as u32).count(),
        }));
    }
    // Islands without a state of their own, joined across water.
    let attached = (0..lm_area.len())
        .filter(|&k| {
            let c0 = lm_cells[k][0] as usize;
            st[c0] != NONE && !eligible(c0)
        })
        .count();
    let areas: Vec<f64> = s_area.clone();
    let mean = areas.iter().sum::<f64>() / areas.len().max(1) as f64;

    let mut f = Fields::default();
    f.put("state", Field::U16(f_state));
    f.put("region", Field::U16(f_region));
    f.put("continent", Field::U8(f_cont));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "states": count,
            "regions": region_cont.len(),
            "continents": n_cont,
            "target_states": target,
            "seeded_states": before,
            "mean_area_km2": mean.round(),
            "largest_km2": areas.iter().cloned().fold(0.0, f64::max).round(),
            "smallest_km2": areas.iter().cloned().fold(f64::MAX, f64::min).round(),
            "attached_islands": attached,
            "fragments_moved": moved,
            "painted_cells": painted,
            "merged_by_editor": merged,
            "unapplied_edits": unapplied,
            "table": { "states": states_json, "regions": regions_json, "continents": conts_json },
        }),
    }
}

/// Assign states to region seeds by Dijkstra over the state graph. States the
/// graph does not reach go to the nearest seed in a straight line.
fn grow_regions(members: &[u32], seeds: &[u32], adj: &[Vec<(u32, f64)>], centroid: &[Vec3]) -> BTreeMap<u32, u32> {
    let member: HashSet<u32> = members.iter().copied().collect();
    let mut dist: BTreeMap<u32, (f64, u32)> = BTreeMap::new();
    for (r, &s) in seeds.iter().enumerate() {
        dist.insert(s, (0.0, r as u32));
    }
    let mut done: HashSet<u32> = HashSet::new();
    loop {
        // Small graphs: a linear scan for the closest open node is enough.
        let Some((&s, &(d, r))) = dist.iter().filter(|(s, _)| !done.contains(s)).min_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap().then(a.0.cmp(b.0)))
        else {
            break;
        };
        done.insert(s);
        for &(t, w) in &adj[s as usize] {
            if !member.contains(&t) || done.contains(&t) {
                continue;
            }
            let nd = d + w;
            if dist.get(&t).map_or(true, |e| nd < e.0) {
                dist.insert(t, (nd, r));
            }
        }
    }
    let mut out = BTreeMap::new();
    for &s in members {
        let r = match dist.get(&s) {
            Some(&(_, r)) => r,
            None => (0..seeds.len())
                .min_by(|&a, &b| {
                    let da = centroid[s as usize].angle_to(centroid[seeds[a] as usize]);
                    let db = centroid[s as usize].angle_to(centroid[seeds[b] as usize]);
                    da.partial_cmp(&db).unwrap()
                })
                .unwrap() as u32,
        };
        out.insert(s, r);
    }
    out
}

fn empty_output(n: usize) -> StepOutput {
    let mut f = Fields::default();
    f.put("state", Field::U16(vec![0; n]));
    f.put("region", Field::U16(vec![0; n]));
    f.put("continent", Field::U8(vec![0; n]));
    StepOutput {
        fields: f,
        meta: serde_json::json!({ "states": 0, "regions": 0, "continents": 0, "table": { "states": [], "regions": [], "continents": [] } }),
    }
}
