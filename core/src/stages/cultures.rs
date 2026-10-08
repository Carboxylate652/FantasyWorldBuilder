//! Step 11 — cultures (Stage 3).
//!
//! Cultures are not assigned: they emerge from an agent simulation on the
//! province graph.
//!
//! - **Bands** are groups of people with a home province, a population, a
//!   vector of cultural traits (Axelrod's model) and an empty polity slot that
//!   Stage 4 will fill.
//! - **Travel** follows province borders. A step costs its length times
//!   1 + barrier; wasteland is slow; straits open in the second era, coastal
//!   sailing in the third and the open sea in the fourth. Travel range grows
//!   with the eras.
//! - **One tick** (a generation): logistic growth toward each province's
//!   carrying capacity (area × habitability), fission of large bands into the
//!   nearby province with the most room, migration out of crowded provinces,
//!   contact between bands (likelier the closer and the more alike they are;
//!   on contact one copies a trait of the other, and the contact graph gains
//!   weight), random drift, and fading of old contacts.
//! - **Culture checks** every few ticks: the band graph (faded contact ×
//!   similarity²) is split into communities with Louvain. Communities are
//!   matched to the previous cultures by population overlap. A split, a merge
//!   or a band's change of culture only counts once it has held for several
//!   checks in a row, so cultures don't flicker. Splits, merges and extinctions
//!   are logged with dates: the family tree.
//! - **Groups** come from a second, coarser community detection over cultures.
//! - **Names:** each culture speaks a language. The first cultures take a
//!   dialect of their home region's language; a daughter culture starts from a
//!   changed copy of its parent's. Provinces are renamed in the language of
//!   their majority culture, and a state takes its capital's name.

use super::{Ctx, StepOutput};
use crate::community;
use crate::fields::{Field, Fields};
use crate::names::{self, Lang};
use crate::params::CultureParams;
use crate::rng::{hash_unit, stream, Rng};
use crate::vec3::Vec3;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};

const NONE: u32 = u32::MAX;

mod kind {
    pub const LAND: u8 = 0;
    pub const WASTELAND: u8 = 1;
    pub const LAKE: u8 = 2;
    pub const SEA: u8 = 3;
}

mod border {
    pub const LAND: u8 = 0;
    pub const IMPASSABLE: u8 = 1;
    pub const LAKE: u8 = 2;
    pub const COAST: u8 = 3;
    pub const SEA: u8 = 4;
    pub const STRAIT: u8 = 5;
}

struct Prov {
    id: u32,
    kind: u8,
    /// Sea band: 0 coastal, 1 shelf, 2 open.
    band: u8,
    area: f64,
    hab: f64,
    center: Vec3,
    state: u16,
    region: u16,
    continent: u32,
    name: String,
}

struct Border {
    to: u32,
    ty: u8,
    km: f64,
    barrier: f64,
}

struct Culture {
    parent: u32,
    founded: u32,
    ended: Option<u32>,
    fate: &'static str,
    other: u32,
    lang: Lang,
    region: u16,
}

struct Candidate {
    parent: u32,
    members: Vec<u32>,
    streak: u32,
}

/// Bands, struct of arrays.
#[derive(Default)]
struct Bands {
    prov: Vec<u32>,
    pop: Vec<f64>,
    traits: Vec<u8>,
    culture: Vec<u32>,
    pending: Vec<(u32, u32)>,
    alive: Vec<bool>,
    /// Polity of each band: always 0 in Stage 3, filled by Stage 4.
    polity: Vec<u32>,
}

impl Bands {
    fn len(&self) -> usize {
        self.prov.len()
    }
    fn traits(&self, b: usize, t: usize) -> &[u8] {
        &self.traits[b * t..(b + 1) * t]
    }
    fn push(&mut self, prov: u32, pop: f64, traits: &[u8], culture: u32) {
        self.prov.push(prov);
        self.pop.push(pop);
        self.traits.extend_from_slice(traits);
        self.culture.push(culture);
        self.pending.push((0, 0));
        self.alive.push(true);
        self.polity.push(0);
    }
}

fn similarity(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).filter(|(x, y)| x == y).count() as f64 / a.len().max(1) as f64
}

fn hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let h = h.rem_euclid(360.0);
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

/// Culture colour: its group's hue, shifted a little per culture (same formula
/// as `cultureColor` in app/src/layers.ts).
pub fn culture_color(id: u32, group: u32) -> [u8; 3] {
    let h = (group as f64 * 137.508) % 360.0 + ((id * 67) % 40) as f64 - 20.0;
    hsl(h, 0.42 + 0.12 * (id % 3) as f64, 0.42 + 0.07 * (id % 4) as f64)
}

pub fn group_color(group: u32) -> [u8; 3] {
    hsl((group as f64 * 137.508) % 360.0, 0.55, 0.5)
}

/// Bounded Dijkstra over the province graph. Returns (province, cost) for
/// every land or wasteland province within `range`, the source included.
fn reach(provs: &[Prov], adj: &[Vec<Border>], src: usize, range: f64, cost: &dyn Fn(usize, &Border) -> Option<f64>) -> Vec<(u32, f32)> {
    let mut dist: HashMap<u32, f64> = HashMap::new();
    let mut heap = BinaryHeap::new();
    dist.insert(src as u32, 0.0);
    heap.push(Reverse((0u64, src as u32)));
    let mut out = Vec::new();
    while let Some(Reverse((bits, p))) = heap.pop() {
        let d = f64::from_bits(bits);
        if d > dist[&p] {
            continue;
        }
        let pu = p as usize;
        if provs[pu].kind <= kind::WASTELAND {
            out.push((p, d as f32));
        }
        for e in &adj[pu] {
            let Some(c) = cost(pu, e) else { continue };
            let nd = d + c;
            if nd <= range && dist.get(&e.to).map_or(true, |&x| nd < x) {
                dist.insert(e.to, nd);
                heap.push(Reverse((nd.to_bits(), e.to)));
            }
        }
    }
    out.sort_by_key(|x| x.0);
    out
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let cp = &ctx.params.cultures;
    let seed = ctx.params.planet.seed;
    let r_km = ctx.params.planet.radius_km;
    let table = ctx.input.meta("table");
    let empty = Vec::new();

    // ------------------------------------------------------------ province graph
    let rows = table["provinces"].as_array().unwrap_or(&empty);
    let index: HashMap<u32, usize> = rows.iter().enumerate().map(|(k, p)| (p["id"].as_u64().unwrap_or(0) as u32, k)).collect();
    let provs: Vec<Prov> = rows
        .iter()
        .map(|p| {
            let ll = &p["center"];
            Prov {
                id: p["id"].as_u64().unwrap_or(0) as u32,
                kind: match p["kind"].as_str() {
                    Some("land") => kind::LAND,
                    Some("wasteland") => kind::WASTELAND,
                    Some("lake") => kind::LAKE,
                    _ => kind::SEA,
                },
                band: match p["band"].as_str() {
                    Some("shelf") => 1,
                    Some("open") => 2,
                    _ => 0,
                },
                area: p["area_km2"].as_f64().unwrap_or(0.0),
                hab: p["habitability"].as_f64().unwrap_or(0.0),
                center: Vec3::from_lat_lon_deg(ll[0].as_f64().unwrap_or(0.0), ll[1].as_f64().unwrap_or(0.0)),
                state: p["state"].as_u64().unwrap_or(0) as u16,
                region: p["region"].as_u64().unwrap_or(0) as u16,
                continent: (p["continent"].as_u64().unwrap_or(1) as u32).saturating_sub(1),
                name: p["name"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();
    let np = provs.len();
    let mut adj: Vec<Vec<Border>> = (0..np).map(|_| Vec::new()).collect();
    for a in table["adjacency"].as_array().unwrap_or(&empty) {
        let (Some(&x), Some(&y)) = (index.get(&(a["from"].as_u64().unwrap_or(0) as u32)), index.get(&(a["to"].as_u64().unwrap_or(0) as u32))) else { continue };
        let ty = match a["type"].as_str() {
            Some("land") | Some("river") => border::LAND,
            Some("impassable") => border::IMPASSABLE,
            Some("lake") => border::LAKE,
            Some("coast") => border::COAST,
            Some("sea") => border::SEA,
            Some("strait") => border::STRAIT,
            _ => continue,
        };
        let km = if ty == border::STRAIT { a["crossing_km"].as_f64().unwrap_or(100.0) } else { provs[x].center.angle_to(provs[y].center) * r_km };
        let barrier = a["barrier"].as_f64().unwrap_or(0.0);
        adj[x].push(Border { to: y as u32, ty, km, barrier });
        adj[y].push(Border { to: x as u32, ty, km, barrier });
    }
    let capacity: Vec<f64> = provs.iter().map(|p| if p.kind <= kind::WASTELAND { cp.density_per_km2 * p.area * p.hab.clamp(0.0, 1.0).powf(1.5) } else { 0.0 }).collect();
    let k_total: f64 = capacity.iter().sum();
    if np == 0 || k_total <= 0.0 {
        return empty_output(ctx.grid.len());
    }
    // At least one band per province that can hold people, so the land can fill.
    let max_bands = (cp.max_bands as usize).max(capacity.iter().filter(|&&k| k > 0.0).count()).max(10);
    let split_pop = 2.0 * k_total / (0.8 * max_bands as f64);
    let nt = cp.traits.clamp(1, 64) as usize;
    let nv = cp.trait_values.clamp(2, 255) as u32;
    let ticks = cp.ticks.max(1);

    // Edge cost in each era (None = not passable yet).
    let bw = cp.barrier_weight.max(0.0);
    let edge_cost = |era: usize| {
        let provs = &provs;
        move |a: usize, e: &Border| -> Option<f64> {
            let b = &provs[e.to as usize];
            let sea_ok = |p: &Prov| era >= 3 || (era >= 2 && p.band == 0);
            match e.ty {
                border::LAND => Some(e.km * (1.0 + bw * e.barrier)),
                border::IMPASSABLE => Some(3.0 * e.km * (1.0 + bw * e.barrier)),
                border::LAKE => Some(1.5 * e.km),
                border::STRAIT => (era >= 1).then(|| 2.0 * e.km),
                border::COAST => {
                    let sea = if provs[a].kind == kind::SEA { &provs[a] } else { b };
                    sea_ok(sea).then(|| cp.sea_cost * e.km)
                }
                _ => (sea_ok(&provs[a]) && sea_ok(b)).then(|| cp.sea_cost * e.km),
            }
        }
    };
    let neighbourhoods = |era: usize| -> Vec<Vec<(u32, f32)>> {
        let cost = edge_cost(era);
        let range = cp.travel_range_km[era.min(3)].max(1.0);
        (0..np).map(|p| if provs[p].kind <= kind::WASTELAND { reach(&provs, &adj, p, range, &cost) } else { Vec::new() }).collect()
    };

    // ------------------------------------------------------------ initial bands
    let mut rng = Rng::new(seed, stream::CULTURES);
    let mut bands = Bands::default();
    {
        let mut order: Vec<(f64, usize)> = (0..np)
            .filter(|&p| capacity[p] > 0.0 && provs[p].kind == kind::LAND)
            .map(|p| (-hash_unit(seed, stream::CULTURES, p as u64).max(1e-12).ln() / capacity[p], p))
            .collect();
        order.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        for &(_, p) in order.iter().take(cp.initial_bands.max(1) as usize) {
            let tr: Vec<u8> = (0..nt).map(|_| rng.below(nv as usize) as u8).collect();
            bands.push(p as u32, (0.5 * split_pop).min(0.5 * capacity[p]), &tr, 0);
        }
    }

    // ------------------------------------------------------------ simulation
    let mut contact: HashMap<u64, f32> = HashMap::new();
    let key = |a: usize, b: usize| ((a.min(b) as u64) << 32) | a.max(b) as u64;
    let mut cultures: Vec<Culture> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut events: Vec<serde_json::Value> = Vec::new();
    let mut era = usize::MAX;
    let mut hood: Vec<Vec<(u32, f32)>> = Vec::new();
    let mut checks = 0u32;
    let mut last_graph: (Vec<u32>, Vec<(u32, u32, f64)>) = (Vec::new(), Vec::new());
    let year = |t: u32| (t as f64 * cp.years_per_tick).round();

    for t in 0..ticks {
        let frac = t as f64 / ticks as f64;
        let e = cp.era_starts.iter().filter(|&&s| frac >= s).count();
        if e != era {
            era = e;
            ctx.progress(0.02 + 0.9 * frac as f32, &format!("Era {}: mapping travel routes", era + 1));
            hood = neighbourhoods(era);
        } else if t % 20 == 0 {
            ctx.progress(0.02 + 0.9 * frac as f32, &format!("Generation {t} of {ticks}: {} bands, {} cultures", bands.alive.iter().filter(|&&a| a).count(), cultures.iter().filter(|c| c.ended.is_none()).count()));
        }
        let scale = cp.contact_scale_km.max(1.0);
        let n0 = bands.len();

        // 1. growth
        let mut pp = vec![0.0f64; np];
        for b in 0..n0 {
            if bands.alive[b] {
                pp[bands.prov[b] as usize] += bands.pop[b];
            }
        }
        for b in 0..n0 {
            if !bands.alive[b] {
                continue;
            }
            let p = bands.prov[b] as usize;
            let k = capacity[p];
            if k <= 1e-9 {
                bands.pop[b] *= 0.5;
            } else {
                bands.pop[b] += cp.growth_rate * bands.pop[b] * (1.0 - pp[p] / k);
            }
            if bands.pop[b] < 1.0 {
                bands.alive[b] = false;
            }
        }
        let mut pnow = vec![0.0f64; np];
        let mut n_alive = 0usize;
        for b in 0..n0 {
            if bands.alive[b] {
                pnow[bands.prov[b] as usize] += bands.pop[b];
                n_alive += 1;
            }
        }

        // 2. fission and migration
        let mut count = vec![0u32; np];
        for b in 0..n0 {
            if bands.alive[b] {
                count[bands.prov[b] as usize] += 1;
            }
        }
        for b in 0..n0 {
            if !bands.alive[b] {
                continue;
            }
            let p = bands.prov[b] as usize;
            // A band splits when it is large, or when its province is nearly
            // full (then the new band settles elsewhere); bands in an
            // overcrowded province may move.
            let full = pnow[p] >= 0.9 * capacity[p];
            // (In sparse land a full province holds few people, so the bar scales with it.)
            let wants = bands.pop[b] > split_pop || (full && bands.pop[b] > (0.25 * split_pop).min(0.5 * capacity[p]).max(20.0));
            let crowded = pnow[p] > 1.2 * capacity[p];
            if !wants && !(crowded && rng.f64() < 0.5) {
                continue;
            }
            // Targets: the best unsettled province (to found a band or settle
            // new land) and the best province with any room (to escape
            // crowding), by room × closeness.
            let (mut open, mut any) = ((0.0f64, p), (0.0f64, p));
            for &(q, c) in &hood[p] {
                let q = q as usize;
                let room = capacity[q] - pnow[q];
                if room > 0.0 && q != p {
                    let s = room * (-(c as f64) / scale).exp() * (0.5 + 0.5 * rng.f64());
                    if s > any.0 {
                        any = (s, q);
                    }
                    if count[q] == 0 && s > open.0 {
                        open = (s, q);
                    }
                }
            }
            // A new band always settles an unsettled province, so bands stay
            // about one per province. At the band cap, a band from a full
            // province it shares with others moves there instead, so new land
            // is still settled.
            let settle = open.1 != p;
            let split = wants && n_alive < max_bands && settle;
            let colonise = wants && !split && settle && count[p] >= 2;
            let q = if split || colonise { open.1 } else { any.1 };
            if split {
                let half = 0.5 * bands.pop[b];
                bands.pop[b] = half;
                let mut tr = bands.traits(b, nt).to_vec();
                for x in tr.iter_mut() {
                    if rng.f64() < cp.fission_mutation {
                        *x = rng.below(nv as usize) as u8;
                    }
                }
                let c = bands.culture[b];
                pnow[p] -= half;
                pnow[q] += half;
                bands.push(q as u32, half, &tr, c);
                count[q] += 1;
                n_alive += 1;
            } else if q != p && (colonise || crowded) {
                count[p] -= 1;
                count[q] += 1;
                pnow[p] -= bands.pop[b];
                pnow[q] += bands.pop[b];
                bands.prov[b] = q as u32;
            }
        }

        // 3. contact
        let n1 = bands.len();
        let mut in_prov: Vec<Vec<u32>> = vec![Vec::new(); np];
        for b in 0..n1 {
            if bands.alive[b] {
                in_prov[bands.prov[b] as usize].push(b as u32);
            }
        }
        // Successful contacts of each band this tick (for conformity).
        let mut met: Vec<Vec<u32>> = vec![Vec::new(); n1];
        // Per source province: cumulative contact weights over its neighbourhood.
        let mut cum: Vec<Option<Vec<f64>>> = (0..np).map(|_| None).collect();
        for b in 0..n1 {
            if !bands.alive[b] {
                continue;
            }
            let p = bands.prov[b] as usize;
            if cum[p].is_none() {
                let mut acc = 0.0;
                cum[p] = Some(
                    hood[p]
                        .iter()
                        .map(|&(q, c)| {
                            acc += pnow[q as usize].max(0.0) * (-(c as f64) / scale).exp();
                            acc
                        })
                        .collect(),
                );
            }
            let weights = cum[p].as_ref().unwrap();
            let total = weights.last().copied().unwrap_or(0.0);
            if total <= 0.0 {
                continue;
            }
            for _ in 0..cp.contacts_per_tick {
                let r = rng.f64() * total;
                let k = weights.partition_point(|&w| w <= r).min(weights.len() - 1);
                let list = &in_prov[hood[p][k].0 as usize];
                if list.is_empty() {
                    continue;
                }
                let mut r2 = rng.f64() * list.iter().map(|&c| bands.pop[c as usize]).sum::<f64>();
                let mut c = list[list.len() - 1] as usize;
                for &x in list {
                    r2 -= bands.pop[x as usize];
                    if r2 <= 0.0 {
                        c = x as usize;
                        break;
                    }
                }
                if c == b {
                    continue;
                }
                let sim = similarity(bands.traits(b, nt), bands.traits(c, nt));
                if rng.f64() >= sim.powf(cp.homophily.max(0.0)).max(0.01) {
                    continue;
                }
                *contact.entry(key(b, c)).or_insert(0.0) += 1.0;
                met[b].push(c as u32);
                met[c].push(b as u32);
                if sim < 1.0 {
                    // The smaller band is likelier to take a trait from the larger.
                    let (from, to) = if rng.f64() < bands.pop[c] / (bands.pop[b] + bands.pop[c]) { (c, b) } else { (b, c) };
                    let diff: Vec<usize> = (0..nt).filter(|&i| bands.traits[from * nt + i] != bands.traits[to * nt + i]).collect();
                    let i = diff[rng.below(diff.len())];
                    bands.traits[to * nt + i] = bands.traits[from * nt + i];
                }
            }
        }

        // Conformity: for one trait, take the value most successful contacts share.
        if cp.conformity > 0.0 {
            let mut votes = vec![0u32; nv as usize];
            for b in 0..n1 {
                if !bands.alive[b] || met[b].len() < 2 || rng.f64() >= cp.conformity {
                    continue;
                }
                let i = rng.below(nt);
                votes.iter_mut().for_each(|v| *v = 0);
                for &c in &met[b] {
                    votes[bands.traits[c as usize * nt + i] as usize] += 1;
                }
                let own = bands.traits[b * nt + i] as usize;
                let (v, &n) = votes.iter().enumerate().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(&a.0))).unwrap();
                if v != own && 2 * n as usize > met[b].len() && n > votes[own] {
                    bands.traits[b * nt + i] = v as u8;
                }
            }
        }

        // 4. drift (geometric skips over all band × trait slots)
        if cp.drift > 0.0 {
            let total = (n1 * nt) as f64;
            let ln_q = (1.0 - cp.drift.min(0.999)).ln();
            let mut pos = (rng.f64().max(1e-12).ln() / ln_q).floor();
            while pos < total {
                let s = pos as usize;
                if bands.alive[s / nt] {
                    bands.traits[s] = rng.below(nv as usize) as u8;
                }
                pos += 1.0 + (rng.f64().max(1e-12).ln() / ln_q).floor();
            }
        }

        // 5. old contacts fade
        let mem = cp.contact_memory.clamp(0.0, 1.0) as f32;
        contact.retain(|_, w| {
            *w *= mem;
            *w > 0.05
        });

        // 6. culture check
        if (t + 1) % cp.check_every.max(1) == 0 || t + 1 == ticks {
            checks += 1;
            let (comm, graph) = detect(&bands, &contact, &in_prov, nt, cp);
            let tick = t + 1;
            update_cultures(seed, cp, tick, &mut bands, &comm, &provs, &mut cultures, &mut candidates, &mut events, year(tick));
            last_graph = graph;
        }
    }

    // ------------------------------------------------------------ groups
    ctx.progress(0.93, "Grouping cultures");
    let n_cult = cultures.len();
    let alive_bands: Vec<usize> = (0..bands.len()).filter(|&b| bands.alive[b]).collect();
    let mut cult_pop = vec![0.0f64; n_cult + 1];
    for &b in &alive_bands {
        cult_pop[bands.culture[b] as usize] += bands.pop[b];
    }
    // Culture-level graph from the last check's band graph.
    let (node_band, edges) = &last_graph;
    let mut cedges: Vec<(u32, u32, f64)> = Vec::new();
    for &(a, b, w) in edges {
        let (ca, cb) = (bands.culture[node_band[a as usize] as usize], bands.culture[node_band[b as usize] as usize]);
        if ca > 0 && cb > 0 {
            cedges.push((ca, cb, w));
        }
    }
    let raw_groups = community::louvain(n_cult + 1, &cedges, cp.group_resolution);
    // Number groups 1.. by population (only living cultures count).
    let mut gpop: BTreeMap<u32, f64> = BTreeMap::new();
    for c in 1..=n_cult {
        if cultures[c - 1].ended.is_none() {
            *gpop.entry(raw_groups[c]).or_insert(0.0) += cult_pop[c];
        }
    }
    let mut gorder: Vec<(u32, f64)> = gpop.into_iter().collect();
    gorder.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    let gid: HashMap<u32, u32> = gorder.iter().enumerate().map(|(k, &(g, _))| (g, k as u32 + 1)).collect();
    let group_of = |c: usize| -> u32 { if c == 0 || cultures[c - 1].ended.is_some() { 0 } else { gid[&raw_groups[c]] } };

    // ------------------------------------------------------------ provinces
    ctx.progress(0.95, "Naming cultures and provinces");
    let mut prov_cult: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); np];
    for &b in &alive_bands {
        *prov_cult[bands.prov[b] as usize].entry(bands.culture[b]).or_insert(0.0) += bands.pop[b];
    }
    let majority = |m: &BTreeMap<u32, f64>| m.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap().then(b.0.cmp(a.0))).map(|(&c, _)| c);
    let prov_major: Vec<u32> = prov_cult.iter().map(|m| majority(m).unwrap_or(0)).collect();
    let world_pop: f64 = cult_pop.iter().sum();

    // Names: cultures, groups, then provinces (majority culture's language, or
    // the state's main culture for empty land).
    let mut name_rng = Rng::new(seed, stream::NAMES + 3);
    let mut used: HashSet<String> = HashSet::new();
    for key in ["continents", "regions"] {
        for r in table[key].as_array().unwrap_or(&empty) {
            if let Some(n) = r["name"].as_str() {
                used.insert(n.to_string());
            }
        }
    }
    let cult_names: Vec<String> = (0..n_cult).map(|c| cultures[c].lang.unique(&mut name_rng, &mut used)).collect();
    let mut state_cult: BTreeMap<u16, BTreeMap<u32, f64>> = BTreeMap::new();
    for p in 0..np {
        if provs[p].state > 0 {
            for (&c, &v) in &prov_cult[p] {
                *state_cult.entry(provs[p].state).or_default().entry(c).or_insert(0.0) += v;
            }
        }
    }
    let state_major: BTreeMap<u16, u32> = state_cult.iter().filter_map(|(&s, m)| majority(m).map(|c| (s, c))).collect();
    let mut prov_names: Vec<String> = provs.iter().map(|p| p.name.clone()).collect();
    // The culture that names each land province (0 = keeps its Stage 2 name).
    let namer: Vec<u32> = (0..np)
        .map(|p| match provs[p].kind {
            kind::LAND | kind::WASTELAND if prov_major[p] > 0 => prov_major[p],
            kind::LAND | kind::WASTELAND => state_major.get(&provs[p].state).copied().unwrap_or(0),
            _ => 0,
        })
        .collect();
    for p in 0..np {
        if namer[p] == 0 {
            used.insert(provs[p].name.clone());
        }
    }
    for p in 0..np {
        let c = namer[p];
        if c == 0 {
            continue;
        }
        let w = cultures[c as usize - 1].lang.unique(&mut name_rng, &mut used);
        // Wasteland keeps its kind of name ("… Desert", "… Peaks", "… Ice").
        let old = &provs[p].name;
        prov_names[p] = match [" Desert", " Peaks", " Ice"].iter().find(|s| old.ends_with(*s)) {
            Some(s) if provs[p].kind == kind::WASTELAND => format!("{w}{s}"),
            _ => w,
        };
    }
    // States take their capital's name.
    let mut states_json = Vec::new();
    for s in table["states"].as_array().unwrap_or(&empty) {
        let sid = s["id"].as_u64().unwrap_or(0) as u16;
        let cap = s["capital_province"].as_u64().and_then(|c| index.get(&(c as u32))).copied();
        let name = cap.filter(|&p| namer[p] > 0).map(|p| prov_names[p].trim_end_matches(" Desert").trim_end_matches(" Peaks").trim_end_matches(" Ice").to_string());
        states_json.push(serde_json::json!({
            "id": sid,
            "name": name.unwrap_or_else(|| s["name"].as_str().unwrap_or("").to_string()),
            "culture": state_major.get(&sid).copied().unwrap_or(0),
        }));
    }

    // ------------------------------------------------------------ tables
    let mut cult_provs = vec![0usize; n_cult + 1];
    for &c in &prov_major {
        cult_provs[c as usize] += 1;
    }
    let cultures_json: Vec<serde_json::Value> = (1..=n_cult)
        .map(|c| {
            let cu = &cultures[c - 1];
            let g = group_of(c);
            serde_json::json!({
                "id": c, "name": cult_names[c - 1], "group": g, "color": culture_color(c as u32, g),
                "parent": cu.parent, "alive": cu.ended.is_none(),
                "population": cult_pop[c].round(), "provinces": cult_provs[c],
                "founded_year": year(cu.founded), "ended_year": cu.ended.map(year),
                "fate": cu.fate, "fate_other": cu.other, "home_region": cu.region,
            })
        })
        .collect();
    let groups_json: Vec<serde_json::Value> = gorder
        .iter()
        .enumerate()
        .map(|(k, &(g, pop))| {
            let g1 = k as u32 + 1;
            let members: Vec<usize> = (1..=n_cult).filter(|&c| cultures[c - 1].ended.is_none() && raw_groups[c] == g).collect();
            let lead = members.iter().copied().max_by(|&a, &b| cult_pop[a].partial_cmp(&cult_pop[b]).unwrap().then(b.cmp(&a))).unwrap_or(1);
            serde_json::json!({ "id": g1, "name": group_name(&cult_names[lead - 1]), "color": group_color(g1), "cultures": members, "population": pop.round() })
        })
        .collect();
    let provinces_json: Vec<serde_json::Value> = (0..np)
        .filter(|&p| provs[p].kind <= kind::WASTELAND)
        .map(|p| {
            let total: f64 = prov_cult[p].values().sum();
            let mut shares: Vec<(u32, f64)> = prov_cult[p].iter().map(|(&c, &v)| (c, v / total.max(1e-9))).collect();
            shares.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
            let shares: Vec<serde_json::Value> = shares.iter().filter(|s| s.1 >= 0.05).take(4).map(|s| serde_json::json!([s.0, (s.1 * 1000.0).round() / 1000.0])).collect();
            serde_json::json!({
                "id": provs[p].id, "name": prov_names[p], "population": total.round(),
                "density_km2": (total / provs[p].area.max(1.0) * 100.0).round() / 100.0,
                "culture": prov_major[p], "shares": shares,
            })
        })
        .collect();

    // ------------------------------------------------------------ fields
    let prov_field = ctx.input.u32("province");
    let n = ctx.grid.len();
    let mut f_cult = vec![0u16; n];
    let mut f_group = vec![0u16; n];
    let mut f_pop = vec![0f32; n];
    for i in 0..n {
        let Some(&p) = index.get(&prov_field[i]) else { continue };
        let c = prov_major[p];
        f_cult[i] = c.min(u16::MAX as u32) as u16;
        f_group[i] = group_of(c as usize).min(u16::MAX as u32) as u16;
        if provs[p].kind <= kind::WASTELAND {
            f_pop[i] = (prov_cult[p].values().sum::<f64>() / provs[p].area.max(1.0)) as f32;
        }
    }
    let alive_cultures = cultures.iter().filter(|c| c.ended.is_none()).count();
    let count_fate = |f: &str| cultures.iter().filter(|c| c.fate == f).count();
    let mut f = Fields::default();
    f.put("culture", Field::U16(f_cult));
    f.put("culture_group", Field::U16(f_group));
    f.put("population", Field::F32(f_pop));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "cultures": alive_cultures,
            "groups": gorder.len(),
            "cultures_ever": n_cult,
            "splits": cultures.iter().filter(|c| c.parent > 0).count(),
            "merged": count_fate("merged"),
            "extinct": count_fate("extinct"),
            "bands": alive_bands.len(),
            "population": world_pop.round(),
            "capacity": k_total.round(),
            "band_split_pop": split_pop.round(),
            "years": year(ticks),
            "checks": checks,
            "table": {
                "cultures": cultures_json,
                "groups": groups_json,
                "events": events,
                "provinces": provinces_json,
                "states": states_json,
            },
        }),
    }
}

/// Replace province and state names in a Stage 2 table with the culture
/// step's names, and add each province's culture and population.
pub fn apply_names(table: &mut serde_json::Value, cultures: &serde_json::Value) {
    let mut prov: HashMap<u64, &serde_json::Value> = HashMap::new();
    for p in cultures["provinces"].as_array().into_iter().flatten() {
        prov.insert(p["id"].as_u64().unwrap_or(0), p);
    }
    let states: HashMap<u64, &serde_json::Value> = cultures["states"].as_array().into_iter().flatten().map(|s| (s["id"].as_u64().unwrap_or(0), s)).collect();
    if let Some(list) = table["provinces"].as_array_mut() {
        for p in list {
            if let Some(c) = prov.get(&p["id"].as_u64().unwrap_or(0)) {
                p["name"] = c["name"].clone();
                p["culture"] = c["culture"].clone();
                p["population"] = c["population"].clone();
            }
        }
    }
    if let Some(list) = table["states"].as_array_mut() {
        for s in list {
            if let Some(c) = states.get(&s["id"].as_u64().unwrap_or(0)) {
                s["name"] = c["name"].clone();
                s["key"] = serde_json::json!(names::key("STATE", c["name"].as_str().unwrap_or("")));
                s["culture"] = c["culture"].clone();
            }
        }
    }
}

/// "Velan" → "Velanic", "Tora" → "Toric".
fn group_name(culture: &str) -> String {
    let base = culture.trim_end_matches(|c: char| "aeiouy".contains(c));
    let base = if base.len() < 2 { culture } else { base };
    format!("{base}ic")
}

/// Band graph (faded contact × similarity², plus a weak link between bands
/// sharing a province) and its communities; tiny communities join the one
/// they are most tied to. Returns the community of each band (NONE if dead)
/// and the graph (node → band, edges between nodes).
fn detect(bands: &Bands, contact: &HashMap<u64, f32>, in_prov: &[Vec<u32>], nt: usize, cp: &CultureParams) -> (Vec<u32>, (Vec<u32>, Vec<(u32, u32, f64)>)) {
    let nb = bands.len();
    let mut node = vec![NONE; nb];
    let mut node_band = Vec::new();
    for b in 0..nb {
        if bands.alive[b] {
            node[b] = node_band.len() as u32;
            node_band.push(b as u32);
        }
    }
    let mut keys: Vec<(&u64, &f32)> = contact.iter().collect();
    keys.sort_by_key(|x| *x.0);
    let mut edges: Vec<(u32, u32, f64)> = Vec::new();
    for (&k, &w) in keys {
        let (a, b) = ((k >> 32) as usize, (k & 0xFFFF_FFFF) as usize);
        if node[a] == NONE || node[b] == NONE {
            continue;
        }
        let s = similarity(bands.traits(a, nt), bands.traits(b, nt));
        edges.push((node[a], node[b], w as f64 * s * s + 1e-6));
    }
    for list in in_prov {
        for (i, &a) in list.iter().enumerate() {
            for &b in &list[i + 1..] {
                let s = similarity(bands.traits(a as usize, nt), bands.traits(b as usize, nt));
                edges.push((node[a as usize], node[b as usize], 0.5 * s * s + 1e-6));
            }
        }
    }
    let nn = node_band.len();
    let mut comm = community::louvain(nn, &edges, cp.culture_resolution);
    // Tiny communities join the community they share the most weight with.
    let nc = comm.iter().copied().max().map_or(0, |m| m as usize + 1);
    let world: f64 = node_band.iter().map(|&b| bands.pop[b as usize]).sum();
    let min_pop = cp.min_culture_share * world;
    let mut cpop = vec![0.0; nc];
    for (i, &c) in comm.iter().enumerate() {
        cpop[c as usize] += bands.pop[node_band[i] as usize];
    }
    let mut link: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); nc];
    for &(a, b, w) in &edges {
        let (ca, cb) = (comm[a as usize], comm[b as usize]);
        if ca != cb {
            *link[ca as usize].entry(cb).or_insert(0.0) += w;
            *link[cb as usize].entry(ca).or_insert(0.0) += w;
        }
    }
    let mut parent: Vec<u32> = (0..nc as u32).collect();
    fn root(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    let mut order: Vec<usize> = (0..nc).collect();
    order.sort_by(|&a, &b| cpop[a].partial_cmp(&cpop[b]).unwrap().then(a.cmp(&b)));
    for c in order {
        if root(&mut parent, c as u32) != c as u32 || cpop[c] >= min_pop {
            continue;
        }
        let mut best: Option<(u32, f64)> = None;
        let links: Vec<(u32, f64)> = link[c].iter().map(|(&k, &v)| (k, v)).collect();
        let mut agg: BTreeMap<u32, f64> = BTreeMap::new();
        for (k, v) in links {
            let r = root(&mut parent, k);
            if r != c as u32 {
                *agg.entry(r).or_insert(0.0) += v;
            }
        }
        for (&r, &v) in &agg {
            if best.map_or(true, |(_, bv)| v > bv) {
                best = Some((r, v));
            }
        }
        if let Some((r, _)) = best {
            parent[c] = r;
            cpop[r as usize] += cpop[c];
            let moved = std::mem::take(&mut link[c]);
            for (k, v) in moved {
                *link[r as usize].entry(k).or_insert(0.0) += v;
            }
        }
    }
    for c in comm.iter_mut() {
        *c = root(&mut parent, *c);
    }
    community::renumber(&mut comm);
    let mut out = vec![NONE; nb];
    for (i, &b) in node_band.iter().enumerate() {
        out[b as usize] = comm[i];
    }
    (out, (node_band, edges))
}

/// Match this check's communities to cultures and apply splits, merges and
/// band changes that have held for `persistence` checks.
#[allow(clippy::too_many_arguments)]
fn update_cultures(
    seed: u64,
    cp: &CultureParams,
    tick: u32,
    bands: &mut Bands,
    comm: &[u32],
    provs: &[Prov],
    cultures: &mut Vec<Culture>,
    candidates: &mut Vec<Candidate>,
    events: &mut Vec<serde_json::Value>,
    year: f64,
) {
    let nb = bands.len();
    let nc = comm.iter().filter(|&&c| c != NONE).copied().max().map_or(0, |m| m as usize + 1);
    let mut members: Vec<Vec<u32>> = vec![Vec::new(); nc];
    let mut cpop = vec![0.0f64; nc];
    let mut overlap: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); nc];
    let mut prov_pop: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); nc];
    for b in 0..nb {
        let c = comm[b];
        if c == NONE {
            continue;
        }
        members[c as usize].push(b as u32);
        cpop[c as usize] += bands.pop[b];
        *overlap[c as usize].entry(bands.culture[b]).or_insert(0.0) += bands.pop[b];
        *prov_pop[c as usize].entry(bands.prov[b]).or_insert(0.0) += bands.pop[b];
    }
    let world: f64 = cpop.iter().sum();
    let persist = cp.persistence.max(1);
    let home_region = |c: usize| -> (u16, u32) {
        let p = prov_pop[c].iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap().then(b.0.cmp(a.0))).map_or(0, |(&p, _)| p as usize);
        (provs[p].region, provs[p].continent)
    };
    let new_culture = |cultures: &mut Vec<Culture>, parent: u32, c: usize, events: &mut Vec<serde_json::Value>| -> u32 {
        let id = cultures.len() as u32 + 1;
        let (region, cont) = home_region(c);
        let lang = if parent > 0 {
            cultures[parent as usize - 1].lang.dialect(seed, 10_000 + id as u64)
        } else {
            names::region_lang(seed, cont, region).dialect(seed, 10_000 + id as u64)
        };
        cultures.push(Culture { parent, founded: tick, ended: None, fate: "", other: 0, lang, region });
        events.push(serde_json::json!({ "year": year, "kind": if parent > 0 { "split" } else { "emerged" }, "culture": id, "other": parent }));
        id
    };

    let mut mapped = vec![0u32; nc];
    let before = cultures.len() as u32;
    if cultures.is_empty() {
        for c in 0..nc {
            mapped[c] = new_culture(cultures, 0, c, events);
        }
    } else {
        // Leading culture of each community, and which community keeps it.
        let lead: Vec<(u32, f64)> = overlap
            .iter()
            .map(|m| m.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap().then(b.0.cmp(a.0))).map_or((0, 0.0), |(&k, &v)| (k, v)))
            .collect();
        let mut keeper: BTreeMap<u32, (f64, usize)> = BTreeMap::new();
        for (c, &(l, v)) in lead.iter().enumerate() {
            let e = keeper.entry(l).or_insert((-1.0, c));
            if v > e.0 {
                *e = (v, c);
            }
        }
        let min_pop = cp.min_culture_share * world;
        let mut next: Vec<Candidate> = Vec::new();
        let mut matched = vec![false; candidates.len()];
        for c in 0..nc {
            let l = lead[c].0;
            mapped[c] = l;
            if keeper[&l].1 == c && l > 0 {
                continue;
            }
            if l > 0 && (cpop[c] < min_pop || members[c].len() < 2) {
                continue;
            }
            // A split in progress: the same parent and mostly the same bands as last time.
            let mut best: Option<(usize, f64)> = None;
            for (j, cand) in candidates.iter().enumerate() {
                if matched[j] || cand.parent != l {
                    continue;
                }
                let inter = intersect(&cand.members, &members[c]);
                let jac = inter as f64 / (cand.members.len() + members[c].len() - inter).max(1) as f64;
                if jac >= 0.5 && best.map_or(true, |(_, bj)| jac > bj) {
                    best = Some((j, jac));
                }
            }
            let streak = match best {
                Some((j, _)) => {
                    matched[j] = true;
                    candidates[j].streak + 1
                }
                None => 1,
            };
            if streak >= persist || l == 0 {
                mapped[c] = new_culture(cultures, l, c, events);
            } else {
                next.push(Candidate { parent: l, members: members[c].clone(), streak });
            }
        }
        *candidates = next;
    }

    // Bands follow their community's culture once it has held long enough
    // (at once for a culture founded at this check).
    let mut moved: BTreeMap<(u32, u32), f64> = BTreeMap::new();
    for b in 0..nb {
        let c = comm[b];
        if c == NONE {
            continue;
        }
        let x = mapped[c as usize];
        let cur = bands.culture[b];
        if cur == x {
            bands.pending[b] = (0, 0);
        } else if x > before || cur == 0 {
            if cur > 0 {
                *moved.entry((cur, x)).or_insert(0.0) += bands.pop[b];
            }
            bands.culture[b] = x;
            bands.pending[b] = (0, 0);
        } else {
            let pend = &mut bands.pending[b];
            *pend = if pend.0 == x { (x, pend.1 + 1) } else { (x, 1) };
            if pend.1 >= persist {
                *moved.entry((cur, x)).or_insert(0.0) += bands.pop[b];
                bands.culture[b] = x;
                bands.pending[b] = (0, 0);
            }
        }
    }
    // Cultures left with no living band end: merged into the culture that took
    // most of their people, or extinct.
    let mut has = vec![false; cultures.len() + 1];
    for b in 0..nb {
        if bands.alive[b] {
            has[bands.culture[b] as usize] = true;
        }
    }
    for (k, cu) in cultures.iter_mut().enumerate() {
        let id = k as u32 + 1;
        if cu.ended.is_some() || has[id as usize] {
            continue;
        }
        cu.ended = Some(tick);
        let into = moved.iter().filter(|((from, _), _)| *from == id).max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map(|((_, to), _)| *to);
        match into {
            Some(to) => {
                cu.fate = "merged";
                cu.other = to;
                events.push(serde_json::json!({ "year": year, "kind": "merged", "culture": id, "other": to }));
            }
            None => {
                cu.fate = "extinct";
                events.push(serde_json::json!({ "year": year, "kind": "extinct", "culture": id, "other": 0 }));
            }
        }
    }
}

fn intersect(a: &[u32], b: &[u32]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

fn empty_output(n: usize) -> StepOutput {
    let mut f = Fields::default();
    f.put("culture", Field::U16(vec![0; n]));
    f.put("culture_group", Field::U16(vec![0; n]));
    f.put("population", Field::F32(vec![0.0; n]));
    StepOutput {
        fields: f,
        meta: serde_json::json!({ "cultures": 0, "groups": 0, "bands": 0, "population": 0, "table": { "cultures": [], "groups": [], "events": [], "provinces": [], "states": [] } }),
    }
}
