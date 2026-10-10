//! Institutions (Stage 4), in the manner of EU4: the ideas that open each
//! era — gunpowder, navigation, industrialisation, synthetic fertilizer,
//! motorisation and aviation.
//!
//! - **Birth:** once the calendar reaches an institution's year (the era
//!   year in the parameters) and the one before it has been born, it is born
//!   in one province: a candidate chosen by the user or the AI guide (an
//!   `institution_birth` directive), or else drawn by chance among the best
//!   candidates — populous, well-run provinces of advanced nations, with a
//!   preference that fits the idea (a good harbour for navigation, coal and
//!   iron for industrialisation, farmland for fertilizer, oil for motors,
//!   big cities for aviation).
//! - **Spread:** each province holds a presence (0–1) of each born
//!   institution. It grows from the strongest contact: neighbouring provinces
//!   (faster along better roads), nearby shores of other landmasses (coastal
//!   sailing, slow), the same railway line, ports linked by sea, airports,
//!   and the nation's capital (and its liege's). A nation takes it up only
//!   as far as its development reaches: fully when its technology is near
//!   the institution's year, hardly at all 60 years behind (rich nations
//!   first). Isolationist nations take it up slowly, reforming nations
//!   three times as fast.
//! - **Eras:** a nation enters an era once at least `institution_share` of
//!   its provinces have embraced its institution (presence ≥ 0.9). Its
//!   technology cannot pass the next era's year until it embraces the next
//!   institution.
//! - **Reforms (westernization):** a nation two or more eras behind a
//!   neighbour may reform on its model: for 40 years institutions spread
//!   through it three times as fast, at the cost of a year's income and of
//!   stability.

use super::*;

pub const INSTITUTIONS: [&str; 6] = ["gunpowder", "navigation", "industrialisation", "synthetic fertilizer", "motorisation", "aviation"];
/// Presence at which a province has embraced an institution.
pub const EMBRACED: f32 = 0.9;
/// Contact through a land border by road quality (none, track, paved, highway).
const ROAD_CONTACT: [f32; 4] = [0.5, 0.65, 0.85, 1.0];
/// Later institutions spread faster (printing, telegraph, radio, cars).
const SPEED: [f64; 6] = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
/// How far behind an institution's year a nation's technology can be and still take it up at all.
const ABSORB_YEARS: f64 = 60.0;
/// Years a reform lasts.
const REFORM_YEARS: f64 = 40.0;

impl NationSim {
    /// Era year of era e (1–6); the earliest birth year of institution e − 1.
    pub(super) fn era_year(&self, e: u8) -> i32 {
        let np = &self.np;
        match e {
            0 => i32::MIN / 2,
            1 => np.gunpowder_year,
            2 => np.shipping_year,
            3 => np.industrial_year,
            4 => np.fertilizer_year,
            5 => np.motor_year,
            _ => np.air_year,
        }
    }

    /// Whether institution i can be born now (its year has come and the one before it is born).
    fn inst_due(&self, i: usize) -> bool {
        self.born[i].is_none() && (i == 0 || self.born[i - 1].is_some()) && self.t + 1e-9 >= self.era_year(i as u8 + 1) as f64
    }

    /// Candidate birthplaces of institution i, best first, with their scores.
    pub(super) fn inst_candidates(&self, i: usize) -> Vec<(usize, f64)> {
        let living: Vec<u32> = (1..=self.nations.len() as u32).filter(|&m| self.alive(m)).collect();
        let mut order = living.clone();
        order.sort_by(|&a, &b| self.nation(a).tech.total_cmp(&self.nation(b).tech).then(a.cmp(&b)));
        let d = (order.len().max(2) - 1) as f64;
        let rank: HashMap<u32, f64> = order.iter().enumerate().map(|(k, &m)| (m, k as f64 / d)).collect();
        let mut all: Vec<(usize, f64)> = Vec::new();
        for p in 0..self.provs.len() {
            let o = self.owner[p];
            if o == 0 || !self.provs[p].land || self.pop[p] < 1.0 || (self.nation(o).era as usize) < i {
                continue;
            }
            let pr = &self.provs[p];
            let fit = match i {
                1 => {
                    if !self.ports.contains_key(&p) && self.harbour[p] < 0.8 * self.np.port_quality {
                        continue;
                    }
                    0.5 + self.harbour[p]
                }
                2 => 1.0 + if pr.coal { 0.6 } else { 0.0 } + if pr.iron { 0.4 } else { 0.0 },
                3 => 0.5 + pr.fertile,
                4 => 1.0 + if pr.oil { 0.6 } else { 0.0 },
                5 => {
                    if self.pop[p] >= BIG_CITY {
                        1.5
                    } else {
                        1.0
                    }
                }
                _ => 1.0,
            };
            let cradle = if self.has_state_tag(pr.state as u32, "institution_cradle") { 3.0 } else { 1.0 };
            let score = (0.3 + rank[&o]) * (1.0 + self.pop[p] / 10_000.0).ln() * (0.5 + self.integ[p]) * fit * cradle;
            all.push((p, score));
        }
        all.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        // Up to three per nation, so the choice spans more than one realm.
        let mut per: HashMap<u32, usize> = HashMap::new();
        let mut out = Vec::new();
        for (p, s) in all {
            let c = per.entry(self.owner[p]).or_insert(0);
            if *c < 3 {
                *c += 1;
                out.push((p, s));
            }
            if out.len() >= self.np.institution_candidates.max(1) as usize {
                break;
            }
        }
        out
    }

    fn candidate_rows(&self, i: usize) -> Vec<Value> {
        self.inst_candidates(i)
            .iter()
            .map(|&(p, s)| {
                let (la, lo) = self.provs[p].center.lat_lon();
                let o = self.owner[p];
                json!({ "province": self.provs[p].id, "name": self.provs[p].name, "owner": o, "owner_name": if o > 0 { self.nation(o).name.clone() } else { String::new() },
                        "score": (s * 100.0).round() / 100.0, "at": [(la.to_degrees() * 100.0).round() / 100.0, (lo.to_degrees() * 100.0).round() / 100.0] })
            })
            .collect()
    }

    /// The institution about to be born with no birthplace chosen yet, and its candidates
    /// (a live run can stop here to let the user choose).
    pub fn pending_institution(&self) -> Option<Value> {
        let i = (0..INSTITUTIONS.len()).find(|&i| self.inst_due(i))?;
        // Chosen already, or a choice is queued for the next step.
        let queued = self.directives[self.next_directive..].iter().any(|d| d.action == "institution_birth" && d.args["institution"].as_f64().map(|x| x.round() as usize) == Some(i));
        if self.birth_choice[i].is_some() || queued {
            return None;
        }
        let c = self.candidate_rows(i);
        if c.is_empty() {
            return None;
        }
        Some(json!({ "index": i, "name": INSTITUTIONS[i], "era": ERA_NAMES[i + 1], "year": self.era_year(i as u8 + 1), "candidates": c }))
    }

    /// The next institution not yet born, with its earliest year and (once
    /// the one before it is born) its current candidates.
    pub(super) fn next_institution(&self) -> Option<Value> {
        let i = (0..INSTITUTIONS.len()).find(|&i| self.born[i].is_none())?;
        let ready = i == 0 || self.born[i - 1].is_some();
        Some(json!({ "index": i, "name": INSTITUTIONS[i], "era": ERA_NAMES[i + 1], "earliest": self.era_year(i as u8 + 1),
                     "chosen": self.birth_choice[i].map(|c| c.map(|p| self.provs[p].id)), "candidates": if ready { self.candidate_rows(i) } else { vec![] } }))
    }

    /// Birth of the next due institution (at most one per step).
    pub(super) fn births(&mut self) {
        let Some(i) = (0..INSTITUTIONS.len()).find(|&i| self.inst_due(i)) else { return };
        let cands = self.inst_candidates(i);
        if cands.is_empty() {
            return;
        }
        let chosen = match self.birth_choice[i] {
            Some(Some(p)) if self.provs[p].land => p,
            _ => {
                let total: f64 = cands.iter().map(|c| c.1.max(1e-9)).sum();
                let mut r = self.rng.f64() * total;
                let mut pick = cands[0].0;
                for &(p, s) in &cands {
                    r -= s.max(1e-9);
                    if r <= 0.0 {
                        pick = p;
                        break;
                    }
                }
                pick
            }
        };
        self.bear(i, chosen);
    }

    pub(super) fn bear(&mut self, i: usize, p: usize) {
        self.born[i] = Some((self.year, p));
        self.presence[p][i] = 1.0;
        let o = self.owner[p];
        let (name, prov) = (INSTITUTIONS[i], self.provs[p].name.clone());
        let by = if o > 0 { format!(" ({})", self.nation(o).name) } else { String::new() };
        let mut t = name.to_string();
        t[..1].make_ascii_uppercase();
        self.event("institution", o, 0, Some(p), format!("{t} is born in {prov}{by}"));
    }

    /// Institutions spread for dt years.
    pub(super) fn spread_institutions(&mut self, dt: f64) {
        let n = self.provs.len();
        let mut rate = self.np.institution_spread.max(0.0);
        if self.world_tag("slow_institutions") {
            rate *= 0.5;
        }
        if self.world_tag("fast_institutions") {
            rate *= 2.0;
        }
        if rate <= 0.0 {
            return;
        }
        // Per province: growth multiplier (isolation, reform) and reform floor.
        let mut mult = vec![1.0f32; n];
        let mut floor = vec![0.0f32; n];
        for m in 1..=self.nations.len() as u32 {
            if !self.alive(m) {
                continue;
            }
            let (iso, reform) = (self.has_nation_tag(m, "isolationist"), self.t < self.nation(m).reform_until);
            for &p in &self.members[m as usize] {
                if iso {
                    mult[p] = 0.3;
                }
                if reform {
                    mult[p] = 3.0;
                    floor[p] = 0.5;
                }
            }
        }
        let lanes = self.port_lanes().clone();
        let open: Vec<&Line> = self.lines.iter().filter(|l| l.is_open()).collect();
        for i in 0..INSTITUTIONS.len() {
            if self.born[i].is_none() || self.saturated[i] {
                continue;
            }
            let pres: Vec<f32> = self.presence.iter().map(|x| x[i]).collect();
            // Once every settled province has it, there is nothing left to spread.
            if (0..n).all(|p| !self.provs[p].land || self.pop[p] < 1.0 || pres[p] >= 0.999) {
                self.saturated[i] = true;
                continue;
            }
            let mut pressure = vec![0.0f32; n];
            for p in 0..n {
                if pres[p] <= 0.0 {
                    continue;
                }
                for e in &self.adj[p] {
                    let q = e.to as usize;
                    let w = match e.ty {
                        edge::STRAIT => 0.3,
                        edge::IMPASSABLE => 0.25,
                        _ => ROAD_CONTACT[self.road_q(p, q) as usize],
                    };
                    pressure[q] = pressure[q].max(pres[p] * w);
                }
            }
            for l in &open {
                let m = l.stations().map(|s| pres[s]).fold(0.0f32, f32::max);
                for s in l.stations() {
                    pressure[s] = pressure[s].max(0.9 * m);
                }
            }
            // Coastal sailing to nearby shores of other landmasses (slow).
            for (p, near) in &self.coast_near {
                let m = near.iter().map(|&q| pres[q]).fold(0.0f32, f32::max);
                pressure[*p] = pressure[*p].max(0.2 * m);
            }
            for (p, near) in &lanes {
                let m = near.iter().map(|&q| pres[q]).fold(0.0f32, f32::max);
                pressure[*p] = pressure[*p].max(0.75 * m * (self.harbour[*p] as f32).max(0.3));
            }
            let air = self.airports.keys().map(|&a| pres[a]).fold(0.0f32, f32::max);
            for &a in self.airports.keys() {
                pressure[a] = pressure[a].max(air);
            }
            for m in 1..=self.nations.len() as u32 {
                if !self.alive(m) {
                    continue;
                }
                let x = self.nation(m);
                let mut cp = pres[x.capital];
                if x.liege > 0 && self.alive(x.liege) {
                    cp = cp.max(0.6 * pres[self.nation(x.liege).capital]);
                }
                if cp <= 0.0 {
                    continue;
                }
                for &p in &self.members[m as usize] {
                    pressure[p] = pressure[p].max(cp * 0.3 * (self.integ[p] as f32).max(0.2));
                }
            }
            // A nation takes up an idea only as far as its own development
            // reaches: fully when its technology is near the institution's
            // year, hardly at all a century behind it.
            let year = self.era_year(i as u8 + 1) as f64;
            let absorb: Vec<f32> = std::iter::once(0.3)
                .chain((1..=self.nations.len()).map(|m| ((1.0 - (year - self.nations[m - 1].tech) / ABSORB_YEARS).clamp(0.03, 1.0)) as f32))
                .collect();
            let k = (dt * rate * SPEED[i]) as f32;
            for p in 0..n {
                if !self.provs[p].land {
                    continue;
                }
                let pr = pressure[p].max(floor[p]);
                if pr > 0.0 {
                    let a = absorb[self.owner[p] as usize];
                    let v = &mut self.presence[p][i];
                    *v = (*v + k * mult[p] * a * pr * (1.0 - *v)).min(1.0);
                }
            }
        }
    }

    /// Share of nation n's provinces that have embraced institution i.
    pub(super) fn embraced_share(&self, n: u32, i: usize) -> f64 {
        let m = &self.members[n as usize];
        if m.is_empty() {
            return 0.0;
        }
        m.iter().filter(|&&p| self.presence[p][i] >= EMBRACED).count() as f64 / m.len() as f64
    }

    /// Embrace every born institution up to era e in all of nation n's provinces.
    pub(super) fn embrace_all(&mut self, n: u32, upto: usize) {
        let members: Vec<usize> = self.members[n as usize].iter().copied().collect();
        for i in 0..upto.min(INSTITUTIONS.len()) {
            if self.born[i].is_none() {
                continue;
            }
            for &p in &members {
                self.presence[p][i] = 1.0;
            }
        }
    }

    /// Nations far behind a neighbour may reform on its model (westernization).
    pub(super) fn reforms(&mut self, dt: f64) {
        for m in 1..=self.nations.len() as u32 {
            if !self.alive(m) || self.t < self.nation(m).reform_until || self.has_nation_tag(m, "isolationist") {
                continue;
            }
            let e = self.nation(m).era;
            let model = self.members[m as usize]
                .iter()
                .flat_map(|&p| self.adj[p].iter().map(|x| self.owner[x.to as usize]))
                .filter(|&o| o > 0 && o != m && self.alive(o))
                .max_by(|&a, &b| self.nation(a).era.cmp(&self.nation(b).era).then(b.cmp(&a)));
            let Some(model) = model else { continue };
            if self.nation(model).era < e + 2 || self.nation(m).treasury < self.nation(m).income {
                continue;
            }
            if self.rng.f64() < dt / 60.0 {
                self.start_reform(m, model, REFORM_YEARS);
            }
        }
    }

    pub(super) fn start_reform(&mut self, m: u32, model: u32, years: f64) {
        let k = m as usize - 1;
        let cost = self.nations[k].income.max(0.0);
        self.nations[k].treasury -= cost;
        self.nations[k].reform_until = self.t + years;
        self.nations[k].reform_model = model;
        let a = self.nations[k].name.clone();
        let b = if model > 0 { format!(" on the model of {}", self.nation(model).name) } else { String::new() };
        self.event("reform", m, model, None, format!("{a} begins reforms{b}"));
    }

    pub(super) fn institution_rows(&self) -> Vec<Value> {
        (0..INSTITUTIONS.len())
            .map(|i| {
                let embraced = (0..self.provs.len()).filter(|&p| self.provs[p].land && self.presence[p][i] >= EMBRACED).count();
                let (year, prov) = match self.born[i] {
                    Some((y, p)) => (json!(y), json!(self.provs[p].id)),
                    None => (Value::Null, Value::Null),
                };
                let nations = (1..=self.nations.len() as u32).filter(|&m| self.alive(m) && self.nation(m).era as usize > i).count();
                json!({ "index": i, "name": INSTITUTIONS[i], "era": ERA_NAMES[i + 1], "earliest": self.era_year(i as u8 + 1),
                        "born": year, "province": prov, "province_name": self.born[i].map(|(_, p)| self.provs[p].name.clone()),
                        "embraced_provinces": embraced, "nations_in_era": nations })
            })
            .collect()
    }
}
