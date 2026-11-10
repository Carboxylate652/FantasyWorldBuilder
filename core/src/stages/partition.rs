//! Shared machinery for cutting the map into regions (states, provinces, sea
//! zones): weighted Poisson-disc seeds, barrier-aware multi-source Dijkstra
//! growth, Lloyd relaxation, merging of regions that are too small, and
//! clean-up of disconnected fragments.

use crate::graph;
use crate::grid::Grid;
use crate::rng::hash_unit;
use crate::vec3::Vec3;
use std::collections::BTreeMap;

pub const NONE: u32 = u32::MAX;

/// Seed-priority order: an exponential race, so cells with a higher weight tend
/// to come first, but every cell has a chance. Deterministic for a seed/stream.
pub fn race_order(cells: impl Iterator<Item = usize>, weight: impl Fn(usize) -> f64, seed: u64, stream: u64) -> Vec<u32> {
    let mut v: Vec<(f64, u32)> = cells
        .map(|i| {
            let u = hash_unit(seed, stream, i as u64).max(1e-12);
            (-u.ln() / weight(i).max(1e-9), i as u32)
        })
        .collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
    v.into_iter().map(|x| x.1).collect()
}

/// Poisson-disc sampling with a per-cell radius (radians). A seed blocks the
/// cells within its radius that share its group.
pub fn poisson(g: &Grid, order: &[u32], group: &[u32], radius: impl Fn(usize) -> f64) -> Vec<u32> {
    let mut blocked = vec![false; g.len()];
    let mut scratch = Vec::new();
    let mut seeds = Vec::new();
    for &c in order {
        let c = c as usize;
        if blocked[c] {
            continue;
        }
        seeds.push(c as u32);
        let gc = group[c];
        g.for_cells_within(g.pos[c], radius(c), Some(c), &mut scratch, |x, _| {
            if group[x] == gc {
                blocked[x] = true;
            }
        });
    }
    seeds
}

/// Connected components of cells that share a group (cells with group NONE are
/// skipped). Returns the component of each cell and the cells of each component.
pub fn group_components(g: &Grid, group: &[u32]) -> (Vec<u32>, Vec<Vec<u32>>) {
    let n = g.len();
    let mut comp = vec![NONE; n];
    let mut list: Vec<Vec<u32>> = Vec::new();
    let mut stack = Vec::new();
    for s in 0..n {
        if comp[s] != NONE || group[s] == NONE {
            continue;
        }
        let id = list.len() as u32;
        let gs = group[s];
        let mut cells = Vec::new();
        comp[s] = id;
        stack.push(s);
        while let Some(c) = stack.pop() {
            cells.push(c as u32);
            for &nb in g.neighbors(c) {
                let nb = nb as usize;
                if comp[nb] == NONE && group[nb] == gs {
                    comp[nb] = id;
                    stack.push(nb);
                }
            }
        }
        list.push(cells);
    }
    (comp, list)
}

/// Add a seed (the earliest cell in `order`) to every group component that has none.
pub fn seed_every_component(g: &Grid, group: &[u32], order: &[u32], seeds: &mut Vec<u32>) {
    let (comp, list) = group_components(g, group);
    let mut has = vec![false; list.len()];
    for &s in seeds.iter() {
        has[comp[s as usize] as usize] = true;
    }
    for &c in order {
        let k = comp[c as usize];
        if k != NONE && !has[k as usize] {
            has[k as usize] = true;
            seeds.push(c);
        }
    }
}

/// Grow all seeds together inside their groups. `cost(a, b)` multiplies the
/// km length of the edge a→b. Returns the seed index reaching each cell.
pub fn grow(g: &Grid, r_km: f64, group: &[u32], seeds: &[u32], offsets: Option<&[f64]>, cost: impl Fn(usize, usize) -> f64) -> Vec<u32> {
    let sources: Vec<(u32, f64, u32)> = seeds.iter().enumerate().map(|(k, &c)| (c, offsets.map_or(0.0, |o| o[k]), k as u32)).collect();
    graph::multi_source(g, &sources, f64::INFINITY, |a, b, _, edge_len| {
        if group[a] == group[b] {
            Some(edge_len * r_km * cost(a, b))
        } else {
            None
        }
    })
    .label
}

/// Lloyd relaxation: move each seed to the cell of its region nearest the
/// region's weighted centroid.
pub fn recentre(g: &Grid, label: &[u32], seeds: &mut [u32], weight: impl Fn(usize) -> f64) {
    let k = seeds.len();
    let mut sum = vec![Vec3::new(0.0, 0.0, 0.0); k];
    for (i, &l) in label.iter().enumerate() {
        if l != NONE {
            sum[l as usize] = sum[l as usize] + g.pos[i] * weight(i).max(1e-6);
        }
    }
    let mut best = vec![(f64::NEG_INFINITY, NONE); k];
    for (i, &l) in label.iter().enumerate() {
        if l == NONE {
            continue;
        }
        let d = g.pos[i].dot(sum[l as usize]);
        let b = &mut best[l as usize];
        if d > b.0 || (d == b.0 && (i as u32) < b.1) {
            *b = (d, i as u32);
        }
    }
    for (s, b) in seeds.iter_mut().zip(best) {
        if b.1 != NONE {
            *s = b.1;
        }
    }
}

/// Union-find with path halving.
fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

/// Merge regions smaller than their `min_area` (per label) into the neighbour they share the
/// longest border with (`adjacent(a, b)` says whether cells a and b may be
/// joined across). Regions with no such neighbour are kept. Labels are then
/// renumbered 0.. in order of their smallest old label. Returns the new count.
pub fn merge_small(g: &Grid, label: &mut [u32], count: usize, area: &[f64], min_area: &[f64], adjacent: impl Fn(usize, usize) -> bool) -> usize {
    let mut a = vec![0.0f64; count];
    for (i, &l) in label.iter().enumerate() {
        if l != NONE {
            a[l as usize] += area[i];
        }
    }
    let mut nb: Vec<BTreeMap<u32, u32>> = vec![BTreeMap::new(); count];
    for i in 0..g.len() {
        let li = label[i];
        if li == NONE {
            continue;
        }
        for &j in g.neighbors(i) {
            let lj = label[j as usize];
            if lj != NONE && lj != li && adjacent(i, j as usize) {
                *nb[li as usize].entry(lj).or_insert(0) += 1;
            }
        }
    }
    let mut parent: Vec<u32> = (0..count as u32).collect();
    let mut by_area: Vec<u32> = (0..count as u32).collect();
    by_area.sort_by(|x, y| a[*x as usize].partial_cmp(&a[*y as usize]).unwrap().then(x.cmp(y)));
    for s in by_area {
        if find(&mut parent, s) != s || a[s as usize] >= min_area[s as usize] {
            continue;
        }
        // Neighbour with the longest shared border (ties: lower id).
        let mut best: Option<(u32, u32)> = None;
        let entries: Vec<(u32, u32)> = nb[s as usize].iter().map(|(&k, &v)| (k, v)).collect();
        let mut merged: BTreeMap<u32, u32> = BTreeMap::new();
        for (k, v) in entries {
            let r = find(&mut parent, k);
            if r != s {
                *merged.entry(r).or_insert(0) += v;
            }
        }
        for (&r, &v) in &merged {
            if best.map_or(true, |(_, bv)| v > bv) {
                best = Some((r, v));
            }
        }
        let Some((t, _)) = best else { continue };
        parent[s as usize] = t;
        a[t as usize] += a[s as usize];
        let moved = std::mem::take(&mut nb[s as usize]);
        for (k, v) in moved {
            let r = find(&mut parent, k);
            if r != t {
                *nb[t as usize].entry(r).or_insert(0) += v;
            }
        }
    }
    let mut new_id = vec![NONE; count];
    let mut next = 0u32;
    for l in 0..count as u32 {
        let r = find(&mut parent, l);
        if new_id[r as usize] == NONE {
            new_id[r as usize] = next;
            next += 1;
        }
    }
    for l in label.iter_mut() {
        if *l != NONE {
            let r = find(&mut parent, *l);
            *l = new_id[r as usize];
        }
    }
    next as usize
}

/// Give every disconnected fragment of a region (all but its largest piece) to
/// the neighbouring region it shares the most border with. Fragments with no
/// neighbour across `adjacent` edges (islands) are left alone. Returns how many
/// cells moved.
pub fn absorb_fragments(g: &Grid, label: &mut [u32], area: &[f64], adjacent: impl Fn(usize, usize) -> bool) -> usize {
    let mut moved = 0;
    for _ in 0..3 {
        let (comp, list) = group_components(g, label);
        // Largest piece of each label.
        let mut main: BTreeMap<u32, (f64, u32)> = BTreeMap::new();
        for (k, cells) in list.iter().enumerate() {
            let l = label[cells[0] as usize];
            let a: f64 = cells.iter().map(|&c| area[c as usize]).sum();
            let e = main.entry(l).or_insert((-1.0, NONE));
            if a > e.0 {
                *e = (a, k as u32);
            }
        }
        let mut changed = 0;
        for (k, cells) in list.iter().enumerate() {
            let l = label[cells[0] as usize];
            if main[&l].1 == k as u32 {
                continue;
            }
            let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
            for &c in cells {
                for &j in g.neighbors(c as usize) {
                    let lj = label[j as usize];
                    if lj != NONE && lj != l && comp[j as usize] != k as u32 && adjacent(c as usize, j as usize) {
                        *counts.entry(lj).or_insert(0) += 1;
                    }
                }
            }
            let Some((&t, _)) = counts.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))) else { continue };
            for &c in cells {
                label[c as usize] = t;
            }
            changed += cells.len();
        }
        moved += changed;
        if changed == 0 {
            break;
        }
    }
    moved
}
