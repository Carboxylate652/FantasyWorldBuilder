//! Step 12 — nations and history (Stage 4).
//!
//! Nations grow on the finished culture map, on a clock in years from
//! `start_year` (first polities) to `start_date` (the map's start, by default
//! 1949: late in the second great war's era, early in the cold war's).
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
//! - **Technology and eras, per nation:** each nation's technology (in
//!   years) advances faster when it is rich per head and large, and spreads
//!   from more advanced neighbours; the leader can run a few years ahead of
//!   the calendar. A nation enters an era when most of its provinces have
//!   embraced the era's institution (below); its technology cannot pass the
//!   next era's year until then:
//!   gunpowder (cheaper expansion, steadier states), ocean shipping (colonies
//!   across the sea), industry (faster growth, railways), synthetic
//!   fertilizer (farmland holds more people, phased in over 20 years), the
//!   motor age (highways) and the air age (airports).
//! - **Economy:** output is people × integration × productivity (by
//!   technology and era); a share is taxed into the treasury, which pays the
//!   army and the building and upkeep of roads, railways and airports. A
//!   nation deep in debt goes bankrupt: it closes its newest line, lets a
//!   road decay, writes off half its debt and is less stable for a while.
//! - **Access:** the cheapest travel cost from the capital to each province
//!   over the nation's roads, railways (with the time lost changing lines at
//!   junctions), sea lanes and air routes. Well-connected provinces are
//!   integrated: they pay more taxes, take the ruler's culture sooner, draw
//!   migrants, and the nation projects power over them (reach in expansion
//!   and war, and less spread-out instability).
//! - **Roads** (as in the classical empires): track, paved road and highway
//!   (motor age). Each nation links its cities to its capital as it can
//!   afford; better roads cost more to build and to keep, and roads no one
//!   keeps decay.
//! - **Breakups:** large, culturally mixed or spread-out nations may break
//!   apart: the provinces of their largest foreign culture secede, or else
//!   the part farthest from the capital.
//! - **Feedback on culture:** in every ruled province a share of the other
//!   cultures takes the ruler's culture each year, so cultures slowly
//!   converge within borders. Settlers bring the ruler's culture to empty land.
//! - **Railways:** in the industrial era each nation links its largest cities
//!   along the cheapest route over its own land, as lines with stations at
//!   the ends, at cities, every few provinces and in dry land (water stops).
//!   A line that reaches the end of another extends it; one that meets
//!   another elsewhere makes a junction station there, where passengers and
//!   freight change lines (a time penalty). Track and stations cost money to
//!   build and keep (paid by the provinces' owners). Stations draw people,
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
//! - **Time steps** shorten toward the present (by default 2 years until
//!   1400, 1 year until 1800, half a year after), so recent centuries get
//!   finer detail.
//! - **Ports** (`nations/ports.rs`): in the ocean-shipping era, coastal
//!   provinces with a good harbour (calm winds, deep water, shelter, a river
//!   mouth, no winter ice) and enough people around become ports; sea lanes,
//!   colonies and institutions cross the sea through them.
//! - **Institutions** (`nations/institutions.rs`): each era's idea is born in
//!   one province (chosen by the user or the guide, or by chance among the
//!   candidates) and spreads over land, roads, railways, ports and airports;
//!   a nation enters the era once most of its provinces have embraced it.
//!   Nations far behind a neighbour may reform on its model.
//! - **Tags and feudal empires** (`nations/feudal.rs`): directives tag the
//!   world, states or nations with special rules; a feudal empire enfeoffs
//!   kings and dukes over its land and lasts until it is dissolved.
//! - **Directives** (by hand or from the AI guide) apply in the step whose
//!   time span contains the moment they were issued, so a steered history
//!   replays exactly.

use super::{Ctx, Step, StepOutput};
use crate::directives::{arg_f64, arg_ids, arg_u64, Directive};
use crate::fields::{Field, Fields};
use crate::params::NationParams;
use crate::rng::{stream, Rng};
use crate::vec3::Vec3;
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

mod feudal;
mod institutions;
mod ports;
mod transport;
use feudal::Empire;
pub use feudal::RANKS;
use institutions::EMBRACED;
pub use institutions::INSTITUTIONS;
use transport::{
    travel_costs, Line, AIRPORT_BUILD, AIRPORT_UPKEEP, AIR_FACTOR, AIR_PENALTY, RAIL_BUILD_KM, RAIL_UPKEEP_KM, ROAD_BUILD, ROAD_NAMES, ROAD_SPEED, ROAD_UPKEEP, SEA_FACTOR, SEA_PENALTY,
    STATION_BUILD, STATION_UPKEEP,
};

/// Eras, in order; a nation's era is the last whose technology year it has reached.
pub const ERA_NAMES: [&str; 7] = ["early", "gunpowder", "ocean shipping", "industry", "fertilizer", "motor age", "air age"];
/// Number of institutions (one per era after the first).
const N_INST: usize = 6;

/// Simulation time is a whole number of months; the year is the count divided by 12.
pub const MONTHS: i64 = 12;

/// A duration in years as whole months.
fn months(years: f64) -> i64 {
    (years * MONTHS as f64).round() as i64
}

/// A time in months as "year-month" (e.g. "1853-07") for the state and tables.
fn date(t: i64) -> String {
    format!("{}-{:02}", t.div_euclid(MONTHS), t.rem_euclid(MONTHS) + 1)
}

mod era {
    pub const GUN: u8 = 1;
    pub const SHIP: u8 = 2;
    pub const IND: u8 = 3;
    pub const FERT: u8 = 4;
    pub const MOTOR: u8 = 5;
    pub const AIR: u8 = 6;
}

/// Travel cost (km on foot) at which a province counts as half integrated.
const INTEGRATION_KM: f64 = 1500.0;
/// Integration of a province the capital cannot reach (an exclave before shipping).
const UNREACHED: f64 = 0.2;
/// Expansion and war reach use access scaled to straight-line km (a roadless
/// route costs more than the crow flies).
const ACCESS_SCALE: f64 = 0.75;
/// Share of income spent on the army.
const ARMY: f64 = 0.3;
/// Most a nation's technology lags the calendar (the poorest and smallest).
const TECH_LAG: f64 = 220.0;
/// Most infrastructure upkeep a nation takes on by itself, as a share of income.
const INFRA_SHARE: f64 = 0.4;
/// Years of income a nation keeps in its treasury; half of anything more is spent each year.
const RESERVE_YEARS: f64 = 2.0;
/// People a city needs for a paved road to the capital before industry.
const PAVED_CITY: f64 = 150_000.0;
/// People a city needs for a highway or an airport.
const BIG_CITY: f64 = 300_000.0;

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
    /// Deposits that draw institutions (coal and iron: industry; oil: motors).
    coal: bool,
    iron: bool,
    oil: bool,
    /// How fertile the land is (0–1): synthetic fertilizer favours farmland.
    fertile: f64,
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
    /// When the current capital became the capital.
    capital_since: i64,
    /// Technology, in years (the era follows it).
    tech: f64,
    era: u8,
    treasury: f64,
    /// Income and spending per year at the last step.
    income: f64,
    upkeep: f64,
    /// Upkeep of roads, railways and airports per year.
    infra: f64,
    /// Bankrupt until this time (less stable; no new cuts).
    debt_until: i64,
    /// Feudal liege (0 = independent) and rank (see `RANKS`).
    liege: u32,
    rank: u8,
    /// When the next road and railway projects are due.
    next_road: i64,
    next_rail: i64,
    /// Reforming (westernizing) until this time, on the model of this nation.
    reform_until: i64,
    reform_model: u32,
}

/// A city pin: attraction placed by a directive.
#[derive(Clone)]
struct CityPin {
    id: u32,
    province: usize,
    value: f64,
    label: String,
    until: Option<i64>,
    by: String,
    since: i64,
}

#[derive(Clone)]
enum Effect {
    Aggression(u32, f64),
    Toward(u32, usize),
    War(u32, u32),
    Peace(u32, u32),
    Stability(u32, f64),
}

/// The Stage 4 simulation, one step at a time (see `step_len`).
pub struct NationSim {
    np: NationParams,
    r_km: f64,
    provs: Vec<Prov>,
    adj: Vec<Vec<Edge>>,
    index: HashMap<u32, usize>,
    prov_field: Vec<u32>,
    region_names: BTreeMap<u16, String>,
    state_names: BTreeMap<u16, String>,
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
    /// Railway per province from the open lines: 0 none, 1 track, 2 station, 3 junction.
    rail: Vec<u8>,
    lines: Vec<Line>,
    /// People the railway draws to a province (track, stations, junctions).
    cap_extra: Vec<f64>,
    /// Roads over borders (lower index first): quality and year built.
    roads: BTreeMap<(usize, usize), (u8, i32)>,
    /// Airports: province and year opened.
    airports: BTreeMap<usize, i32>,
    /// Harbour quality of each province (0 = no sea coast).
    harbour: Vec<f64>,
    /// Ports: province and year opened; the nearest ports of each (cached).
    ports: BTreeMap<usize, i32>,
    port_near: Option<Vec<(usize, Vec<usize>)>>,
    /// Coastal sailing: for each coastal province, the nearest coasts of other
    /// landmasses within reach (institutions cross the sea slowly before ports).
    coast_near: Vec<(usize, Vec<usize>)>,
    /// Presence (0–1) of each institution in each province.
    presence: Vec<[f32; N_INST]>,
    /// Year and province each institution was born in.
    born: [Option<(i32, usize)>; N_INST],
    /// Birthplaces chosen by directives (Some(None): by chance).
    birth_choice: [Option<Option<usize>>; N_INST],
    /// Institutions every settled province has embraced (no more spreading to do).
    saturated: [bool; N_INST],
    /// Tags: (scope, id, tag); see `feudal::scope`.
    tags: BTreeSet<(u8, u32, String)>,
    empires: Vec<Empire>,
    /// Per nation (by id): travel cost from the capital to its provinces and
    /// their neighbours; recomputed when `stale`.
    access: Vec<HashMap<usize, f64>>,
    stale: Vec<bool>,
    /// Integration of each province with its owner (0–1).
    integ: Vec<f64>,
    /// How many nations have reached each era (events for the first three).
    era_firsts: [u32; 7],
    events: Vec<Value>,
    /// Provinces taken from one nation by another since the last war summary.
    tally: BTreeMap<(u32, u32), u32>,
    rng: Rng,
    /// Time the next step starts, in whole months from year 0 (month 0 of
    /// year y is y × 12), and its year (the month count divided by 12).
    t: i64,
    pub year: i32,
    /// When the next war summary and progress report are due (months).
    next_flush: i64,
    next_progress: i64,
    directives: Vec<Directive>,
    next_directive: usize,
    effects: Vec<(Effect, i64)>,
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
        let has = |p: &Value, d: &str| p["resources"].as_array().is_some_and(|a| a.iter().any(|x| x.as_str() == Some(d)));
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
                    coal: has(p, "coal"),
                    iron: has(p, "iron"),
                    oil: has(p, "oil"),
                    fertile: p["habitability"].as_f64().unwrap_or(capf).clamp(0.0, 1.0),
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
        let state_names: BTreeMap<u16, String> = ptable["states"].as_array().unwrap_or(&empty).iter().map(|r| (r["id"].as_u64().unwrap_or(0) as u16, r["name"].as_str().unwrap_or("").to_string())).collect();
        let harbour = ports::harbour_quality(ctx, &provs, &index);
        let coastal_land: Vec<usize> = (0..n).filter(|&p| provs[p].land && provs[p].coastal && harbour[p] > 0.0).collect();
        let coast_near: Vec<(usize, Vec<usize>)> = coastal_land
            .iter()
            .map(|&a| {
                let mut near: Vec<(f64, usize)> = coastal_land
                    .iter()
                    .filter(|&&b| provs[b].continent != provs[a].continent)
                    .map(|&b| (provs[a].center.angle_to(provs[b].center) * r_km, b))
                    .filter(|x| x.0 <= 900.0)
                    .collect();
                near.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
                (a, near.into_iter().take(4).map(|x| x.1).collect())
            })
            .filter(|x: &(usize, Vec<usize>)| !x.1.is_empty())
            .collect();
        Some(NationSim {
            year: np.start_year,
            t: np.start_year as i64 * MONTHS,
            next_flush: (np.start_year as i64 + 50) * MONTHS,
            next_progress: np.start_year as i64 * MONTHS,
            directives: crate::directives::of_stage(&ctx.edits.overrides.directives, "nations").into_iter().cloned().collect(),
            np,
            r_km,
            adj,
            index,
            prov_field: ctx.input.u32("province").to_vec(),
            region_names,
            state_names,
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
            lines: Vec::new(),
            cap_extra: vec![0.0; n],
            roads: BTreeMap::new(),
            airports: BTreeMap::new(),
            harbour,
            ports: BTreeMap::new(),
            port_near: None,
            coast_near,
            presence: vec![[0.0; N_INST]; n],
            born: [None; N_INST],
            birth_choice: [None; N_INST],
            saturated: [false; N_INST],
            tags: BTreeSet::new(),
            empires: Vec::new(),
            access: vec![HashMap::new()],
            stale: vec![false],
            integ: vec![0.0; n],
            era_firsts: [0; 7],
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
        self.t >= self.np.start_date as i64 * MONTHS
    }

    /// Current time in whole months from year 0.
    pub fn time(&self) -> i64 {
        self.t
    }

    /// Month of the current year (1–12).
    pub fn month(&self) -> u8 {
        (self.t.rem_euclid(MONTHS) + 1) as u8
    }

    /// Current time in years, for rates and technology (not for keeping time).
    fn now(&self) -> f64 {
        self.t as f64 / MONTHS as f64
    }

    /// Length in months of the step starting now: `months_per_step`, then the
    /// shorter steps from `step_year_1` and `step_year_2`, never crossing one
    /// of those years or the start date.
    pub fn step_months(&self) -> i64 {
        let np = &self.np;
        let (y1, y2) = (np.step_year_1 as i64 * MONTHS, np.step_year_2 as i64 * MONTHS);
        let mut dm = if self.t >= y2 {
            np.months_per_step_2
        } else if self.t >= y1 {
            np.months_per_step_1
        } else {
            np.months_per_step
        } as i64;
        dm = dm.clamp(1, 1200);
        for b in [y1, y2, np.start_date as i64 * MONTHS] {
            if b > self.t && self.t + dm > b {
                dm = b - self.t;
            }
        }
        dm
    }

    /// Length of the step starting now in years (rates are per year).
    pub fn step_len(&self) -> f64 {
        self.step_months() as f64 / MONTHS as f64
    }

    /// Queue a directive issued during a live run (in the order a fresh run
    /// would apply it: by time, then as issued).
    pub fn add_directive(&mut self, d: crate::directives::Directive) {
        let k = (self.next_directive..self.directives.len()).find(|&k| self.directives[k].month_index() > d.month_index()).unwrap_or(self.directives.len());
        self.directives.insert(k, d);
    }

    pub fn start_year(&self) -> i32 {
        self.np.start_year
    }

    pub fn end_year(&self) -> i32 {
        self.np.start_date
    }


    /// The era a technology year falls in.
    fn era_from(&self, tech: f64) -> u8 {
        let np = &self.np;
        [np.gunpowder_year, np.shipping_year, np.industrial_year, np.fertilizer_year, np.motor_year, np.air_year].iter().filter(|&&y| tech >= y as f64).count() as u8
    }

    /// The most advanced living nation (era first, then technology).
    fn leader(&self) -> Option<u32> {
        (1..=self.nations.len() as u32)
            .filter(|&m| self.alive(m))
            .max_by(|&a, &b| self.nation(a).era.cmp(&self.nation(b).era).then(self.nation(a).tech.total_cmp(&self.nation(b).tech)).then(b.cmp(&a)))
    }

    /// The world's era: the leading nation's (by the calendar before any nation).
    pub fn era(&self) -> &'static str {
        ERA_NAMES[self.leader().map_or_else(|| self.era_from(self.year as f64), |m| self.nation(m).era) as usize]
    }

    /// Productivity per person: grows with technology, and jumps with
    /// industry, fertilizer, motors and flight.
    fn prod(&self, n: u32) -> f64 {
        let x = self.nation(n);
        let mut p = 1.0 + ((x.tech - 1000.0) / 250.0).max(0.0);
        for (e, f) in [(era::IND, 1.8), (era::FERT, 1.15), (era::MOTOR, 1.3), (era::AIR, 1.1)] {
            if x.era >= e {
                p *= f;
            }
        }
        p
    }

    /// How much more farmland holds with synthetic fertilizer (phased in over 20 years of technology).
    fn fert_mult(&self, n: u32) -> f64 {
        if n == 0 {
            return 1.0;
        }
        let t = self.nation(n).tech - self.np.fertilizer_year as f64;
        1.0 + (self.np.fertilizer_boost - 1.0).max(0.0) * (t / 20.0).clamp(0.0, 1.0)
    }

    /// Institutions so far, the next one to come, and the empires (for the live view).
    pub fn institutions(&self) -> Value {
        json!({ "institutions": self.institution_rows(), "next": self.next_institution(), "pending": self.pending_institution() })
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
        let tags = if self.has_nation_tag(n, "expansionist") { 1.5 } else { 1.0 } * if self.world_tag("frequent_wars") { 1.5 } else { 1.0 };
        tags * self.effects.iter().filter_map(|(e, _)| if let Effect::Aggression(m, f) = e { (*m == n).then_some(*f) } else { None }).product::<f64>()
    }

    /// A province of a free state: it never changes hands by war or settlement.
    fn free(&self, p: usize) -> bool {
        self.has_state_tag(self.provs[p].state as u32, "free_state")
    }

    /// How much a province is coveted: ×3 in a holy land.
    fn coveted(&self, p: usize) -> f64 {
        if self.has_state_tag(self.provs[p].state as u32, "holy_land") {
            3.0
        } else {
            1.0
        }
    }

    /// People of nation n, or of its whole feudal empire if it belongs to one.
    fn realm_pop(&self, n: u32) -> f64 {
        let list = match self.empire_of(n) {
            Some(k) => self.empire_members(k),
            None => vec![n],
        };
        list.iter().map(|&m| self.members[m as usize].iter().map(|&p| self.pop[p]).sum::<f64>()).sum()
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
        // A breakaway keeps its parent's technology and era; a new polity
        // starts at the world's median technology, and its era follows the
        // institutions its land has embraced.
        let (tech, era) = if parent > 0 {
            (self.nation(parent).tech, self.nation(parent).era)
        } else {
            let mut t: Vec<f64> = self.nations.iter().filter(|x| x.ended.is_none()).map(|x| x.tech).collect();
            t.sort_by(|a, b| a.partial_cmp(b).unwrap());
            (t.get(t.len() / 2).copied().unwrap_or(self.now()), 0)
        };
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
            capital_since: self.t,
            tech,
            era,
            treasury: 0.0,
            income: 0.0,
            upkeep: 0.0,
            infra: 0.0,
            debt_until: i64::MIN,
            liege: 0,
            rank: 0,
            next_road: self.t + (id % 7) as i64 * MONTHS,
            next_rail: self.t + (id % 5) as i64 * MONTHS,
            reform_until: i64::MIN,
            reform_model: 0,
        });
        self.members.push(BTreeSet::new());
        self.access.push(HashMap::new());
        self.stale.push(true);
        let prev = self.owner[p];
        self.take(p, id);
        self.set_era(id);
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
            self.stale[old as usize] = true;
        }
        self.owner[p] = n;
        if n > 0 {
            self.members[n as usize].insert(p);
            self.stale[n as usize] = true;
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
        self.nations[k].capital_since = self.t;
        self.stale[n as usize] = true;
        let (a, b) = (self.nations[k].name.clone(), self.provs[q].name.clone());
        self.event("capital", n, 0, Some(q), format!("{b} becomes the capital of {a}"));
    }

    /// Attraction of every province this step: devastation, capitals (grown
    /// into over `capital_years`, larger for larger nations), railway
    /// stations and city pins.
    fn update_attraction(&mut self) {
        let np = &self.np;
        for p in 0..self.provs.len() {
            self.attr[p] = self.devastation[p]
                + match self.rail[p] {
                    2 => 0.15,
                    3 => 0.2,
                    _ => 0.0,
                }
                + if self.airports.contains_key(&p) { 0.1 } else { 0.0 }
                + if self.ports.contains_key(&p) { 0.1 } else { 0.0 };
        }
        for (k, x) in self.nations.iter().enumerate() {
            if x.ended.is_some() {
                continue;
            }
            let size = self.members[k + 1].len().max(1) as f64;
            let grown = ((self.t - x.capital_since) as f64 / MONTHS as f64 / np.capital_years.max(1.0)).clamp(0.0, 1.0);
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
                        "until": p.until.map(date), "since": date(p.since), "by": p.by, "owner": self.owner[p.province],
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
                        "capital": capital_of.contains_key(&p), "attraction": (self.attr[p] * 100.0).round() / 100.0, "station": self.rail[p] >= 2,
                        "junction": self.rail[p] == 3, "airport": self.airports.contains_key(&p), "port": self.ports.contains_key(&p), "road": self.road_at(p),
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

    fn apply_directives(&mut self, dm: i64) {
        let t = self.t;
        self.effects.retain(|(_, until)| t < *until);
        // A step covers months t .. t + dm: directives dated in it apply now.
        while self.next_directive < self.directives.len() && self.directives[self.next_directive].month_index() < t + dm {
            let d = self.directives[self.next_directive].clone();
            self.next_directive += 1;
            let a = &d.args;
            let until = t + months(arg_f64(a, "years", 0.0)).max(dm);
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
                    // One treasury, and the more advanced technology.
                    let (tr, te) = (self.nation(target).treasury, self.nation(target).tech);
                    let x = &mut self.nations[nat as usize - 1];
                    x.treasury += tr;
                    x.tech = x.tech.max(te);
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
                    (Some(x), Some(z)) if self.nation(nat).era >= era::IND && self.owner[z] == nat => ok = self.build_line(nat, x, &[z].into_iter().collect(), false).is_some(),
                    _ => ok = false,
                },
                "build_road" if self.alive(nat) => match (p_from, p_to) {
                    (Some(x), Some(z)) => {
                        let q = arg_f64(a, "quality", 1.0).round().clamp(1.0, 3.0) as u8;
                        ok = (q < 3 || self.nation(nat).era >= era::MOTOR) && self.owner[z] == nat && self.build_road(nat, x, z, q, false);
                    }
                    _ => ok = false,
                },
                "airport" if self.alive(nat) => match p_province {
                    Some(p) if self.nation(nat).era >= era::AIR && self.owner[p] == nat && !self.airports.contains_key(&p) => self.build_airport(nat, p),
                    _ => ok = false,
                },
                "subsidy" if self.alive(nat) => {
                    let x = &mut self.nations[nat as usize - 1];
                    x.treasury += x.income.max(0.0) * arg_f64(a, "years", 0.0).max(0.0);
                }
                "tech" if self.alive(nat) => {
                    let tech = self.nation(nat).tech + arg_f64(a, "years", 0.0);
                    // Institutions whose era year the new technology passes:
                    // born in its capital if they aren't yet, embraced everywhere.
                    let mut upto = 0;
                    for i in 0..N_INST {
                        if tech + 1e-9 < self.era_year(i as u8 + 1) as f64 || (i > 0 && self.born[i - 1].is_none()) {
                            break;
                        }
                        if self.born[i].is_none() {
                            let cap = self.nation(nat).capital;
                            self.bear(i, cap);
                        }
                        upto = i + 1;
                    }
                    self.embrace_all(nat, upto);
                    self.nations[nat as usize - 1].tech = tech;
                    self.set_era(nat);
                }
                "port" if self.alive(nat) => match p_province {
                    Some(p) if self.nation(nat).era >= era::SHIP && self.owner[p] == nat && self.harbour[p] > 0.0 && !self.ports.contains_key(&p) => {
                        let cost = ports::PORT_BUILD * self.np.rail_cost.max(0.0) * self.prod(nat);
                        self.nations[nat as usize - 1].treasury -= cost;
                        self.open_port(nat, p);
                    }
                    _ => ok = false,
                },
                "institution_birth" => {
                    let i = arg_f64(a, "institution", -1.0).round();
                    if (0.0..N_INST as f64).contains(&i) && self.born[i as usize].is_none() {
                        let choice = match arg_u64(a, "province") {
                            Some(_) => p_province.filter(|&p| self.provs[p].land),
                            None => None,
                        };
                        if a["province"].is_null() || choice.is_some() {
                            self.birth_choice[i as usize] = Some(choice);
                        } else {
                            ok = false;
                        }
                    } else {
                        ok = false;
                    }
                }
                "reform" if self.alive(nat) => {
                    let years = arg_f64(a, "years", 40.0).max(dm as f64 / MONTHS as f64);
                    self.start_reform(nat, 0, years);
                }
                "feudal_empire" if self.alive(nat) => {
                    if a["on"].as_bool().unwrap_or(true) {
                        let places = self.places(a);
                        ok = self.create_empire(nat, places, a["name"].as_str().map(str::to_string));
                    } else {
                        match self.empires.iter().position(|e| e.dissolved.is_none() && e.emperor == self.realm_of(nat)) {
                            Some(k) => self.dissolve_empire(k),
                            None => ok = false,
                        }
                    }
                }
                "tag" => {
                    let id = arg_u64(a, "id").unwrap_or(0) as u32;
                    ok = self.set_tag(a["scope"].as_str().unwrap_or(""), id, a["tag"].as_str().unwrap_or(""), a["on"].as_bool().unwrap_or(true));
                }
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
                        let until = a["years"].as_f64().map(|yrs| t + months(yrs).max(dm));
                        let label = a["label"].as_str().unwrap_or("").trim().to_string();
                        self.pins.push(CityPin { id, province: p, value: arg_f64(a, "value", 0.0).clamp(-1.0, 1.0), label, until, by: d.by.clone(), since: t });
                    }
                    None => ok = false,
                },
                "pin_move" => {
                    let pid = arg_u64(a, "pin").unwrap_or(0) as u32;
                    match (self.pins.iter_mut().find(|x| x.id == pid), p_province.filter(|&p| self.provs[p].land)) {
                        (Some(pin), Some(p)) => {
                            pin.province = p;
                            pin.since = t;
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
            self.applied.push(json!({ "year": t.div_euclid(MONTHS), "month": t.rem_euclid(MONTHS) + 1, "action": d.action, "args": d.args, "note": d.note, "by": d.by, "applied": ok }));
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
    fn expand(&mut self, n: u32) {
        let cap = self.nation(n).capital;
        let members: Vec<usize> = self.members[n as usize].iter().copied().collect();
        if members.is_empty() {
            return;
        }
        let (gun, ship) = (self.nation(n).era >= era::GUN, self.nation(n).era >= era::SHIP);
        let reach = self.np.reach_km.max(50.0);
        let toward = self.toward(n);
        // Large nations probe a sample of their border.
        let probe: Vec<usize> = if members.len() > 48 { (0..48).map(|_| members[self.rng.below(members.len())]).collect() } else { members.clone() };
        let mut best: Option<(f64, usize)> = None;
        for &p in &probe {
            for e in &self.adj[p] {
                let q = e.to as usize;
                let o = self.owner[q];
                // Barren land that holds no one (ice caps) is left unclaimed;
                // fellow members of a feudal empire and free states are left alone.
                if o == n || !self.provs[q].land || (o > 0 && (self.at_peace(n, o) || self.same_realm(n, o))) || (self.cap[q] < 1.0 && self.pop[q] < 1.0) || self.free(q) {
                    continue;
                }
                let step = self.edge_cost(p, e);
                let value = (self.pop[q] + 0.1 * self.cap[q]).max(1.0).sqrt() * self.coveted(q);
                let mut cost = (1.0 + step / 300.0) * (1.0 + self.reach_cost(n, q) / reach) * (1.0 + self.np.culture_weight * self.cdist(n, q));
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
        // Colonies across the sea, sailing from the nation's ports (better
        // harbours reach farther; merchant republics sail more often).
        let ports: Vec<usize> = if ship && !self.world_tag("no_overseas_colonies") && !self.has_nation_tag(n, "isolationist") {
            self.ports.keys().copied().filter(|&p| self.owner[p] == n).take(16).collect()
        } else {
            vec![]
        };
        if !ports.is_empty() {
            let chance = if self.has_nation_tag(n, "merchant_republic") { 0.6 } else { 0.3 };
            let harbour = ports.iter().map(|&p| self.harbour[p]).fold(0.0, f64::max);
            let range = self.np.overseas_km * (0.6 + 0.8 * harbour);
            if self.rng.f64() < chance && !self.coastal_land.is_empty() {
                let own_pop: f64 = members.iter().map(|&p| self.pop[p]).sum();
                for _ in 0..12 {
                    let q = self.coastal_land[self.rng.below(self.coastal_land.len())];
                    let o = self.owner[q];
                    // Free land, or land of a much weaker nation (colonial conquest).
                    if o == n || self.free(q) || (o > 0 && (self.at_peace(n, o) || self.same_realm(n, o) || self.realm_pop(o) > 0.25 * own_pop)) {
                        continue;
                    }
                    let d = ports.iter().map(|&p| self.km(p, q)).fold(f64::INFINITY, f64::min);
                    if d > range {
                        continue;
                    }
                    let mut s = (self.pop[q] + 0.1 * self.cap[q]).max(1.0).sqrt() * self.coveted(q) / (1.0 + d / 1500.0) * (0.5 + self.rng.f64());
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
        // War over q: the two nations' strength near it (a feudal empire's
        // members stand together in defence).
        let strength = |s: &NationSim, m: u32, defend: bool| {
            let total: f64 = if defend { s.realm_pop(m) } else { s.members[m as usize].iter().map(|&p| s.pop[p]).sum() };
            let near = 1.0 / (1.0 + s.reach_cost(m, q) / reach);
            // Better-armed (more productive) nations fight better.
            total.max(1.0).powf(0.8) * near * s.prod(m).powf(0.4)
        };
        let mut att = strength(self, n, false);
        let mut def = strength(self, o, true);
        if self.has_nation_tag(o, "eternal") {
            def *= 1.5;
        }
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

    /// The border between two neighbouring provinces.
    fn border(&self, a: usize, b: usize) -> Option<&Edge> {
        self.adj[a].iter().find(|e| e.to as usize == b)
    }

    fn road_q(&self, a: usize, b: usize) -> u8 {
        self.roads.get(&(a.min(b), a.max(b))).map_or(0, |r| r.0)
    }

    /// Best road touching a province.
    fn road_at(&self, p: usize) -> u8 {
        self.adj[p].iter().map(|e| self.road_q(p, e.to as usize)).max().unwrap_or(0)
    }

    /// Travel cost over a border (km on foot): longer across barriers and
    /// impassable land, divided by the road's speed; a strait is a crossing.
    fn edge_cost(&self, p: usize, e: &Edge) -> f64 {
        let b = 1.0 + self.np.barrier_weight * e.barrier;
        match e.ty {
            edge::LAND => e.km * b / ROAD_SPEED[self.road_q(p, e.to as usize) as usize],
            edge::IMPASSABLE => 3.0 * e.km * b / ROAD_SPEED[self.road_q(p, e.to as usize) as usize],
            _ => 2.0 * e.km + 150.0,
        }
    }

    /// Construction length of a border (km, longer across barriers and twice over impassable land).
    fn build_km(&self, e: &Edge) -> f64 {
        e.km * (1.0 + self.np.barrier_weight * e.barrier) * if e.ty == edge::IMPASSABLE { 2.0 } else { 1.0 }
    }

    /// How far a province is from nation n's capital for expansion and war:
    /// its access (roads, railways, sea and air shorten it), or the straight
    /// distance where the capital has no route yet.
    fn reach_cost(&self, n: u32, q: usize) -> f64 {
        let km = self.km(self.nation(n).capital, q);
        self.access[n as usize].get(&q).map_or(km, |&a| a * ACCESS_SCALE)
    }

    /// Recompute nation n's access: travel cost from its capital over its
    /// land (with roads), its railway stations, sea lanes between its ports
    /// (ocean shipping) and flights between its airports (air age).
    fn compute_access(&mut self, n: u32) {
        let k = n as usize;
        let cap = self.nation(n).capital;
        let era = self.nation(n).era;
        let owner = &self.owner;
        let land = |p: usize, emit: &mut dyn FnMut(usize, f64)| {
            if owner[p] == n {
                for e in &self.adj[p] {
                    emit(e.to as usize, self.edge_cost(p, e));
                }
            }
        };
        let lines: Vec<&Line> = self.lines.iter().filter(|l| l.is_open() && l.stations().any(|p| owner[p] == n)).collect();
        let mut links = Vec::new();
        if era >= era::SHIP {
            links.extend(self.sea_links(n));
        }
        if era >= era::AIR {
            let air: Vec<usize> = self.airports.keys().copied().filter(|&p| owner[p] == n).collect();
            for (i, &a) in air.iter().enumerate() {
                for &b in &air[i + 1..] {
                    links.push((a, b, self.km(a, b) * AIR_FACTOR + AIR_PENALTY));
                }
            }
        }
        let map = travel_costs(cap, &land, &lines, &|p| owner[p] == n, &|a, b| self.km(a, b), self.np.transfer_km, &links);
        self.access[k] = map;
        self.stale[k] = false;
    }

    /// Bring every changed nation's access up to date, then each province's integration.
    fn refresh_access(&mut self) {
        for m in 1..=self.nations.len() as u32 {
            if self.alive(m) && self.stale[m as usize] {
                self.compute_access(m);
            }
        }
        for p in 0..self.provs.len() {
            let o = self.owner[p];
            self.integ[p] = if o == 0 { 0.0 } else { self.access[o as usize].get(&p).map_or(UNREACHED, |&a| 1.0 / (1.0 + a / INTEGRATION_KM)) };
        }
    }

    /// Cheapest route over nation n's land (no straits) from `from` to a
    /// province where `goal` holds, with border weights from `w`.
    fn route(&self, n: u32, from: usize, goal: &dyn Fn(usize) -> bool, w: &dyn Fn(usize, &Edge) -> f64) -> Option<Vec<usize>> {
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
            if p != from && goal(p) {
                end = Some(p);
                break;
            }
            for e in &self.adj[p] {
                let q = e.to as usize;
                if self.owner[q] != n || e.ty == edge::STRAIT {
                    continue;
                }
                let nd = d + w(p, e);
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
        Some(path)
    }

    /// Railway per province and the people it draws, from the open lines.
    fn rebuild_rail(&mut self) {
        let n = self.provs.len();
        let mut track = vec![false; n];
        let mut stations = vec![0u32; n];
        for l in self.lines.iter().filter(|l| l.is_open()) {
            for (k, &p) in l.path.iter().enumerate() {
                track[p] = true;
                if l.station[k] {
                    stations[p] += 1;
                }
            }
        }
        let sp = self.np.station_people;
        for p in 0..n {
            (self.rail[p], self.cap_extra[p]) = match (stations[p], track[p]) {
                (0, false) => (0, 0.0),
                (0, true) => (1, 0.3 * sp),
                (1, _) => (2, sp),
                _ => (3, 1.5 * sp),
            };
        }
    }

    /// Mark the nations owning any of these provinces as needing new access.
    fn touch(&mut self, ps: &[usize]) {
        for &p in ps {
            let o = self.owner[p] as usize;
            if o > 0 {
                self.stale[o] = true;
            }
        }
    }

    /// A railway from `from` to the nearest of nation n's provinces in `to`
    /// over its own land. A line ending where an own line ends extends it;
    /// otherwise it is a new line, and where it meets a line that province
    /// becomes a junction station. Paid from the treasury: with `afford`, only
    /// if the treasury holds the cost. Returns the line's index.
    fn build_line(&mut self, n: u32, from: usize, to: &BTreeSet<usize>, afford: bool) -> Option<usize> {
        if to.contains(&from) || self.owner[from] != n {
            return None;
        }
        let path = self.route(n, from, &|p| to.contains(&p), &|_, e| self.build_km(e))?;
        let last = path.len() - 1;
        let end = path[last];
        let mut station: Vec<bool> = path
            .iter()
            .enumerate()
            .map(|(k, &p)| k == 0 || k == last || k % 5 == 0 || self.provs[p].arid || self.pop[p] >= self.np.found_population)
            .collect();
        let track_km: f64 = path.windows(2).map(|w| self.border(w[0], w[1]).map_or_else(|| self.km(w[0], w[1]), |e| self.build_km(e))).sum();
        // The open line already at the end province, preferring one of ours that ends there.
        let joined = (0..self.lines.len())
            .filter(|&i| self.lines[i].is_open() && self.lines[i].path.contains(&end))
            .max_by_key(|&i| (self.lines[i].owner == n && (self.lines[i].path[0] == end || *self.lines[i].path.last().unwrap() == end), Reverse(i)));
        let extend = joined.filter(|&i| self.lines[i].owner == n && (self.lines[i].path[0] == end || *self.lines[i].path.last().unwrap() == end) && !path[..last].iter().any(|p| self.lines[i].path.contains(p)));
        let end_has_station = joined.is_some_and(|i| {
            let l = &self.lines[i];
            l.path.iter().zip(&l.station).any(|(&p, &s)| p == end && s)
        });
        let new_stations = station[..last].iter().filter(|&&s| s).count() + usize::from(!end_has_station);
        let cost = (track_km * RAIL_BUILD_KM + new_stations as f64 * STATION_BUILD) * self.np.rail_cost.max(0.0) * self.prod(n);
        let more = (RAIL_UPKEEP_KM * track_km + STATION_UPKEEP * new_stations as f64) * self.np.rail_cost.max(0.0) * self.prod(n);
        if afford && (self.nation(n).treasury < cost || !self.can_keep(n, more)) {
            return None;
        }
        self.nations[n as usize - 1].treasury -= cost;
        let seg_km: f64 = path.windows(2).map(|w| self.km(w[0], w[1])).sum();
        let idx = match extend {
            Some(i) => {
                let l = &mut self.lines[i];
                // Append the new stretch at the end it meets, keeping the line in order.
                let mut add: Vec<(usize, bool)> = path[..last].iter().copied().zip(station[..last].iter().copied()).collect();
                if l.path[0] == end {
                    let (p, s): (Vec<usize>, Vec<bool>) = add.into_iter().unzip();
                    l.path.splice(0..0, p);
                    l.station.splice(0..0, s);
                } else {
                    add.reverse();
                    for (p, s) in add {
                        l.path.push(p);
                        l.station.push(s);
                    }
                }
                l.km += seg_km;
                i
            }
            None => {
                if let Some(i) = joined {
                    // A junction: the line met gets a station here, for changing lines.
                    let l = &mut self.lines[i];
                    for (k, &p) in l.path.iter().enumerate() {
                        if p == end {
                            l.station[k] = true;
                        }
                    }
                }
                station[last] = true;
                let id = self.lines.len() as u32 + 1;
                self.lines.push(Line { id, owner: n, path: path.clone(), station, opened: self.year, closed: None, km: seg_km });
                self.lines.len() - 1
            }
        };
        self.rebuild_rail();
        let all = self.lines[idx].path.clone();
        self.touch(&all);
        Some(idx)
    }

    /// A nation's railway project: link its largest city not yet on its
    /// railways to them (the capital starts the network), if it can pay.
    fn railway_project(&mut self, n: u32) -> bool {
        let members: Vec<usize> = self.members[n as usize].iter().copied().collect();
        if members.len() < 4 {
            return false;
        }
        let mut cities: Vec<usize> = members.iter().copied().filter(|&p| self.pop[p] >= self.np.found_population).collect();
        cities.sort_by(|&a, &b| self.pop[b].partial_cmp(&self.pop[a]).unwrap().then(a.cmp(&b)));
        cities.truncate(self.np.railway_cities.max(2) as usize);
        let cap = self.nation(n).capital;
        let net: BTreeSet<usize> = members.iter().copied().filter(|&p| self.rail[p] > 0).collect();
        let net = if net.is_empty() { [cap].into_iter().collect() } else { net };
        for &c in &cities {
            if net.contains(&c) {
                continue;
            }
            let before = self.lines.len();
            if let Some(i) = self.build_line(n, c, &net, true) {
                let (a, b) = (self.nation(n).name.clone(), self.provs[c].name.clone());
                let first = before == 0;
                if first || self.rng.f64() < 0.15 {
                    let what = if self.lines.len() > before { "opens a railway line" } else { "extends its railway" };
                    let junction = self.lines[i].path.first().is_some_and(|&p| self.rail[p] == 3) || self.lines[i].path.last().is_some_and(|&p| self.rail[p] == 3);
                    self.event("railway", n, 0, Some(c), format!("{a} {what} to {b}{}{}", if junction { ", with a junction where lines meet" } else { "" }, if first { " (the world's first railway)" } else { "" }));
                }
                return true;
            }
        }
        false
    }

    /// Whether nation n can take on more upkeep by itself (automatic projects).
    fn can_keep(&self, n: u32, more: f64) -> bool {
        let x = self.nation(n);
        x.infra + more <= INFRA_SHARE * x.income
    }

    /// Cost to nation n of upgrading a path's roads to quality q.
    fn road_cost_of(&self, n: u32, path: &[usize], q: u8) -> f64 {
        path.windows(2)
            .map(|w| {
                let have = self.road_q(w[0], w[1]);
                if have >= q {
                    0.0
                } else {
                    self.border(w[0], w[1]).map_or(0.0, |e| self.build_km(e)) * (ROAD_BUILD[q as usize] - ROAD_BUILD[have as usize])
                }
            })
            .sum::<f64>()
            * self.np.road_cost.max(0.0)
            * self.prod(n)
    }

    /// Route for a road of quality q between two of nation n's provinces:
    /// cheapest to build, reusing roads already good enough.
    fn road_route(&self, n: u32, a: usize, b: usize, q: u8) -> Option<Vec<usize>> {
        if a == b || self.owner[a] != n || self.owner[b] != n {
            return None;
        }
        self.route(n, a, &|p| p == b, &|p, e| {
            let have = self.road_q(p, e.to as usize);
            self.build_km(e) * (0.15 + if have >= q { 0.0 } else { 1.0 - ROAD_BUILD[have as usize] / ROAD_BUILD[q as usize] })
        })
    }

    /// Build or upgrade a road of quality q from a to b over nation n's land.
    /// With `afford`, only if the treasury holds the cost.
    fn build_road(&mut self, n: u32, a: usize, b: usize, q: u8, afford: bool) -> bool {
        let Some(path) = self.road_route(n, a, b, q) else { return false };
        let cost = self.road_cost_of(n, &path, q);
        let more: f64 = path
            .windows(2)
            .map(|w| self.km(w[0], w[1]) * (ROAD_UPKEEP[q as usize] - ROAD_UPKEEP[self.road_q(w[0], w[1]) as usize]).max(0.0))
            .sum::<f64>()
            * 0.5
            * self.np.road_cost.max(0.0)
            * self.prod(n);
        if afford && (cost <= 0.0 || self.nation(n).treasury < cost || !self.can_keep(n, more)) {
            return false;
        }
        self.nations[n as usize - 1].treasury -= cost;
        for w in path.windows(2) {
            let key = (w[0].min(w[1]), w[0].max(w[1]));
            let e = self.roads.entry(key).or_insert((0, self.year));
            if e.0 < q {
                *e = (q, self.year);
            }
        }
        self.touch(&path);
        true
    }

    /// A nation's road project: the first of its largest cities whose road to
    /// the capital is below what it wants (a highway between big cities in
    /// the motor age, a paved road for a large city or in an industrial
    /// nation, else a track; a track if that is all it can pay).
    fn road_project(&mut self, n: u32) -> bool {
        let members: Vec<usize> = self.members[n as usize].iter().copied().collect();
        if members.len() < 2 {
            return false;
        }
        let cap = self.nation(n).capital;
        let mut cities: Vec<usize> = members.iter().copied().filter(|&p| p != cap && self.pop[p] >= 0.5 * self.np.found_population).collect();
        cities.sort_by(|&a, &b| self.pop[b].partial_cmp(&self.pop[a]).unwrap().then(a.cmp(&b)));
        cities.truncate(self.np.railway_cities.max(2) as usize + 4);
        let motor = self.nation(n).era >= era::MOTOR;
        let industrial = self.nation(n).era >= era::IND;
        let treasury = self.nation(n).treasury;
        for c in cities {
            let want = if motor && self.pop[c] >= BIG_CITY && self.pop[cap] >= BIG_CITY {
                3
            } else if industrial || self.pop[c] >= PAVED_CITY {
                2
            } else {
                1
            };
            for q in (1..=want).rev() {
                let Some(path) = self.road_route(n, c, cap, q) else { break };
                let cost = self.road_cost_of(n, &path, q);
                if cost <= 0.0 {
                    // Already as good as wanted.
                    break;
                }
                let first = q == 3 && !self.roads.values().any(|r| r.0 == 3);
                if cost <= treasury && self.build_road(n, c, cap, q, true) {
                    if first {
                        let (a, b) = (self.nation(n).name.clone(), self.provs[c].name.clone());
                        self.event("highway", n, 0, Some(c), format!("{a} opens the world's first highway, to {b}"));
                    }
                    return true;
                }
            }
        }
        false
    }

    fn build_airport(&mut self, n: u32, p: usize) {
        self.nations[n as usize - 1].treasury -= AIRPORT_BUILD * self.np.rail_cost.max(0.0) * self.prod(n);
        let first = self.airports.is_empty();
        self.airports.insert(p, self.year);
        self.stale[n as usize] = true;
        if self.airports.len() <= 5 && self.airports.keys().filter(|&&q| self.owner[q] == n).count() == 1 {
            let (a, b) = (self.nation(n).name.clone(), self.provs[p].name.clone());
            self.event("airport", n, 0, Some(p), format!("{a} opens {}airport at {b}", if first { "the world's first " } else { "its first " }));
        }
    }

    /// A nation in the air age opens an airport at its capital, then at its big cities.
    fn airport_project(&mut self, n: u32) {
        let cost = AIRPORT_BUILD * self.np.rail_cost.max(0.0) * self.prod(n);
        if self.nation(n).treasury < 2.0 * cost || !self.can_keep(n, AIRPORT_UPKEEP * self.np.rail_cost.max(0.0) * self.prod(n)) {
            return;
        }
        let members: Vec<usize> = self.members[n as usize].iter().copied().collect();
        let have = members.iter().filter(|p| self.airports.contains_key(p)).count();
        if have >= (1 + members.len() / 40).min(8) {
            return;
        }
        let cap = self.nation(n).capital;
        let mut cities: Vec<usize> = members.iter().copied().filter(|&p| !self.airports.contains_key(&p) && (p == cap || self.pop[p] >= BIG_CITY)).collect();
        cities.sort_by(|&a, &b| (b == cap).cmp(&(a == cap)).then(self.pop[b].partial_cmp(&self.pop[a]).unwrap()).then(a.cmp(&b)));
        if let Some(&p) = cities.first() {
            self.build_airport(n, p);
        }
    }

    /// Set a nation's era from its technology, with events for the first nations into each era.
    /// Set a nation's era from the institutions its provinces have embraced
    /// (eras never go back), keep its technology within the era, and log
    /// the first nations into each era.
    fn set_era(&mut self, n: u32) {
        let k = n as usize - 1;
        let old = self.nations[k].era;
        let mut new = old;
        let share = self.np.institution_share.clamp(0.01, 1.0);
        while (new as usize) < N_INST && self.born[new as usize].is_some() && self.embraced_share(n, new as usize) + 1e-9 >= share {
            new += 1;
        }
        // Technology cannot pass the next era's year before its institution is embraced.
        let mut tech = self.nations[k].tech;
        if (new as usize) < N_INST {
            tech = tech.min(self.era_year(new + 1) as f64 - 0.1);
        }
        self.nations[k].tech = tech;
        if new == old {
            return;
        }
        self.nations[k].era = new;
        self.stale[n as usize] = true;
        for e in old + 1..=new {
            let rank = self.era_firsts[e as usize];
            self.era_firsts[e as usize] += 1;
            if rank >= 3 {
                continue;
            }
            let name = self.nations[k].name.clone();
            let text = match (e, rank) {
                (era::FERT, 0) => format!("{name} is the first to make synthetic fertilizer: its farmland will feed far more people"),
                (era::IND, 0) => format!("{name} is the first to industrialise"),
                (era::MOTOR, 0) => format!("{name} enters the motor age first: cars and highways"),
                (era::AIR, 0) => format!("{name} enters the air age first: airports and air routes"),
                _ => format!("{name} enters the {} era", ERA_NAMES[e as usize]),
            };
            self.event("era", n, 0, None, text);
        }
    }

    /// Technology: each nation heads for a level set by its rank in wealth
    /// per head and in people (the richest and largest up to
    /// `tech_lead_years` ahead of the calendar, the poorest and smallest up to
    /// `TECH_LAG` years behind), raised by what its neighbours know. Behind it, a nation catches
    /// up faster the further behind it is; ahead of it, it creeps on slowly.
    fn advance_tech(&mut self, dt: f64) {
        let living: Vec<u32> = (1..=self.nations.len() as u32).filter(|&m| self.alive(m)).collect();
        if living.is_empty() {
            return;
        }
        let pops: HashMap<u32, f64> = living.iter().map(|&m| (m, self.members[m as usize].iter().map(|&p| self.pop[p]).sum::<f64>())).collect();
        // Rank of each nation by wealth per head and by people (0 last, 1 first).
        let rank = |key: &dyn Fn(u32) -> f64| -> HashMap<u32, f64> {
            let mut v = living.clone();
            v.sort_by(|&a, &b| key(a).partial_cmp(&key(b)).unwrap().then(a.cmp(&b)));
            let d = (v.len().max(2) - 1) as f64;
            v.iter().enumerate().map(|(i, &m)| (m, i as f64 / d)).collect()
        };
        let wealth = rank(&|m| self.nation(m).income.max(0.0) / pops[&m].max(1.0));
        let power = rank(&|m| pops[&m]);
        let old: Vec<f64> = self.nations.iter().map(|x| x.tech).collect();
        let ceiling = self.now() + self.np.tech_lead_years;
        let spread = self.np.tech_spread.max(0.0);
        for &m in &living {
            let k = m as usize - 1;
            let t = old[k];
            let score = 0.65 * wealth[&m] + 0.35 * power[&m];
            let neigh = self.members[m as usize]
                .iter()
                .flat_map(|&p| self.adj[p].iter().map(|e| self.owner[e.to as usize]))
                .filter(|&o| o > 0 && o != m)
                .map(|o| old[o as usize - 1])
                .fold(t, f64::max);
            let target = (ceiling - TECH_LAG * (1.0 - score).powf(1.3)).max(neigh - 80.0).min(ceiling);
            let gap = target - t;
            let gain = if gap > 0.0 { (dt * (1.0 + spread * gap)).min(gap) } else { 0.2 * dt };
            if t < ceiling {
                self.nations[k].tech = (t + gain).min(ceiling);
            }
            self.set_era(m);
        }
    }

    /// Taxes in, army and infrastructure out; bankruptcy when deep in debt.
    fn economy(&mut self, dt: f64) {
        let nn = self.nations.len();
        let mut spend = vec![0.0; nn + 1];
        // Building and upkeep cost labour: they scale with the payer's productivity.
        let wage: Vec<f64> = std::iter::once(0.0).chain((1..=nn as u32).map(|m| if self.alive(m) { self.prod(m) } else { 0.0 })).collect();
        let rc = self.np.road_cost.max(0.0);
        let mut decay = Vec::new();
        for (&(a, b), &(q, _)) in &self.roads {
            let c = ROAD_UPKEEP[q as usize] * self.km(a, b) * rc;
            let (oa, ob) = (self.owner[a] as usize, self.owner[b] as usize);
            spend[oa] += 0.5 * c * wage[oa];
            spend[ob] += 0.5 * c * wage[ob];
            if oa == 0 && ob == 0 {
                decay.push((a, b));
            }
        }
        // Roads no one keeps fall into ruin.
        for key in decay {
            if self.rng.f64() < dt / 40.0 {
                let r = self.roads.get_mut(&key).unwrap();
                r.0 -= 1;
                if r.0 == 0 {
                    self.roads.remove(&key);
                }
            }
        }
        let lc = self.np.rail_cost.max(0.0);
        for l in self.lines.iter().filter(|l| l.is_open()) {
            let share = (RAIL_UPKEEP_KM * l.km + STATION_UPKEEP * l.stations().count() as f64) * lc / l.path.len().max(1) as f64;
            for &p in &l.path {
                let o = self.owner[p] as usize;
                spend[o] += share * wage[o];
            }
        }
        for &p in self.airports.keys() {
            let o = self.owner[p] as usize;
            spend[o] += AIRPORT_UPKEEP * lc * wage[o];
        }
        for &p in self.ports.keys() {
            let o = self.owner[p] as usize;
            spend[o] += ports::PORT_UPKEEP * lc * wage[o];
        }
        let tax = self.np.tax.max(0.0);
        for m in 1..=nn as u32 {
            if !self.alive(m) {
                continue;
            }
            let prod = self.prod(m);
            // Ports add trade: up to 30% more from a perfect harbour (double for merchant republics).
            let trade = if self.has_nation_tag(m, "merchant_republic") { 0.6 } else { 0.3 };
            let income: f64 = self.members[m as usize]
                .iter()
                .map(|&p| self.pop[p] * self.integ[p] * if self.ports.contains_key(&p) { 1.0 + trade * self.harbour[p] } else { 1.0 })
                .sum::<f64>()
                * prod
                * tax;
            let x = &mut self.nations[m as usize - 1];
            // A full treasury is spent on the court, the army and the cities.
            let surplus = 0.5 * (x.treasury - RESERVE_YEARS * income).max(0.0);
            let upkeep = ARMY * income + spend[m as usize] + surplus;
            x.income = income;
            x.upkeep = upkeep;
            x.infra = spend[m as usize];
            x.treasury += (income - upkeep) * dt;
            if x.treasury < -(2.0 * income).max(50_000.0) && self.t >= x.debt_until {
                self.bankrupt(m);
            }
        }
    }

    /// A nation that can't pay its debts: it closes its newest railway lines
    /// and lets its costliest roads decay a grade until its upkeep is within
    /// its means, writes off half its debt and is less stable for ten years.
    fn bankrupt(&mut self, n: u32) {
        let k = n as usize - 1;
        self.nations[k].treasury *= 0.5;
        self.nations[k].debt_until = self.t + 10 * MONTHS;
        let wage = self.prod(n);
        let (rc, lc) = (self.np.road_cost.max(0.0) * wage, self.np.rail_cost.max(0.0) * wage);
        let mut over = self.nations[k].infra - INFRA_SHARE * self.nations[k].income;
        let (mut lines, mut roads, mut named) = (0, 0, String::new());
        let mut touched = Vec::new();
        for _ in 0..40 {
            if over <= 0.0 {
                break;
            }
            // The newest line through its land, else its costliest road.
            let line = (0..self.lines.len()).filter(|&i| self.lines[i].is_open() && self.lines[i].path.iter().any(|&p| self.owner[p] == n)).max_by_key(|&i| (self.lines[i].opened, i));
            let road = self
                .roads
                .iter()
                .filter(|(&(a, b), _)| self.owner[a] == n || self.owner[b] == n)
                .map(|(&key, &(q, _))| (key, q, (ROAD_UPKEEP[q as usize] - ROAD_UPKEEP[q as usize - 1]) * self.km(key.0, key.1) * 0.5 * (u8::from(self.owner[key.0] == n) + u8::from(self.owner[key.1] == n)) as f64 * rc))
                .max_by(|x, y| x.2.partial_cmp(&y.2).unwrap().then(y.0.cmp(&x.0)));
            if let Some(i) = line.filter(|_| lines < 3) {
                let l = &self.lines[i];
                let mine = l.path.iter().filter(|&&p| self.owner[p] == n).count() as f64 / l.path.len().max(1) as f64;
                over -= (RAIL_UPKEEP_KM * l.km + STATION_UPKEEP * l.stations().count() as f64) * lc * mine;
                if lines == 0 {
                    named = format!("the railway line from {} to {}", self.provs[l.path[0]].name, self.provs[*l.path.last().unwrap()].name);
                }
                self.lines[i].closed = Some(self.year);
                touched.extend(self.lines[i].path.iter().copied());
                lines += 1;
            } else if let Some((key, q, saved)) = road {
                over -= saved;
                if q <= 1 {
                    self.roads.remove(&key);
                } else {
                    self.roads.insert(key, (q - 1, self.year));
                }
                touched.extend([key.0, key.1]);
                roads += 1;
            } else {
                break;
            }
        }
        if lines > 0 {
            self.rebuild_rail();
        }
        self.touch(&touched);
        let mut cuts = Vec::new();
        match lines {
            0 => {}
            1 => cuts.push(format!("closes {named}")),
            _ => cuts.push(format!("closes {lines} railway lines, among them {named}")),
        }
        match roads {
            0 => {}
            1 => cuts.push("lets a road decay".to_string()),
            _ => cuts.push(format!("lets {roads} roads decay")),
        }
        let name = self.nations[k].name.clone();
        let what = if cuts.is_empty() { String::new() } else { format!(": it {}", cuts.join(" and ")) };
        self.event("bankruptcy", n, 0, None, format!("{name} goes bankrupt{what}"));
    }

    /// Run one step (`step_len` years).
    pub fn step(&mut self, progress: &dyn Fn(f32, &str)) {
        if self.done() {
            return;
        }
        let dm = self.step_months();
        let dt = dm as f64 / MONTHS as f64;
        self.apply_directives(dm);
        let y = self.year;
        // Founding slows once the world's seafarers have spread (by the calendar).
        let ship = self.year >= self.np.shipping_year;
        let span = ((self.np.start_date - self.np.start_year).max(1) as i64 * MONTHS) as f32;
        if self.t >= self.next_progress {
            while self.next_progress <= self.t {
                self.next_progress += 50 * MONTHS;
            }
            progress(((self.t - self.np.start_year as i64 * MONTHS) as f32 / span).clamp(0.0, 1.0) * 0.95, &format!("Year {y}: {} nations, {} era", self.nations.iter().filter(|x| x.ended.is_none()).count(), self.era()));
        }
        let n = self.provs.len();
        // City pins that have run their time.
        let t = self.t;
        self.pins.retain(|x| x.until.map_or(true, |u| t < u));
        // Institutions are born and spread; eras follow them. Then access and
        // integration, the treasury, tribute, ports and reforms.
        self.births();
        self.imperial_succession();
        self.spread_institutions(dt);
        self.advance_tech(dt);
        self.refresh_access();
        self.economy(dt);
        self.tribute(dt);
        self.port_growth(dt);
        self.reforms(dt);
        self.update_attraction();
        // 1. growth: attraction scales how many people a province holds; an
        // industrial owner doubles it and grows faster, fertilizer adds more.
        let owner_era = |s: &NationSim, p: usize| if s.owner[p] > 0 { s.nation(s.owner[p]).era } else { 0 };
        let kbase: Vec<f64> = (0..n)
            .map(|p| (self.cap[p] + self.cap_extra[p]) * if owner_era(self, p) >= era::IND { 2.0 } else { 1.0 } * self.fert_mult(self.owner[p]))
            .collect();
        for p in 0..n {
            if self.pop[p] > 0.0 {
                // Devastation stops growth (and pushes people out, below) but
                // leaves the land able to hold them once it heals.
                let k = kbase[p] * pull_of((self.attr[p] - self.devastation[p]).clamp(-1.0, 1.0));
                let rate = self.np.growth + if owner_era(self, p) >= era::IND { self.np.industrial_growth } else { 0.0 };
                let r = rate * (1.0 + 0.5 * self.devastation[p]).max(0.0);
                if k > 1.0 {
                    self.pop[p] = (self.pop[p] + self.pop[p] * r * dt * (1.0 - self.pop[p] / k).max(if self.pop[p] > k { -1.0 } else { 0.0 })).max(0.0);
                } else {
                    // Nothing holds people here any more: they leave.
                    self.pop[p] *= 0.9f64.powf(dt);
                }
            }
        }
        // Migration within each nation toward its attractive, well-connected
        // provinces, as far as they have room; devastated or shunned
        // provinces send more.
        if self.np.migration > 0.0 {
            let m = (self.np.migration * dt).clamp(0.0, 0.5);
            for k in 1..=self.nations.len() {
                if self.nations[k - 1].ended.is_some() {
                    continue;
                }
                let members: Vec<usize> = self.members[k].iter().copied().collect();
                let room: Vec<f64> = members
                    .iter()
                    .map(|&p| if self.attr[p] > 0.0 { (kbase[p] * pull_of((self.attr[p] - self.devastation[p]).clamp(-1.0, 1.0)) - self.pop[p]).max(0.0) } else { 0.0 })
                    .collect();
                let weight: Vec<f64> = members.iter().zip(&room).map(|(&p, r)| r * (0.5 + self.integ[p])).collect();
                let total_room: f64 = room.iter().sum();
                let total_weight: f64 = weight.iter().sum();
                if total_room <= 0.0 || total_weight <= 0.0 {
                    continue;
                }
                let want: Vec<f64> = members.iter().map(|&p| if self.attr[p] <= 0.0 { self.pop[p] * (m * (1.0 + 4.0 * (-self.attr[p]))).min(0.5) } else { 0.0 }).collect();
                let total_want: f64 = want.iter().sum();
                if total_want <= 0.0 {
                    continue;
                }
                let moved = total_want.min(total_room);
                let gain: Vec<f64> = weight.iter().zip(&room).map(|(w, r)| (w / total_weight * moved).min(*r)).collect();
                // Those who find no room stay.
                let left = gain.iter().sum::<f64>() / total_want;
                for (i, &p) in members.iter().enumerate() {
                    self.pop[p] += gain[i] - want[i] * left;
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
                    self.expand(m);
                }
            }
        }
        // 4. breakups
        for &m in &living {
            // Members of a feudal empire and eternal nations never break apart.
            if !self.alive(m) || self.members[m as usize].len() < 4 || self.empire_of(m).is_some() || self.has_nation_tag(m, "eternal") {
                continue;
            }
            let members: Vec<usize> = self.members[m as usize].iter().copied().collect();
            let total: f64 = members.iter().map(|&p| self.pop[p]).sum::<f64>().max(1.0);
            let foreign: f64 = members.iter().map(|&p| self.pop[p] * self.cdist(m, p)).sum::<f64>() / total;
            let size = (members.len() as f64 / 60.0).min(3.0);
            let spread = members.iter().map(|&p| self.reach_cost(m, p)).sum::<f64>() / members.len() as f64 / (2.0 * self.np.reach_km.max(50.0));
            let instability = 1.2 * foreign + 0.4 * size + 0.4 * spread.min(3.0);
            let e = self.nation(m).era;
            let era_f = if e >= era::IND { 0.5 } else if e >= era::GUN { 0.7 } else { 1.0 };
            let debt = if self.t < self.nation(m).debt_until { 1.6 } else { 1.0 };
            let reform = if self.t < self.nation(m).reform_until { 1.5 } else { 1.0 };
            let calm = if self.world_tag("stable_realms") { 0.5 } else { 1.0 };
            let chance = self.np.collapse_rate * dt / 2.0 * instability * instability * era_f * debt * reform * calm / self.stability_mult(m).max(0.05);
            if self.rng.f64() < chance {
                self.split(m, None);
            }
        }
        // 5. culture feedback, faster where the ruler's reach is strong
        let a0 = (self.np.assimilation * dt).clamp(0.0, 1.0);
        if a0 > 0.0 {
            for p in 0..n {
                let o = self.owner[p];
                if o == 0 || self.pop[p] <= 0.0 {
                    continue;
                }
                let prim = self.nation(o).primary;
                if prim == 0 {
                    continue;
                }
                let a = (a0 * (0.5 + self.integ[p])).min(1.0);
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
        // 6. building: roads, railways (industry) and airports (air age), each
        // nation on its own schedule.
        let rail_every = self.np.railway_every_years.max(1) as i64 * MONTHS;
        let road_every = self.np.road_every_years.max(1) as i64 * MONTHS;
        let living: Vec<u32> = (1..=self.nations.len() as u32).filter(|&m| self.alive(m)).collect();
        for m in living {
            // Up to three projects at a time, as far as the treasury goes.
            if self.t >= self.nation(m).next_road {
                self.nations[m as usize - 1].next_road = self.t + road_every;
                for _ in 0..3 {
                    if !self.alive(m) || !self.road_project(m) {
                        break;
                    }
                }
                if self.nation(m).era >= era::AIR {
                    self.airport_project(m);
                }
            }
            if self.nation(m).era >= era::IND && self.t >= self.nation(m).next_rail {
                self.nations[m as usize - 1].next_rail = self.t + rail_every;
                for _ in 0..3 {
                    if !self.railway_project(m) {
                        break;
                    }
                }
            }
        }
        self.t += dm;
        self.year = self.t.div_euclid(MONTHS) as i32;
        if self.t >= self.next_flush || self.done() {
            self.flush_wars();
            self.next_flush += 50 * MONTHS;
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
        // Infrastructure per nation: road km by quality (a border counts half
        // for each owner), railway km over its land, stations and airports.
        let nn = self.nations.len();
        let mut road_km = vec![[0.0f64; 3]; nn + 1];
        for (&(a, b), &(q, _)) in &self.roads {
            let km = self.km(a, b);
            road_km[self.owner[a] as usize][q as usize - 1] += 0.5 * km;
            road_km[self.owner[b] as usize][q as usize - 1] += 0.5 * km;
        }
        let mut rail_km = vec![0.0f64; nn + 1];
        for l in self.lines.iter().filter(|l| l.is_open()) {
            for w in l.path.windows(2) {
                let km = self.km(w[0], w[1]);
                rail_km[self.owner[w[0]] as usize] += 0.5 * km;
                rail_km[self.owner[w[1]] as usize] += 0.5 * km;
            }
        }
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
            let integ = if pop > 0.0 { m.iter().map(|&p| self.pop[p] * self.integ[p]).sum::<f64>() / pop } else { 0.0 };
            let rk = road_km[id as usize];
            out.push(json!({
                "id": id, "name": x.name, "government": if x.ended.is_some() { "" } else { government(m.len()) }, "color": x.color,
                "capital": self.provs[x.capital].id, "capital_name": self.provs[x.capital].name,
                "primary_culture": x.primary, "culture_name": self.culture_name(x.primary),
                "provinces": m.len(), "population": pop.max(0.0).round() as u64, "overseas_provinces": overseas,
                "founded": x.founded, "ended": x.ended, "fate": x.fate, "fate_other": x.other, "parent": x.parent,
                "tech": (x.tech * 10.0).round() / 10.0, "era": ERA_NAMES[x.era as usize], "era_index": x.era,
                "treasury": x.treasury.round(), "income": x.income.round(), "upkeep": x.upkeep.round(),
                "integration": (integ * 1000.0).round() / 1000.0, "bankrupt": self.t < x.debt_until,
                "road_km": { "track": rk[0].round(), "paved": rk[1].round(), "highway": rk[2].round() },
                "rail_km": rail_km[id as usize].round(),
                "railway_provinces": m.iter().filter(|&&p| self.rail[p] > 0).count(),
                "stations": m.iter().filter(|&&p| self.rail[p] >= 2).count(),
                "airports": m.iter().filter(|p| self.airports.contains_key(p)).count(),
                "ports": m.iter().filter(|p| self.ports.contains_key(p)).count(),
                "liege": x.liege, "rank": RANKS[x.rank as usize], "realm": if x.ended.is_none() { self.realm_of(id) } else { id },
                "empire": if x.ended.is_none() { self.empire_of(id).map(|k| self.empires[k].name.clone()) } else { None },
                "reforming": self.t < x.reform_until,
                "institutions": (0..N_INST).map(|i| (self.embraced_share(id, i) * 100.0).round() / 100.0).collect::<Vec<_>>(),
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
        let mut f_road = vec![0u8; n];
        let mut f_air = vec![0u8; n];
        let mut f_era = vec![0u8; n];
        let mut f_port = vec![0u8; n];
        let mut f_realm = vec![0u16; n];
        let mut f_imperial = vec![0u8; n];
        let mut f_inst = vec![0u8; n];
        let mut f_presence = vec![0f32; n];
        let mut f_pop = vec![0f32; n];
        // Province area from the cell count is not needed: density uses the cells' share.
        let mut cells = vec![0u32; self.provs.len()];
        for i in 0..n {
            if let Some(&p) = self.index.get(&self.prov_field[i]) {
                cells[p] += 1;
            }
        }
        let road: Vec<u8> = (0..self.provs.len()).map(|p| self.road_at(p)).collect();
        // Per province: realm (top liege in a feudal empire, else the owner),
        // imperial territory, embraced institutions and the newest one's presence.
        let realm: Vec<u16> = (0..self.provs.len()).map(|p| if self.owner[p] > 0 { self.realm_of(self.owner[p]).min(u16::MAX as u32) as u16 } else { 0 }).collect();
        let mut imperial = vec![0u8; self.provs.len()];
        for e in self.empires.iter().filter(|e| e.dissolved.is_none()) {
            for &p in &e.territory {
                imperial[p] = 1;
            }
        }
        let newest = (0..N_INST).rev().find(|&i| self.born[i].is_some());
        for i in 0..n {
            let Some(&p) = self.index.get(&self.prov_field[i]) else { continue };
            f_owner[i] = self.owner[p].min(u16::MAX as u32) as u16;
            f_cult[i] = self.major[p].min(u16::MAX as u32) as u16;
            f_rail[i] = self.rail[p];
            f_road[i] = road[p];
            f_air[i] = u8::from(self.airports.contains_key(&p));
            // Era of the owner, plus one (0: no owner).
            f_era[i] = if self.owner[p] > 0 { self.nation(self.owner[p]).era + 1 } else { 0 };
            f_port[i] = if self.ports.contains_key(&p) {
                2
            } else if self.harbour[p] >= self.np.port_quality {
                1
            } else {
                0
            };
            f_realm[i] = realm[p];
            f_imperial[i] = imperial[p];
            f_inst[i] = (0..N_INST).filter(|&k| self.presence[p][k] >= EMBRACED).fold(0u8, |a, k| a | (1 << k));
            if let Some(k) = newest {
                f_presence[i] = self.presence[p][k];
            }
            if self.provs[p].land {
                f_pop[i] = (self.pop[p] / cells[p].max(1) as f64) as f32;
            }
        }
        let mut f = Fields::default();
        f.put("owner", Field::U16(f_owner));
        f.put("nation_culture", Field::U16(f_cult));
        f.put("railway", Field::U8(f_rail));
        f.put("road", Field::U8(f_road));
        f.put("airport", Field::U8(f_air));
        f.put("nation_era", Field::U8(f_era));
        f.put("port", Field::U8(f_port));
        f.put("realm", Field::U16(f_realm));
        f.put("imperial", Field::U8(f_imperial));
        f.put("institutions", Field::U8(f_inst));
        f.put("institution", Field::F32(f_presence));
        f.put("nation_population", Field::F32(f_pop));
        let f_attr: Vec<f32> = (0..n).map(|i| self.index.get(&self.prov_field[i]).map_or(0.0, |&p| self.attr[p] as f32)).collect();
        f.put("nation_attraction", Field::F32(f_attr));
        f
    }

    fn line_rows(&self) -> Vec<Value> {
        self.lines
            .iter()
            .map(|l| {
                let (a, b) = (&self.provs[l.path[0]].name, &self.provs[*l.path.last().unwrap()].name);
                json!({
                    "id": l.id, "name": format!("{a}–{b}"), "owner": l.owner, "opened": l.opened, "closed": l.closed, "km": l.km.round(),
                    "provinces": l.path.iter().map(|&p| self.provs[p].id).collect::<Vec<_>>(),
                    "stations": l.stations().map(|p| self.provs[p].id).collect::<Vec<_>>(),
                })
            })
            .collect()
    }

    fn road_rows(&self) -> Vec<Value> {
        self.roads
            .iter()
            .map(|(&(a, b), &(q, built))| {
                json!({ "from": self.provs[a].id, "to": self.provs[b].id, "quality": q, "kind": ROAD_NAMES[q as usize], "built": built, "km": self.km(a, b).round() })
            })
            .collect()
    }

    fn airport_rows(&self) -> Vec<Value> {
        self.airports.iter().map(|(&p, &y)| json!({ "province": self.provs[p].id, "name": self.provs[p].name, "owner": self.owner[p], "opened": y })).collect()
    }

    /// Stations, with the open lines serving each (two or more: a junction).
    fn station_rows(&self) -> Vec<Value> {
        let mut at: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
        for l in self.lines.iter().filter(|l| l.is_open()) {
            for p in l.stations() {
                at.entry(p).or_default().push(l.id);
            }
        }
        at.into_iter()
            .map(|(p, ls)| json!({ "province": self.provs[p].id, "name": self.provs[p].name, "owner": self.owner[p], "junction": ls.len() > 1, "lines": ls }))
            .collect()
    }

    /// World totals of the infrastructure: road km by quality, open lines and their km, junctions, airports.
    fn transport_summary(&self) -> Value {
        let mut rk = [0.0f64; 3];
        for (&(a, b), &(q, _)) in &self.roads {
            rk[q as usize - 1] += self.km(a, b);
        }
        let open: Vec<&Line> = self.lines.iter().filter(|l| l.is_open()).collect();
        json!({
            "road_km": { "track": rk[0].round(), "paved": rk[1].round(), "highway": rk[2].round() },
            "railway_lines": open.len(), "railway_lines_closed": self.lines.len() - open.len(),
            "rail_km": open.iter().map(|l| l.km).sum::<f64>().round(),
            "stations": self.rail.iter().filter(|&&r| r >= 2).count(),
            "junctions": self.rail.iter().filter(|&&r| r == 3).count(),
            "airports": self.airports.len(),
            "ports": self.ports.len(),
        })
    }

    /// Living nations per era.
    fn era_counts(&self) -> Value {
        let mut c = [0usize; 7];
        for x in self.nations.iter().filter(|x| x.ended.is_none()) {
            c[x.era as usize] += 1;
        }
        Value::Object(ERA_NAMES.iter().zip(c).filter(|x| x.1 > 0).map(|(k, v)| (k.to_string(), json!(v))).collect())
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
                Effect::Aggression(n, f) => json!({ "effect": "aggression", "nation": n, "factor": f, "until": date(*until) }),
                Effect::Toward(n, p) => json!({ "effect": "expand_toward", "nation": n, "province": self.provs[*p].id, "until": date(*until) }),
                Effect::War(a, b) => json!({ "effect": "war", "nation": a, "target": b, "until": date(*until) }),
                Effect::Peace(a, b) => json!({ "effect": "peace", "nation": a, "target": b, "until": date(*until) }),
                Effect::Stability(n, f) => json!({ "effect": "stability", "nation": n, "factor": f, "until": date(*until) }),
            })
            .collect();
        let leader = self.leader().map(|m| json!({ "nation": m, "name": self.nation(m).name, "tech": (self.nation(m).tech * 10.0).round() / 10.0, "era": ERA_NAMES[self.nation(m).era as usize] }));
        let summary = json!({
            "stage": "nations",
            "year": self.year, "month": self.month(), "time": self.t, "step_months": self.step_months(),
            "start_year": self.np.start_year, "end_year": self.np.start_date, "era": self.era(),
            "leader": leader, "eras": self.era_counts(),
            "institutions": self.institution_rows(), "next_institution": self.next_institution(), "pending_institution": self.pending_institution(),
            "tags": self.tag_rows(), "empires": self.empire_rows(),
            "population": self.pop.iter().sum::<f64>().round(),
            "land_provinces": land, "ruled_provinces": ruled,
            "nations": nations, "nations_ever": self.nations.len(),
            "regions": regions, "effects": effects,
            "railways": self.lines.iter().filter(|l| l.is_open()).count(),
            "transport": self.transport_summary(),
            "pins": self.pin_rows(),
            "cities": self.city_rows(20),
            "ruins": self.ruined.iter().filter(|&&r| r).count(),
            "events": self.events.iter().rev().take(20).rev().cloned().collect::<Vec<_>>(),
            "directives": self.applied,
            "queued": self.directives[self.next_directive..].iter().map(|d| json!({ "year": d.at, "month": d.month.max(1), "action": d.action, "args": d.args, "note": d.note, "by": d.by })).collect::<Vec<_>>(),
        });
        (summary, self.fields())
    }

    pub fn finish(self) -> StepOutput {
        let provinces: Vec<Value> = (0..self.provs.len())
            .filter(|&p| self.provs[p].land)
            .map(|p| {
                let sh: Vec<Value> = self.shares[p].iter().filter(|x| x.1 >= 0.05).map(|x| json!([x.0, (x.1 * 1000.0).round() / 1000.0])).collect();
                json!({ "id": self.provs[p].id, "owner": self.owner[p], "culture": self.major[p], "shares": sh, "population": self.pop[p].max(0.0).round() as u64,
                        "railway": self.rail[p], "road": self.road_at(p), "airport": self.airports.contains_key(&p), "port": self.ports.contains_key(&p),
                        "harbour": (self.harbour[p] * 100.0).round() / 100.0, "integration": (self.integ[p] * 1000.0).round() / 1000.0,
                        "institutions": (0..N_INST).filter(|&k| self.presence[p][k] >= EMBRACED).map(|k| INSTITUTIONS[k]).collect::<Vec<_>>(),
                        "realm": if self.owner[p] > 0 { self.realm_of(self.owner[p]) } else { 0 } })
            })
            .collect();
        let nations = self.nation_rows(false);
        let living = self.nations.iter().filter(|x| x.ended.is_none()).count();
        let ruled = (0..self.provs.len()).filter(|&p| self.provs[p].land && self.owner[p] > 0).count();
        let land = (0..self.provs.len()).filter(|&p| self.provs[p].land).count();
        let count = |k: &str| self.events.iter().filter(|e| e["event"] == k).count();
        let leader = self.leader().map(|m| json!({ "nation": m, "name": self.nation(m).name, "tech": (self.nation(m).tech * 10.0).round() / 10.0, "era": ERA_NAMES[self.nation(m).era as usize] }));
        let ruins: Vec<Value> = (0..self.provs.len()).filter(|&p| self.ruined[p]).map(|p| json!({ "province": self.provs[p].id, "name": self.provs[p].name, "population": self.pop[p].round() as u64, "peak": self.peak[p].0.round() as u64, "peak_year": self.peak[p].1 })).collect();
        let table = json!({
            "nations": nations,
            "provinces": provinces,
            "events": self.events,
            "railways": self.line_rows(),
            "stations": self.station_rows(),
            "roads": self.road_rows(),
            "airports": self.airport_rows(),
            "ports": self.port_rows(),
            "titles": self.title_rows(),
            "institutions": self.institution_rows(),
            "empires": self.empire_rows(),
            "cities": self.city_rows(100),
            "pins": self.pin_rows(),
            "ruins": ruins,
        });
        StepOutput {
            fields: self.fields(),
            meta: json!({
                "nations": living,
                "nations_ever": self.nations.len(),
                "start_year": self.np.start_year,
                "start_date": self.np.start_date,
                "era": self.era(),
                "leader": leader,
                "eras": self.era_counts(),
                "ruled_share": ruled as f64 / land.max(1) as f64,
                "population": self.pop.iter().sum::<f64>().round(),
                "railways": self.lines.iter().filter(|l| l.is_open()).count(),
                "railway_provinces": self.rail.iter().filter(|&&r| r > 0).count(),
                "transport": self.transport_summary(),
                "conquests": count("conquest"),
                "independences": count("independence"),
                "colonies": count("colony"),
                "bankruptcies": count("bankruptcy"),
                "reforms": count("reform"),
                "institutions": self.institution_rows(),
                "empires": self.empire_rows(),
                "tags": self.tag_rows(),
                "metropolises": self.metropolis.iter().filter(|&&m| m).count(),
                "ruins": self.ruined.iter().filter(|&&r| r).count(),
                "directives": self.applied,
                "table": table,
            }),
        }
    }
}

fn empty_output(n: usize) -> StepOutput {
    let mut f = Fields::default();
    f.put("owner", Field::U16(vec![0; n]));
    f.put("nation_culture", Field::U16(vec![0; n]));
    f.put("railway", Field::U8(vec![0; n]));
    f.put("road", Field::U8(vec![0; n]));
    f.put("airport", Field::U8(vec![0; n]));
    f.put("nation_era", Field::U8(vec![0; n]));
    f.put("port", Field::U8(vec![0; n]));
    f.put("realm", Field::U16(vec![0; n]));
    f.put("imperial", Field::U8(vec![0; n]));
    f.put("institutions", Field::U8(vec![0; n]));
    f.put("institution", Field::F32(vec![0.0; n]));
    f.put("nation_population", Field::F32(vec![0.0; n]));
    f.put("nation_attraction", Field::F32(vec![0.0; n]));
    StepOutput {
        fields: f,
        meta: json!({ "nations": 0, "nations_ever": 0, "table": { "nations": [], "provinces": [], "events": [], "railways": [], "stations": [], "roads": [], "airports": [], "ports": [], "titles": [] } }),
    }
}
