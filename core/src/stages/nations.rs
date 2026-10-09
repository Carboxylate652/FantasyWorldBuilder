//! Step 12 — nations and history (Stage 4).
//!
//! Nations grow on the finished culture map, on a clock in years from
//! `start_year` (first polities) to `start_date` (the map's start, 1910–1920).
//!
//! - **Ownership is per province.** A nation's border can cut through a
//!   Stage 2 state, and its land need not be connected: colonies, exclaves
//!   and detached territories are allowed. States are never redrawn.
//! - **Formation:** a province with enough people and no ruler may found a
//!   polity, named after it, ruled by its majority culture.
//! - **Expansion:** each step a nation tries to take frontier provinces. A
//!   target is worth its people and land, and costs more across barriers,
//!   far from the capital and among other cultures (half as much within the
//!   culture group). Empty land is settled; land held by another nation is
//!   fought over, won with the odds of the two nations' strength near it
//!   (people, reach from the capital, defending its own culture).
//! - **Eras:** gunpowder states (cheaper expansion, steadier states), ocean
//!   shipping (colonies across the sea), industry (faster growth, railways).
//! - **Breakups:** large, culturally mixed or spread-out nations may break
//!   apart: the provinces of their largest foreign culture secede, or else
//!   the part farthest from the capital.
//! - **Feedback on culture:** in every ruled province a share of the other
//!   cultures takes the ruler's culture each year, so cultures slowly
//!   converge within borders. Settlers bring the ruler's culture to empty land.
//! - **Railways:** in the industrial era each nation links its largest cities
//!   along the cheapest route over its own land, with stations at the ends,
//!   every few provinces and in dry land (water stops). Stations draw people,
//!   which grows railway towns, also in the desert.
//! - **Cities rise and fall:** each province has an attraction (−1 to +1)
//!   that changes with history. A capital grows into its pull over a few
//!   decades (more for a large nation) and loses it when the capital moves;
//!   a conquered province is sacked and devastated, and the devastation
//!   heals slowly, so a city fought over again and again empties; railway
//!   stations draw people. City pins (directives, by hand or from the AI
//!   guide) add, move or remove attraction anywhere. Attraction scales how
//!   many people a province holds (up to 5×, down to none), and each year a
//!   share of a nation's people moves toward its attractive provinces, more
//!   from devastated or shunned ones: capitals become metropolises,
//!   battlefields become ruins.
//! - **Directives** (by hand or from the AI guide) apply in the year they
//!   were issued, so a steered history replays exactly.

use super::{Ctx, Step, StepOutput};
use crate::directives::{arg_f64, arg_ids, arg_u64, Directive};
use crate::fields::{Field, Fields};
use crate::params::NationParams;
use crate::rng::{stream, Rng};
use crate::vec3::Vec3;
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

mod edge {
    pub const LAND: u8 = 0;
    pub const IMPASSABLE: u8 = 1;
    pub const STRAIT: u8 = 2;
}

struct Prov {
    id: u32,
    land: bool,
    state: u16,
    region: u16,
    continent: u32,
    center: Vec3,
    coastal: bool,
    arid: bool,
    name: String,
    /// People the land can hold before industry, from its habitability.
    land_cap: f64,
}

struct Edge {
    to: u32,
    ty: u8,
    km: f64,
    barrier: f64,
}

#[derive(Clone)]
struct Nation {
    name: String,
    color: [u8; 3],
    capital: usize,
    primary: u32,
    founded: i32,
    ended: Option<i32>,
    fate: String,
    other: u32,
    parent: u32,
    aggression: f64,
    /// Year the current capital became the capital.
    capital_since: i32,
}

/// A city pin: attraction placed by a directive.
#[derive(Clone)]
struct CityPin {
    id: u32,
    province: usize,
    value: f64,
    label: String,
    until: Option<i32>,
    by: String,
    since: i32,
}

#[derive(Clone)]
enum Effect {
    Aggression(u32, f64),
    Toward(u32, usize),
    War(u32, u32),
    Peace(u32, u32),
    Stability(u32, f64),
}

/// The Stage 4 simulation, one step (`years_per_step` years) at a time.
pub struct NationSim {
    np: NationParams,
    r_km: f64,
    provs: Vec<Prov>,
    adj: Vec<Vec<Edge>>,
    index: HashMap<u32, usize>,
    prov_field: Vec<u32>,
    region_names: BTreeMap<u16, String>,
    culture_group: HashMap<u32, u32>,
    culture_names: HashMap<u32, String>,
    coastal_land: Vec<usize>,
    pop: Vec<f64>,
    cap: Vec<f64>,
    shares: Vec<Vec<(u32, f64)>>,
    major: Vec<u32>,
    owner: Vec<u32>,
    nations: Vec<Nation>,
    members: Vec<BTreeSet<usize>>,
    rail: Vec<u8>,
    rail_net: Vec<BTreeSet<usize>>,
    railways: Vec<Value>,
    events: Vec<Value>,
    /// Provinces taken from one nation by another since the last war summary.
    tally: BTreeMap<(u32, u32), u32>,
    rng: Rng,
    /// Year the next step starts.
    pub year: i32,
    directives: Vec<Directive>,
    next_directive: usize,
    effects: Vec<(Effect, i32)>,
    applied: Vec<Value>,
    /// Devastation per province (0 to −1), healing over time.
    devastation: Vec<f64>,
    /// Attraction per province this step (devastation + capital + station + pins).
    attr: Vec<f64>,
    pins: Vec<CityPin>,
    next_pin: u32,
    /// Most people a province ever held, and when.
    peak: Vec<(f64, i32)>,
    ruined: Vec<bool>,
    metropolis: Vec<bool>,
}

/// Capacity multiplier of an attraction: ×5 at +1, ×0 at −1 (as in Stage 3).
fn pull_of(a: f64) -> f64 {
    if a >= 0.0 {
        1.0 + 4.0 * a
    } else {
        (1.0 + a).powi(2)
    }
}

/// People at which a city counts as a metropolis (an event).
const METROPOLIS: f64 = 1_000_000.0;

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

/// Nation colour (same formula as `nationColor` in app/src/layers.ts).
pub fn nation_color(id: u32) -> [u8; 3] {
    hsl(id as f64 * 137.508 + 20.0, 0.5 + 0.15 * (id % 3) as f64, 0.42 + 0.08 * (id % 4) as f64)
}

/// Government by size: city-state, kingdom or empire.
pub fn government(provinces: usize) -> &'static str {
    match provinces {
        0..=3 => "city-state",
        4..=30 => "kingdom",
        _ => "empire",
    }
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let Some(mut sim) = NationSim::new(ctx) else { return empty_output(ctx.grid.len()) };
    let progress = |f: f32, m: &str| ctx.progress(f, m);
    while !sim.done() {
        sim.step(&progress);
    }
    sim.finish()
}

impl NationSim {
    pub fn new(ctx: &Ctx) -> Option<NationSim> {
        let np = ctx.params.nations.clone();
        let seed = ctx.params.planet.seed;
        let r_km = ctx.params.planet.radius_km;
        let ptable = &ctx.input.steps.get(Step::Provinces.index())?.meta["table"];
        let ctable = &ctx.input.steps.get(Step::Cultures.index())?.meta["table"];
        let empty = Vec::new();
        let rows = ptable["provinces"].as_array().unwrap_or(&empty);
        let index: HashMap<u32, usize> = rows.iter().enumerate().map(|(k, p)| (p["id"].as_u64().unwrap_or(0) as u32, k)).collect();
        // Culture-step names and people per province.
        let cprov: HashMap<u64, &Value> = ctable["provinces"].as_array().unwrap_or(&empty).iter().map(|p| (p["id"].as_u64().unwrap_or(0), p)).collect();
        let provs: Vec<Prov> = rows
            .iter()
            .map(|p| {
                let ll = &p["center"];
                let land = matches!(p["kind"].as_str(), Some("land") | Some("wasteland"));
                let area = p["area_km2"].as_f64().unwrap_or(0.0);
                let capf = p["capacity_factor"].as_f64().unwrap_or(0.0);
                let id = p["id"].as_u64().unwrap_or(0);
                Prov {
                    id: id as u32,
                    land,
                    state: p["state"].as_u64().unwrap_or(0) as u16,
                    region: p["region"].as_u64().unwrap_or(0) as u16,
                    continent: p["continent"].as_u64().unwrap_or(0) as u32,
                    center: Vec3::from_lat_lon_deg(ll[0].as_f64().unwrap_or(0.0), ll[1].as_f64().unwrap_or(0.0)),
                    coastal: p["coastal"].as_bool().unwrap_or(false),
                    arid: p["rain_mm"].as_f64().unwrap_or(1000.0) < 300.0,
                    name: cprov.get(&id).and_then(|c| c["name"].as_str()).or_else(|| p["name"].as_str()).unwrap_or("").to_string(),
                    land_cap: if land { 3.0 * ctx.params.cultures.density_per_km2 * area * capf } else { 0.0 },
                }
            })
            .collect();
        let n = provs.len();
        if n == 0 {
            return None;
        }
        let mut adj: Vec<Vec<Edge>> = (0..n).map(|_| Vec::new()).collect();
        for a in ptable["adjacency"].as_array().unwrap_or(&empty) {
            let (Some(&x), Some(&y)) = (index.get(&(a["from"].as_u64().unwrap_or(0) as u32)), index.get(&(a["to"].as_u64().unwrap_or(0) as u32))) else { continue };
            let ty = match a["type"].as_str() {
                Some("land") | Some("river") => edge::LAND,
                Some("impassable") => edge::IMPASSABLE,
                Some("strait") => edge::STRAIT,
                _ => continue,
            };
            let km = if ty == edge::STRAIT { a["crossing_km"].as_f64().unwrap_or(100.0) } else { provs[x].center.angle_to(provs[y].center) * r_km };
            let barrier = a["barrier"].as_f64().unwrap_or(0.0);
            adj[x].push(Edge { to: y as u32, ty, km, barrier });
            adj[y].push(Edge { to: x as u32, ty, km, barrier });
        }
        let mut pop = vec![0.0; n];
        let mut shares: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
        let mut major = vec![0u32; n];
        for (k, p) in provs.iter().enumerate() {
            if let Some(c) = cprov.get(&(p.id as u64)) {
                pop[k] = c["population"].as_f64().unwrap_or(0.0);
                major[k] = c["culture"].as_u64().unwrap_or(0) as u32;
                let sh: Vec<(u32, f64)> = c["shares"].as_array().into_iter().flatten().filter_map(|s| Some((s[0].as_u64()? as u32, s[1].as_f64()?))).collect();
                let total: f64 = sh.iter().map(|x| x.1).sum();
                shares[k] = if total > 0.0 { sh.iter().map(|&(c, v)| (c, v / total)).collect() } else if major[k] > 0 { vec![(major[k], 1.0)] } else { vec![] };
            }
        }
        // Room to grow: settled land can reach about six times its people, or what the land holds.
        let cap: Vec<f64> = (0..n).map(|p| if provs[p].land { (6.0 * pop[p]).max(provs[p].land_cap) } else { 0.0 }).collect();
        let culture_group: HashMap<u32, u32> = ctable["cultures"].as_array().unwrap_or(&empty).iter().map(|c| (c["id"].as_u64().unwrap_or(0) as u32, c["group"].as_u64().unwrap_or(0) as u32)).collect();
        let culture_names: HashMap<u32, String> = ctable["cultures"].as_array().unwrap_or(&empty).iter().map(|c| (c["id"].as_u64().unwrap_or(0) as u32, c["name"].as_str().unwrap_or("").to_string())).collect();
        let region_names: BTreeMap<u16, String> = ptable["regions"].as_array().unwrap_or(&empty).iter().map(|r| (r["id"].as_u64().unwrap_or(0) as u16, r["name"].as_str().unwrap_or("").to_string())).collect();
        let coastal_land: Vec<usize> = (0..n).filter(|&p| provs[p].land && provs[p].coastal).collect();
        Some(NationSim {
            year: np.start_year,
            directives: crate::directives::of_stage(&ctx.edits.overrides.directives, "nations").into_iter().cloned().collect(),
            np,
            r_km,
            adj,
            index,
            prov_field: ctx.input.u32("province").to_vec(),
            region_names,
            culture_group,
            culture_names,
            coastal_land,
            pop,
            cap,
            shares,
            major,
            owner: vec![0; n],
            nations: Vec::new(),
            members: vec![BTreeSet::new()],
            rail: vec![0; n],
            rail_net: vec![BTreeSet::new()],
            railways: Vec::new(),
            events: Vec::new(),
            tally: BTreeMap::new(),
            rng: Rng::new(seed, stream::NATIONS),
            next_directive: 0,
            effects: Vec::new(),
            applied: Vec::new(),
            devastation: vec![0.0; n],
            attr: vec![0.0; n],
            pins: Vec::new(),
            next_pin: 1,
            peak: vec![(0.0, 0); n],
            ruined: vec![false; n],
            metropolis: vec![false; n],
            provs,
        })
    }

    pub fn done(&self) -> bool {
        self.year >= self.np.start_date
    }

    /// Queue a directive issued during a live run (in the order a fresh run
    /// would apply it: by time, then as issued).
    pub fn add_directive(&mut self, d: crate::directives::Directive) {
        let k = (self.next_directive..self.directives.len()).find(|&k| self.directives[k].at > d.at).unwrap_or(self.directives.len());
        self.directives.insert(k, d);
    }

    pub fn start_year(&self) -> i32 {
        self.np.start_year
    }

    pub fn end_year(&self) -> i32 {
        self.np.start_date
    }

    pub fn years_per_step(&self) -> i32 {
        self.np.years_per_step.max(1) as i32
    }

    /// The era a year falls in.
    pub fn era_of(&self, y: i32) -> &'static str {
        if y >= self.np.industrial_year {
            "industry"
        } else if y >= self.np.shipping_year {
            "ocean shipping"
        } else if y >= self.np.gunpowder_year {
            "gunpowder"
        } else {
            "early"
        }
    }

    /// Directives of this run, in the order they apply.
    pub fn directives(&self) -> &[Directive] {
        &self.directives
    }

    fn km(&self, a: usize, b: usize) -> f64 {
        self.provs[a].center.angle_to(self.provs[b].center) * self.r_km
    }

    fn alive(&self, n: u32) -> bool {
        n > 0 && (n as usize) <= self.nations.len() && self.nations[n as usize - 1].ended.is_none()
    }

    fn nation(&self, n: u32) -> &Nation {
        &self.nations[n as usize - 1]
    }

    /// Cultural distance from a nation's ruling culture: 0 same, 0.5 same group, 1 other, 0.3 empty land.
    fn cdist(&self, n: u32, p: usize) -> f64 {
        let (c, prim) = (self.major[p], self.nation(n).primary);
        if c == 0 || prim == 0 {
            0.3
        } else if c == prim {
            0.0
        } else if self.culture_group.get(&c).is_some_and(|g| *g > 0 && self.culture_group.get(&prim) == Some(g)) {
            0.5
        } else {
            1.0
        }
    }

    fn aggression_mult(&self, n: u32) -> f64 {
        self.effects.iter().filter_map(|(e, _)| if let Effect::Aggression(m, f) = e { (*m == n).then_some(*f) } else { None }).product()
    }

    fn stability_mult(&self, n: u32) -> f64 {
        self.effects.iter().filter_map(|(e, _)| if let Effect::Stability(m, f) = e { (*m == n).then_some(*f) } else { None }).product()
    }

    fn toward(&self, n: u32) -> Option<usize> {
        self.effects.iter().rev().find_map(|(e, _)| if let Effect::Toward(m, p) = e { (*m == n).then_some(*p) } else { None })
    }

    fn at_war(&self, a: u32, b: u32) -> bool {
        self.effects.iter().any(|(e, _)| matches!(e, Effect::War(x, y) if *x == a && *y == b))
    }

    fn at_peace(&self, a: u32, b: u32) -> bool {
        self.effects.iter().any(|(e, _)| matches!(e, Effect::Peace(x, y) if (*x == a && *y == b) || (*x == b && *y == a)))
    }

    fn event(&mut self, kind: &str, nation: u32, other: u32, province: Option<usize>, text: String) {
        self.events.push(json!({
            "year": self.year, "event": kind, "nation": nation, "other": other,
            "province": province.map(|p| self.provs[p].id), "text": text,
        }));
    }

    fn culture_name(&self, c: u32) -> String {
        self.culture_names.get(&c).cloned().unwrap_or_else(|| "no".into())
    }

    /// Found a nation in province p (taking it from its owner if any).
    fn found(&mut self, p: usize, name: Option<String>, parent: u32) -> u32 {
        let id = self.nations.len() as u32 + 1;
        let primary = if self.major[p] > 0 { self.major[p] } else { parent.checked_sub(1).map(|k| self.nations[k as usize].primary).unwrap_or(0) };
        let aggression = 0.6 + 0.8 * self.rng.f64();
        self.nations.push(Nation {
            name: name.filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()).unwrap_or_else(|| self.provs[p].name.clone()),
            color: nation_color(id),
            capital: p,
            primary,
            founded: self.year,
            ended: None,
            fate: String::new(),
            other: 0,
            parent,
            aggression,
            capital_since: self.year,
        });
        self.members.push(BTreeSet::new());
        self.rail_net.push(BTreeSet::new());
        let prev = self.owner[p];
        self.take(p, id);
        let (nm, cn) = (self.nations[id as usize - 1].name.clone(), self.culture_name(primary));
        if prev > 0 {
            let old = self.nation(prev).name.clone();
            self.event("independence", id, prev, Some(p), format!("{nm} breaks away from {old} ({cn} culture)"));
        } else {
            self.event("founded", id, 0, Some(p), format!("{nm} is founded ({cn} culture)"));
        }
        id
    }

    /// Province p passes to nation n (0 = no one).
    fn take(&mut self, p: usize, n: u32) {
        let old = self.owner[p];
        if old == n {
            return;
        }
        if old > 0 {
            self.members[old as usize].remove(&p);
        }
        self.owner[p] = n;
        if n > 0 {
            self.members[n as usize].insert(p);
            // Settlers bring the ruler's culture to empty land.
            if self.pop[p] < 1.0 && self.provs[p].land {
                let prim = self.nation(n).primary;
                self.pop[p] = (0.02 * self.cap[p]).max(200.0).min(self.cap[p].max(200.0));
                if prim > 0 {
                    self.shares[p] = vec![(prim, 1.0)];
                    self.major[p] = prim;
                }
            }
        }
        if old > 0 && self.alive(old) && self.nation(old).capital == p {
            // The capital moves to the most populous remaining province.
            let next = self.members[old as usize].iter().copied().max_by(|&a, &b| self.pop[a].partial_cmp(&self.pop[b]).unwrap().then(b.cmp(&a)));
            if let Some(q) = next {
                self.set_capital(old, q);
            }
        }
        if old > 0 && self.alive(old) && self.members[old as usize].is_empty() {
            let k = old as usize - 1;
            self.nations[k].ended = Some(self.year);
            self.nations[k].fate = if n > 0 { "conquered".into() } else { "collapsed".into() };
            self.nations[k].other = n;
            let (a, b) = (self.nations[k].name.clone(), if n > 0 { self.nation(n).name.clone() } else { String::new() });
            self.event("ended", old, n, Some(p), if n > 0 { format!("{a} falls to {b}") } else { format!("{a} collapses") });
        }
    }

    /// Move a nation's capital: the new one grows into its pull from now on.
    fn set_capital(&mut self, n: u32, q: usize) {
        let k = n as usize - 1;
        if self.nations[k].capital == q {
            return;
        }
        self.nations[k].capital = q;
        self.nations[k].capital_since = self.year;
        let (a, b) = (self.nations[k].name.clone(), self.provs[q].name.clone());
        self.event("capital", n, 0, Some(q), format!("{b} becomes the capital of {a}"));
    }

    /// Attraction of every province this step: devastation, capitals (grown
    /// into over `capital_years`, larger for larger nations), railway
    /// stations and city pins.
    fn update_attraction(&mut self) {
        let np = &self.np;
        for p in 0..self.provs.len() {
            self.attr[p] = self.devastation[p] + if self.rail[p] == 2 { 0.15 } else { 0.0 };
        }
        for (k, x) in self.nations.iter().enumerate() {
            if x.ended.is_some() {
                continue;
            }
            let size = self.members[k + 1].len().max(1) as f64;
            let grown = ((self.year - x.capital_since) as f64 / np.capital_years.max(1.0)).clamp(0.0, 1.0);
            let scale = 0.4 + 0.6 * (size.ln() / 30f64.ln()).clamp(0.0, 1.0);
            self.attr[x.capital] += np.capital_pull * grown * scale;
        }
        for pin in &self.pins {
            self.attr[pin.province] += pin.value;
        }
        for a in self.attr.iter_mut() {
            *a = a.clamp(-1.0, 1.0);
        }
    }

    fn pin_rows(&self) -> Vec<Value> {
        self.pins
            .iter()
            .map(|p| {
                let (la, lo) = self.provs[p.province].center.lat_lon();
                json!({ "id": p.id, "province": self.provs[p.province].id, "province_name": self.provs[p.province].name, "value": p.value, "label": p.label,
                        "until": p.until, "since": p.since, "by": p.by, "owner": self.owner[p.province],
                        "at": [(la.to_degrees() * 100.0).round() / 100.0, (lo.to_degrees() * 100.0).round() / 100.0] })
            })
            .collect()
    }

    /// The largest cities (provinces by people), with how they changed.
    fn city_rows(&self, n: usize) -> Vec<Value> {
        let mut order: Vec<usize> = (0..self.provs.len()).filter(|&p| self.provs[p].land && self.pop[p] > 0.0).collect();
        order.sort_by(|&a, &b| self.pop[b].partial_cmp(&self.pop[a]).unwrap().then(a.cmp(&b)));
        let capital_of: HashMap<usize, u32> = self.nations.iter().enumerate().filter(|(_, x)| x.ended.is_none()).map(|(k, x)| (x.capital, k as u32 + 1)).collect();
        order
            .into_iter()
            .take(n)
            .map(|p| {
                json!({ "province": self.provs[p].id, "name": self.provs[p].name, "population": self.pop[p].round() as u64, "owner": self.owner[p],
                        "capital": capital_of.contains_key(&p), "attraction": (self.attr[p] * 100.0).round() / 100.0, "station": self.rail[p] == 2,
                        "peak": self.peak[p].0.round() as u64, "peak_year": self.peak[p].1 })
            })
            .collect()
    }

    fn places(&self, args: &Value) -> Vec<usize> {
        let (pids, states, regions) = (arg_ids(args, "provinces"), arg_ids(args, "states"), arg_ids(args, "regions"));
        (0..self.provs.len())
            .filter(|&p| {
                let pr = &self.provs[p];
                pr.land && (pids.contains(&(pr.id as u64)) || (pr.state > 0 && states.contains(&(pr.state as u64))) || (pr.region > 0 && regions.contains(&(pr.region as u64))))
            })
            .collect()
    }

    fn apply_directives(&mut self) {
        let y = self.year;
        self.effects.retain(|(_, until)| y < *until);
        // A step covers years y .. y + years_per_step: directives dated in it apply now.
        let end = (y + self.np.years_per_step.max(1) as i32) as i64;
        while self.next_directive < self.directives.len() && self.directives[self.next_directive].at < end {
            let d = self.directives[self.next_directive].clone();
            self.next_directive += 1;
            let a = &d.args;
            let until = y + arg_f64(a, "years", 0.0).ceil().max(1.0) as i32;
            let nat = arg_u64(a, "nation").map(|x| x as u32).unwrap_or(0);
            let target = arg_u64(a, "target").map(|x| x as u32).unwrap_or(0);
            let index = &self.index;
            let prov = |key: &str| arg_u64(a, key).and_then(|id| index.get(&(id as u32))).copied();
            let (p_province, p_from, p_to) = (prov("province"), prov("from"), prov("to"));
            let mut ok = true;
            match d.action.as_str() {
                "aggression" if self.alive(nat) => self.effects.push((Effect::Aggression(nat, arg_f64(a, "factor", 1.0)), until)),
                "stability" if self.alive(nat) => self.effects.push((Effect::Stability(nat, arg_f64(a, "factor", 1.0).max(0.05)), until)),
                "expand_toward" if self.alive(nat) => match p_province {
                    Some(p) => self.effects.push((Effect::Toward(nat, p), until)),
                    None => ok = false,
                },
                "war" if self.alive(nat) && self.alive(target) && nat != target => self.effects.push((Effect::War(nat, target), until)),
                "peace" if self.alive(nat) && self.alive(target) && nat != target => self.effects.push((Effect::Peace(nat, target), until)),
                "split" if self.alive(nat) => ok = self.split(nat, arg_u64(a, "culture").map(|c| c as u32)),
                "found" => match p_province {
                    Some(p) if self.provs[p].land => {
                        self.found(p, a["name"].as_str().map(str::to_string), self.owner[p]);
                    }
                    _ => ok = false,
                },
                "union" if self.alive(nat) && self.alive(target) && nat != target => {
                    let list: Vec<usize> = self.members[target as usize].iter().copied().collect();
                    let (an, bn) = (self.nation(nat).name.clone(), self.nation(target).name.clone());
                    for p in list {
                        self.take(p, nat);
                    }
                    let k = target as usize - 1;
                    self.nations[k].fate = "united".into();
                    self.event("union", nat, target, None, format!("{bn} unites with {an}"));
                }
                "transfer" if self.alive(nat) => {
                    for p in self.places(a) {
                        self.take(p, nat);
                    }
                }
                "rename" if self.alive(nat) => {
                    if let Some(name) = a["name"].as_str().filter(|s| !s.trim().is_empty()) {
                        let old = std::mem::replace(&mut self.nations[nat as usize - 1].name, name.trim().to_string());
                        self.event("renamed", nat, 0, None, format!("{old} takes the name {}", name.trim()));
                    }
                }
                "railway" if self.alive(nat) => match (p_from, p_to) {
                    (Some(x), Some(z)) if self.year >= self.np.industrial_year => ok = self.build_line(nat, x, &[z].into_iter().collect()).is_some(),
                    _ => ok = false,
                },
                "catastrophe" => {
                    let sev = arg_f64(a, "severity", 0.0).clamp(0.0, 0.95);
                    for p in self.places(a) {
                        self.pop[p] *= 1.0 - sev;
                    }
                }
                "pin_add" => match p_province.filter(|&p| self.provs[p].land) {
                    Some(p) => {
                        let id = self.next_pin;
                        self.next_pin += 1;
                        let until = a["years"].as_f64().map(|yrs| y + yrs.ceil().max(1.0) as i32);
                        let label = a["label"].as_str().unwrap_or("").trim().to_string();
                        self.pins.push(CityPin { id, province: p, value: arg_f64(a, "value", 0.0).clamp(-1.0, 1.0), label, until, by: d.by.clone(), since: y });
                    }
                    None => ok = false,
                },
                "pin_move" => {
                    let pid = arg_u64(a, "pin").unwrap_or(0) as u32;
                    match (self.pins.iter_mut().find(|x| x.id == pid), p_province.filter(|&p| self.provs[p].land)) {
                        (Some(pin), Some(p)) => {
                            pin.province = p;
                            pin.since = y;
                        }
                        _ => ok = false,
                    }
                }
                "pin_remove" => {
                    let pid = arg_u64(a, "pin").unwrap_or(0) as u32;
                    let before = self.pins.len();
                    self.pins.retain(|x| x.id != pid);
                    ok = self.pins.len() < before;
                }
                "move_capital" if self.alive(nat) => match p_province.filter(|&p| self.owner[p] == nat) {
                    Some(p) => self.set_capital(nat, p),
                    None => ok = false,
                },
                "note" => {}
                _ => ok = false,
            }
            self.applied.push(json!({ "year": y, "action": d.action, "args": d.args, "note": d.note, "by": d.by, "applied": ok }));
        }
    }

    /// Breakup: the provinces of one culture (given, or the largest foreign
    /// one), or else the part farthest from the capital, form a new nation.
    fn split(&mut self, n: u32, culture: Option<u32>) -> bool {
        let members: Vec<usize> = self.members[n as usize].iter().copied().collect();
        if members.len() < 2 {
            return false;
        }
        let cap = self.nation(n).capital;
        let prim = self.nation(n).primary;
        let mut by_cult: BTreeMap<u32, f64> = BTreeMap::new();
        let total: f64 = members.iter().map(|&p| self.pop[p]).sum::<f64>().max(1.0);
        for &p in &members {
            *by_cult.entry(self.major[p]).or_insert(0.0) += self.pop[p];
        }
        let foreign = culture.filter(|c| *c != prim).or_else(|| {
            by_cult.iter().filter(|(&c, &v)| c > 0 && c != prim && v / total >= 0.15).max_by(|a, b| a.1.partial_cmp(b.1).unwrap().then(b.0.cmp(a.0))).map(|(&c, _)| c)
        });
        let mut set: Vec<usize> = match foreign {
            Some(c) => members.iter().copied().filter(|&p| p != cap && self.major[p] == c).collect(),
            None => vec![],
        };
        if set.is_empty() {
            let far = members.iter().copied().max_by(|&a, &b| self.km(a, cap).partial_cmp(&self.km(b, cap)).unwrap().then(b.cmp(&a))).unwrap();
            set = members.iter().copied().filter(|&p| p != cap && self.km(p, far) < self.km(p, cap)).collect();
        }
        if set.is_empty() || set.len() >= members.len() {
            return false;
        }
        let capital = set.iter().copied().max_by(|&a, &b| self.pop[a].partial_cmp(&self.pop[b]).unwrap().then(b.cmp(&a))).unwrap();
        let id = self.found(capital, None, n);
        for p in set {
            self.take(p, id);
        }
        true
    }

    /// One expansion attempt by nation n.
    fn expand(&mut self, n: u32, gun: bool, ship: bool) {
        let cap = self.nation(n).capital;
        let members: Vec<usize> = self.members[n as usize].iter().copied().collect();
        if members.is_empty() {
            return;
        }
        let bw = self.np.barrier_weight;
        let reach = self.np.reach_km.max(50.0);
        let toward = self.toward(n);
        // Large nations probe a sample of their border.
        let probe: Vec<usize> = if members.len() > 48 { (0..48).map(|_| members[self.rng.below(members.len())]).collect() } else { members.clone() };
        let mut best: Option<(f64, usize)> = None;
        for &p in &probe {
            for e in &self.adj[p] {
                let q = e.to as usize;
                let o = self.owner[q];
                // Barren land that holds no one (ice caps) is left unclaimed.
                if o == n || !self.provs[q].land || (o > 0 && self.at_peace(n, o)) || (self.cap[q] < 1.0 && self.pop[q] < 1.0) {
                    continue;
                }
                let step = match e.ty {
                    edge::LAND => e.km * (1.0 + bw * e.barrier),
                    edge::IMPASSABLE => 3.0 * e.km * (1.0 + bw * e.barrier),
                    _ => 2.0 * e.km + 150.0,
                };
                let value = (self.pop[q] + 0.1 * self.cap[q]).max(1.0).sqrt();
                let mut cost = (1.0 + step / 300.0) * (1.0 + self.km(cap, q) / reach) * (1.0 + self.np.culture_weight * self.cdist(n, q));
                if o > 0 {
                    cost *= if self.at_war(n, o) { 0.6 } else { 2.0 };
                }
                if gun {
                    cost *= 0.8;
                }
                let mut s = value / cost * (0.5 + self.rng.f64());
                if let Some(t) = toward {
                    s *= 3.0 / (1.0 + self.km(q, t) / 800.0);
                }
                if best.map_or(true, |b| s > b.0) {
                    best = Some((s, q));
                }
            }
        }
        // Colonies across the sea.
        if ship && self.provs[cap].coastal || ship && members.iter().any(|&p| self.provs[p].coastal) {
            if self.rng.f64() < 0.3 && !self.coastal_land.is_empty() {
                let ports: Vec<usize> = members.iter().copied().filter(|&p| self.provs[p].coastal).take(12).collect();
                let own_pop: f64 = members.iter().map(|&p| self.pop[p]).sum();
                for _ in 0..12 {
                    let q = self.coastal_land[self.rng.below(self.coastal_land.len())];
                    let o = self.owner[q];
                    // Free land, or land of a much weaker nation (colonial conquest).
                    if o == n || (o > 0 && (self.at_peace(n, o) || self.members[o as usize].iter().map(|&p| self.pop[p]).sum::<f64>() > 0.25 * own_pop)) {
                        continue;
                    }
                    let d = ports.iter().map(|&p| self.km(p, q)).fold(f64::INFINITY, f64::min);
                    if d > self.np.overseas_km {
                        continue;
                    }
                    let mut s = (self.pop[q] + 0.1 * self.cap[q]).max(1.0).sqrt() / (1.0 + d / 1500.0) * (0.5 + self.rng.f64());
                    if o > 0 {
                        s *= 0.5;
                    }
                    if let Some(t) = toward {
                        s *= 3.0 / (1.0 + self.km(q, t) / 800.0);
                    }
                    if best.map_or(true, |b| s > b.0) {
                        best = Some((s, q));
                    }
                }
            }
        }
        let Some((_, q)) = best else { return };
        let o = self.owner[q];
        let colony = self.provs[q].continent != self.provs[cap].continent && !self.members[n as usize].iter().any(|&p| self.provs[p].continent == self.provs[q].continent);
        if o == 0 {
            if self.rng.f64() < if gun { 0.8 } else { 0.55 } {
                self.take(q, n);
                if colony {
                    let (a, b) = (self.nation(n).name.clone(), self.provs[q].name.clone());
                    self.event("colony", n, 0, Some(q), format!("{a} founds a colony at {b}"));
                }
            }
            return;
        }
        // War over q: the two nations' strength near it.
        let strength = |s: &NationSim, m: u32| {
            let total: f64 = s.members[m as usize].iter().map(|&p| s.pop[p]).sum();
            let near = 1.0 / (1.0 + s.km(s.nation(m).capital, q) / reach);
            total.max(1.0).powf(0.8) * near
        };
        let mut att = strength(self, n);
        let mut def = strength(self, o);
        if self.major[q] != 0 && self.major[q] == self.nation(o).primary {
            def *= 1.3;
        }
        if self.at_war(n, o) {
            att *= 1.5;
        }
        if self.rng.f64() < att / (att + def) {
            let capital = self.nation(o).capital == q && self.members[o as usize].len() > 1;
            let (a, b, c) = (self.nation(n).name.clone(), self.nation(o).name.clone(), self.provs[q].name.clone());
            self.take(q, n);
            // The sack: people lost now, and devastation that drives more away until it heals.
            self.pop[q] *= 1.0 - (self.np.war_sack * if capital { 4.0 } else { 1.0 }).clamp(0.0, 0.9);
            self.devastation[q] = (self.devastation[q] - self.np.war_devastation).max(-1.0);
            *self.tally.entry((n, o)).or_insert(0) += 1;
            if capital {
                self.event("conquest", n, o, Some(q), format!("{a} takes {b}'s capital {c}"));
            } else if colony {
                self.event("colony", n, o, Some(q), format!("{a} takes {c} from {b} as a colony"));
            }
        }
    }

    /// Cheapest route over nation n's land from `from` to any province in
    /// `to`, then a railway along it. Returns the line's index.
    fn build_line(&mut self, n: u32, from: usize, to: &BTreeSet<usize>) -> Option<usize> {
        if to.contains(&from) || self.owner[from] != n {
            return None;
        }
        let bw = self.np.barrier_weight;
        let mut dist: HashMap<usize, f64> = HashMap::new();
        let mut prev: HashMap<usize, usize> = HashMap::new();
        let mut heap = BinaryHeap::new();
        dist.insert(from, 0.0);
        heap.push(Reverse((0u64, from)));
        let mut end = None;
        while let Some(Reverse((bits, p))) = heap.pop() {
            let d = f64::from_bits(bits);
            if d > dist[&p] {
                continue;
            }
            if to.contains(&p) {
                end = Some(p);
                break;
            }
            for e in &self.adj[p] {
                let q = e.to as usize;
                if self.owner[q] != n || e.ty == edge::STRAIT {
                    continue;
                }
                let nd = d + e.km * (1.0 + bw * e.barrier) * if e.ty == edge::IMPASSABLE { 2.0 } else { 1.0 };
                if dist.get(&q).map_or(true, |&x| nd < x) {
                    dist.insert(q, nd);
                    prev.insert(q, p);
                    heap.push(Reverse((nd.to_bits(), q)));
                }
            }
        }
        let end = end?;
        let mut path = vec![end];
        while let Some(&p) = prev.get(path.last().unwrap()) {
            path.push(p);
        }
        path.reverse();
        let last = path.len() - 1;
        let mut stations = Vec::new();
        for (k, &p) in path.iter().enumerate() {
            let station = k == 0 || k == last || k % 5 == 0 || self.provs[p].arid;
            if station && self.rail[p] < 2 {
                self.rail[p] = 2;
                self.cap[p] += self.np.station_people;
            } else if self.rail[p] == 0 {
                self.rail[p] = 1;
                self.cap[p] += 0.3 * self.np.station_people;
            }
            if station {
                stations.push(self.provs[p].id);
            }
            self.rail_net[n as usize].insert(p);
        }
        let id = self.railways.len() + 1;
        self.railways.push(json!({
            "id": id, "owner": n, "opened": self.year,
            "provinces": path.iter().map(|&p| self.provs[p].id).collect::<Vec<_>>(),
            "stations": stations,
        }));
        Some(id - 1)
    }

    /// A nation's railway project: link its largest city not yet on its
    /// network to the network (the capital starts it).
    fn railway_project(&mut self, n: u32) {
        let members: Vec<usize> = self.members[n as usize].iter().copied().collect();
        if members.len() < 4 {
            return;
        }
        let mut cities: Vec<usize> = members.iter().copied().filter(|&p| self.pop[p] >= self.np.found_population).collect();
        cities.sort_by(|&a, &b| self.pop[b].partial_cmp(&self.pop[a]).unwrap().then(a.cmp(&b)));
        cities.truncate(self.np.railway_cities.max(2) as usize);
        let cap = self.nation(n).capital;
        let net: BTreeSet<usize> = self.rail_net[n as usize].iter().copied().filter(|&p| self.owner[p] == n).collect();
        let net = if net.is_empty() { [cap].into_iter().collect() } else { net };
        for &c in &cities {
            if !net.contains(&c) && self.build_line(n, c, &net).is_some() {
                let (a, b) = (self.nation(n).name.clone(), self.provs[c].name.clone());
                if self.railways.len() == 1 || self.rng.f64() < 0.15 {
                    self.event("railway", n, 0, Some(c), format!("{a} opens a railway to {b}"));
                }
                return;
            }
        }
    }

    /// Run one step of `years_per_step` years.
    pub fn step(&mut self, progress: &dyn Fn(f32, &str)) {
        if self.done() {
            return;
        }
        self.apply_directives();
        let y = self.year;
        let dt = self.np.years_per_step.max(1) as f64;
        let (gun, ship, ind) = (y >= self.np.gunpowder_year, y >= self.np.shipping_year, y >= self.np.industrial_year);
        let span = (self.np.start_date - self.np.start_year).max(1) as f32;
        if (y - self.np.start_year) % 50 < self.np.years_per_step as i32 {
            progress(((y - self.np.start_year) as f32 / span).clamp(0.0, 1.0) * 0.95, &format!("Year {y}: {} nations", self.nations.iter().filter(|x| x.ended.is_none()).count()));
        }
        let n = self.provs.len();
        // City pins that have run their time.
        self.pins.retain(|x| x.until.map_or(true, |u| y < u));
        self.update_attraction();
        // 1. growth: attraction scales how many people a province holds.
        let rate = self.np.growth + if ind { self.np.industrial_growth } else { 0.0 };
        for p in 0..n {
            if self.pop[p] > 0.0 {
                // Devastation stops growth (and pushes people out, below) but
                // leaves the land able to hold them once it heals.
                let k = self.cap[p] * if ind { 2.0 } else { 1.0 } * pull_of((self.attr[p] - self.devastation[p]).clamp(-1.0, 1.0));
                let r = rate * (1.0 + 0.5 * self.devastation[p]).max(0.0);
                if k > 1.0 {
                    self.pop[p] = (self.pop[p] + self.pop[p] * r * dt * (1.0 - self.pop[p] / k).max(if self.pop[p] > k { -1.0 } else { 0.0 })).max(0.0);
                } else {
                    // Nothing holds people here any more: they leave.
                    self.pop[p] *= 0.9f64.powf(dt);
                }
            }
        }
        // Migration within each nation toward its attractive provinces, as far
        // as they have room; devastated or shunned provinces send more.
        if self.np.migration > 0.0 {
            let m = (self.np.migration * dt).clamp(0.0, 0.5);
            let ind_mult = if ind { 2.0 } else { 1.0 };
            for k in 1..=self.nations.len() {
                if self.nations[k - 1].ended.is_some() {
                    continue;
                }
                let members: Vec<usize> = self.members[k].iter().copied().collect();
                let room: Vec<f64> = members.iter().map(|&p| if self.attr[p] > 0.0 { (self.cap[p] * ind_mult * pull_of((self.attr[p] - self.devastation[p]).clamp(-1.0, 1.0)) - self.pop[p]).max(0.0) } else { 0.0 }).collect();
                let total_room: f64 = room.iter().sum();
                if total_room <= 0.0 {
                    continue;
                }
                let want: Vec<f64> = members.iter().map(|&p| if self.attr[p] <= 0.0 { self.pop[p] * (m * (1.0 + 4.0 * (-self.attr[p]))).min(0.5) } else { 0.0 }).collect();
                let total_want: f64 = want.iter().sum();
                if total_want <= 0.0 {
                    continue;
                }
                let scale = (total_room / total_want).min(1.0);
                let moved = total_want * scale;
                for (i, &p) in members.iter().enumerate() {
                    self.pop[p] += room[i] / total_room * moved - want[i] * scale;
                }
            }
        }
        // Devastation heals; peaks, metropolises and ruins.
        let heal = (1.0 - self.np.recovery * dt).clamp(0.0, 1.0);
        let fp0 = self.np.found_population.max(1.0);
        for p in 0..n {
            self.devastation[p] *= heal;
            if self.pop[p] > self.peak[p].0 {
                self.peak[p] = (self.pop[p], y);
            }
            if self.ruined[p] && self.pop[p] > 0.6 * self.peak[p].0 {
                // Rebuilt.
                self.ruined[p] = false;
            }
            if !self.metropolis[p] && self.pop[p] >= METROPOLIS {
                self.metropolis[p] = true;
                let (o, name) = (self.owner[p], self.provs[p].name.clone());
                self.event("metropolis", o, 0, Some(p), format!("{name} passes a million people"));
            }
            if !self.ruined[p] && self.peak[p].0 >= 3.0 * fp0 && self.pop[p] < 0.25 * self.peak[p].0 {
                self.ruined[p] = true;
                let (o, name) = (self.owner[p], self.provs[p].name.clone());
                self.event("ruined", o, 0, Some(p), format!("{name} lies abandoned: {} people of {} at its height in {}", self.pop[p].round(), self.peak[p].0.round(), self.peak[p].1));
            }
        }
        // 2. new polities
        let fp = self.np.found_population.max(1.0);
        for p in 0..n {
            if self.owner[p] == 0 && self.provs[p].land && self.pop[p] >= fp {
                let chance = self.np.found_rate * dt * (self.pop[p] / fp).min(4.0) * if ship { 0.3 } else { 1.0 };
                if self.rng.f64() < chance {
                    self.found(p, None, 0);
                }
            }
        }
        // 3. expansion
        let living: Vec<u32> = (1..=self.nations.len() as u32).filter(|&m| self.alive(m)).collect();
        for &m in &living {
            if !self.alive(m) {
                continue;
            }
            let x = self.np.expansion_rate * self.nation(m).aggression * self.aggression_mult(m) * dt / 2.0;
            let tries = x.floor() as usize + usize::from(self.rng.f64() < x.fract());
            for _ in 0..tries {
                if self.alive(m) {
                    self.expand(m, gun, ship);
                }
            }
        }
        // 4. breakups
        for &m in &living {
            if !self.alive(m) || self.members[m as usize].len() < 4 {
                continue;
            }
            let members: Vec<usize> = self.members[m as usize].iter().copied().collect();
            let cap = self.nation(m).capital;
            let total: f64 = members.iter().map(|&p| self.pop[p]).sum::<f64>().max(1.0);
            let foreign: f64 = members.iter().map(|&p| self.pop[p] * self.cdist(m, p)).sum::<f64>() / total;
            let size = (members.len() as f64 / 60.0).min(3.0);
            let spread = members.iter().map(|&p| self.km(p, cap)).sum::<f64>() / members.len() as f64 / (2.0 * self.np.reach_km.max(50.0));
            let instability = 1.2 * foreign + 0.4 * size + 0.4 * spread.min(3.0);
            let era = if ind { 0.5 } else if gun { 0.7 } else { 1.0 };
            let chance = self.np.collapse_rate * dt / 2.0 * instability * instability * era / self.stability_mult(m).max(0.05);
            if self.rng.f64() < chance {
                self.split(m, None);
            }
        }
        // 5. culture feedback
        let a = (self.np.assimilation * dt).clamp(0.0, 1.0);
        if a > 0.0 {
            for p in 0..n {
                let o = self.owner[p];
                if o == 0 || self.pop[p] <= 0.0 {
                    continue;
                }
                let prim = self.nation(o).primary;
                if prim == 0 {
                    continue;
                }
                let sh = &mut self.shares[p];
                let s0 = sh.iter().find(|x| x.0 == prim).map_or(0.0, |x| x.1);
                if s0 >= 1.0 {
                    continue;
                }
                let s1 = s0 + a * (1.0 - s0);
                let scale = (1.0 - s1) / (1.0 - s0).max(1e-12);
                for x in sh.iter_mut() {
                    x.1 = if x.0 == prim { s1 } else { x.1 * scale };
                }
                if s0 == 0.0 {
                    sh.push((prim, s1));
                }
                sh.retain(|x| x.1 >= 0.005);
                self.major[p] = sh.iter().max_by(|x, y| x.1.partial_cmp(&y.1).unwrap().then(y.0.cmp(&x.0))).map_or(0, |x| x.0);
            }
        }
        // 6. railways
        if ind {
            let every = (self.np.railway_every_years.max(1) as f64 / dt).round().max(1.0) as i32;
            let k = ((y - self.np.industrial_year) as f64 / dt).round() as i32;
            let living: Vec<u32> = (1..=self.nations.len() as u32).filter(|&m| self.alive(m)).collect();
            for m in living {
                if (k + m as i32) % every == 0 {
                    self.railway_project(m);
                }
            }
        }
        self.year += self.np.years_per_step.max(1) as i32;
        if (self.year - self.np.start_year) % 50 < self.np.years_per_step as i32 || self.done() {
            self.flush_wars();
        }
    }

    /// Summarise the conquests since the last summary as wars ("A took 7
    /// provinces from B, 1500–1550"), largest first.
    fn flush_wars(&mut self) {
        let mut wars: Vec<((u32, u32), u32)> = std::mem::take(&mut self.tally).into_iter().filter(|x| x.1 >= 3).collect();
        wars.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let from = self.year - 50;
        for ((a, b), k) in wars.into_iter().take(12) {
            let (an, bn) = (self.nation(a).name.clone(), self.nation(b).name.clone());
            self.event("war", a, b, None, format!("{an} takes {k} provinces from {bn} ({from}–{})", self.year));
        }
    }

    fn nation_rows(&self, living_only: bool) -> Vec<Value> {
        let mut out = Vec::new();
        for (k, x) in self.nations.iter().enumerate() {
            let id = k as u32 + 1;
            if living_only && x.ended.is_some() {
                continue;
            }
            let m = &self.members[id as usize];
            let pop: f64 = m.iter().map(|&p| self.pop[p]).sum();
            let cont = self.provs[x.capital].continent;
            let overseas = m.iter().filter(|&&p| self.provs[p].continent != cont).count();
            let mut regs: BTreeMap<u16, usize> = BTreeMap::new();
            for &p in m {
                *regs.entry(self.provs[p].region).or_insert(0) += 1;
            }
            let mut regs: Vec<(u16, usize)> = regs.into_iter().collect();
            regs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            out.push(json!({
                "id": id, "name": x.name, "government": if x.ended.is_some() { "" } else { government(m.len()) }, "color": x.color,
                "capital": self.provs[x.capital].id, "capital_name": self.provs[x.capital].name,
                "primary_culture": x.primary, "culture_name": self.culture_name(x.primary),
                "provinces": m.len(), "population": pop.max(0.0).round() as u64, "overseas_provinces": overseas,
                "founded": x.founded, "ended": x.ended, "fate": x.fate, "fate_other": x.other, "parent": x.parent,
                "railway_provinces": self.rail_net[id as usize].iter().filter(|&&p| self.owner[p] == id).count(),
                "regions": regs.iter().take(4).map(|(r, c)| json!({ "id": r, "name": self.region_names.get(r).cloned().unwrap_or_default(), "provinces": c })).collect::<Vec<_>>(),
            }));
        }
        out
    }

    fn fields(&self) -> Fields {
        let n = self.prov_field.len();
        let mut f_owner = vec![0u16; n];
        let mut f_cult = vec![0u16; n];
        let mut f_rail = vec![0u8; n];
        let mut f_pop = vec![0f32; n];
        // Province area from the cell count is not needed: density uses the cells' share.
        let mut cells = vec![0u32; self.provs.len()];
        for i in 0..n {
            if let Some(&p) = self.index.get(&self.prov_field[i]) {
                cells[p] += 1;
            }
        }
        for i in 0..n {
            let Some(&p) = self.index.get(&self.prov_field[i]) else { continue };
            f_owner[i] = self.owner[p].min(u16::MAX as u32) as u16;
            f_cult[i] = self.major[p].min(u16::MAX as u32) as u16;
            f_rail[i] = self.rail[p];
            if self.provs[p].land {
                f_pop[i] = (self.pop[p] / cells[p].max(1) as f64) as f32;
            }
        }
        let mut f = Fields::default();
        f.put("owner", Field::U16(f_owner));
        f.put("nation_culture", Field::U16(f_cult));
        f.put("railway", Field::U8(f_rail));
        f.put("nation_population", Field::F32(f_pop));
        let f_attr: Vec<f32> = (0..n).map(|i| self.index.get(&self.prov_field[i]).map_or(0.0, |&p| self.attr[p] as f32)).collect();
        f.put("nation_attraction", Field::F32(f_attr));
        f
    }

    /// The world so far, for the live view and the AI guide.
    pub fn snapshot(&self) -> (Value, Fields) {
        let land = (0..self.provs.len()).filter(|&p| self.provs[p].land).count();
        let ruled = (0..self.provs.len()).filter(|&p| self.provs[p].land && self.owner[p] > 0).count();
        let mut nations = self.nation_rows(true);
        nations.sort_by(|a, b| b["population"].as_f64().partial_cmp(&a["population"].as_f64()).unwrap().then(a["id"].as_u64().cmp(&b["id"].as_u64())));
        // Regions: who holds them, and their most populous province (a target for directives).
        let mut regions: BTreeMap<u16, (f64, usize, BTreeMap<u32, usize>)> = BTreeMap::new();
        for p in 0..self.provs.len() {
            if !self.provs[p].land || self.provs[p].region == 0 {
                continue;
            }
            let e = regions.entry(self.provs[p].region).or_insert((0.0, p, BTreeMap::new()));
            e.0 += self.pop[p];
            if self.pop[p] > self.pop[e.1] {
                e.1 = p;
            }
            *e.2.entry(self.owner[p]).or_insert(0) += 1;
        }
        let regions: Vec<Value> = regions
            .iter()
            .map(|(r, (pop, top, owners))| {
                let mut o: Vec<(u32, usize)> = owners.iter().map(|(&k, &v)| (k, v)).collect();
                o.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                json!({ "id": r, "name": self.region_names.get(r).cloned().unwrap_or_default(), "population": pop.round(),
                        "main_province": self.provs[*top].id, "main_province_name": self.provs[*top].name,
                        "owners": o.iter().take(3).map(|x| x.0).collect::<Vec<_>>() })
            })
            .collect();
        let effects: Vec<Value> = self
            .effects
            .iter()
            .map(|(e, until)| match e {
                Effect::Aggression(n, f) => json!({ "effect": "aggression", "nation": n, "factor": f, "until": until }),
                Effect::Toward(n, p) => json!({ "effect": "expand_toward", "nation": n, "province": self.provs[*p].id, "until": until }),
                Effect::War(a, b) => json!({ "effect": "war", "nation": a, "target": b, "until": until }),
                Effect::Peace(a, b) => json!({ "effect": "peace", "nation": a, "target": b, "until": until }),
                Effect::Stability(n, f) => json!({ "effect": "stability", "nation": n, "factor": f, "until": until }),
            })
            .collect();
        let era = if self.year >= self.np.industrial_year {
            "industry"
        } else if self.year >= self.np.shipping_year {
            "ocean shipping"
        } else if self.year >= self.np.gunpowder_year {
            "gunpowder"
        } else {
            "early"
        };
        let summary = json!({
            "stage": "nations",
            "year": self.year, "start_year": self.np.start_year, "end_year": self.np.start_date, "era": era,
            "population": self.pop.iter().sum::<f64>().round(),
            "land_provinces": land, "ruled_provinces": ruled,
            "nations": nations, "nations_ever": self.nations.len(),
            "regions": regions, "effects": effects,
            "railways": self.railways.len(),
            "pins": self.pin_rows(),
            "cities": self.city_rows(20),
            "ruins": self.ruined.iter().filter(|&&r| r).count(),
            "events": self.events.iter().rev().take(20).rev().cloned().collect::<Vec<_>>(),
            "directives": self.applied,
            "queued": self.directives[self.next_directive..].iter().map(|d| json!({ "year": d.at, "action": d.action, "args": d.args, "note": d.note, "by": d.by })).collect::<Vec<_>>(),
        });
        (summary, self.fields())
    }

    pub fn finish(self) -> StepOutput {
        let provinces: Vec<Value> = (0..self.provs.len())
            .filter(|&p| self.provs[p].land)
            .map(|p| {
                let sh: Vec<Value> = self.shares[p].iter().filter(|x| x.1 >= 0.05).map(|x| json!([x.0, (x.1 * 1000.0).round() / 1000.0])).collect();
                json!({ "id": self.provs[p].id, "owner": self.owner[p], "culture": self.major[p], "shares": sh, "population": self.pop[p].max(0.0).round() as u64, "railway": self.rail[p] })
            })
            .collect();
        let nations = self.nation_rows(false);
        let living = self.nations.iter().filter(|x| x.ended.is_none()).count();
        let ruled = (0..self.provs.len()).filter(|&p| self.provs[p].land && self.owner[p] > 0).count();
        let land = (0..self.provs.len()).filter(|&p| self.provs[p].land).count();
        let count = |k: &str| self.events.iter().filter(|e| e["event"] == k).count();
        StepOutput {
            fields: self.fields(),
            meta: json!({
                "nations": living,
                "nations_ever": self.nations.len(),
                "start_year": self.np.start_year,
                "start_date": self.np.start_date,
                "ruled_share": ruled as f64 / land.max(1) as f64,
                "population": self.pop.iter().sum::<f64>().round(),
                "railways": self.railways.len(),
                "railway_provinces": self.rail.iter().filter(|&&r| r > 0).count(),
                "conquests": count("conquest"),
                "independences": count("independence"),
                "colonies": count("colony"),
                "metropolises": self.metropolis.iter().filter(|&&m| m).count(),
                "ruins": self.ruined.iter().filter(|&&r| r).count(),
                "directives": self.applied,
                "table": {
                    "nations": nations,
                    "provinces": provinces,
                    "events": self.events,
                    "railways": self.railways,
                    "cities": self.city_rows(100),
                    "pins": self.pin_rows(),
                    "ruins": (0..self.provs.len()).filter(|&p| self.ruined[p]).map(|p| json!({ "province": self.provs[p].id, "name": self.provs[p].name, "population": self.pop[p].round() as u64, "peak": self.peak[p].0.round() as u64, "peak_year": self.peak[p].1 })).collect::<Vec<_>>(),
                },
            }),
        }
    }
}

fn empty_output(n: usize) -> StepOutput {
    let mut f = Fields::default();
    f.put("owner", Field::U16(vec![0; n]));
    f.put("nation_culture", Field::U16(vec![0; n]));
    f.put("railway", Field::U8(vec![0; n]));
    f.put("nation_population", Field::F32(vec![0.0; n]));
    f.put("nation_attraction", Field::F32(vec![0.0; n]));
    StepOutput { fields: f, meta: json!({ "nations": 0, "nations_ever": 0, "table": { "nations": [], "provinces": [], "events": [], "railways": [] } }) }
}
