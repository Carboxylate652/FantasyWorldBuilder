//! Tags and feudal empires (Stage 4).
//!
//! **Tags** are set by directives on the world, a state or a nation, and
//! change the rules for it (see the `tag` directive for the list).
//!
//! **Feudal empires** (the `feudal_empire` directive): the emperor's nation
//! keeps its capital region as its own domain and enfeoffs a king over each
//! other region of the imperial territory; each king keeps his capital state
//! and enfeoffs a duke over each other state (so does the emperor in his own
//! region). Provinces are the counties, and each county has a few baronies
//! (titles only). Kings and dukes are nations of their own, with a liege:
//! - members never fight each other, never break apart, and defend each
//!   other (a member's defence counts the whole realm's people);
//! - vassals pay a tenth of their income to their liege, and institutions
//!   spread to them from their liege's capital;
//! - the empire lasts in every era — a fantasy world needs no reason the real
//!   one would accept — until a directive dissolves it; if the emperor's own
//!   nation falls, the largest member is elected emperor;
//! - the emperor's land outside the given places is held outside the empire
//!   (like the Habsburg crown lands outside the Holy Roman Empire).

use super::*;

/// Feudal ranks: 0 none, 1 emperor, 2 king, 3 duke.
pub const RANKS: [&str; 4] = ["", "emperor", "king", "duke"];
/// Share of a vassal's income paid to its liege.
const TRIBUTE: f64 = 0.1;
const BARONY_SUFFIX: [&str; 8] = ["ford", "wick", "holm", "stead", "mere", "thorpe", "by", "hall"];

pub struct Empire {
    pub name: String,
    pub emperor: u32,
    /// The de jure imperial territory.
    pub territory: BTreeSet<usize>,
    pub founded: i32,
    pub dissolved: Option<i32>,
}

const SCOPES: [&str; 3] = ["world", "state", "nation"];

/// Tag scopes.
pub mod scope {
    pub const WORLD: u8 = 0;
    pub const STATE: u8 = 1;
    pub const NATION: u8 = 2;
}

impl NationSim {
    pub(super) fn has_nation_tag(&self, n: u32, tag: &str) -> bool {
        n > 0 && self.tags.iter().any(|(s, id, t)| *s == scope::NATION && *id == n && t == tag)
    }

    pub(super) fn has_state_tag(&self, state: u32, tag: &str) -> bool {
        state > 0 && self.tags.iter().any(|(s, id, t)| *s == scope::STATE && *id == state && t == tag)
    }

    pub(super) fn world_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|(s, _, t)| *s == scope::WORLD && t == tag)
    }

    pub(super) fn set_tag(&mut self, sc: &str, id: u32, tag: &str, on: bool) -> bool {
        let s = match sc {
            "world" => scope::WORLD,
            "state" => scope::STATE,
            "nation" => scope::NATION,
            _ => return false,
        };
        let id = if s == scope::WORLD { 0 } else { id };
        if s == scope::NATION && !self.alive(id) {
            return false;
        }
        let key = (s, id, tag.to_string());
        if on {
            self.tags.insert(key)
        } else {
            self.tags.remove(&key)
        }
    }

    pub(super) fn tag_rows(&self) -> Vec<Value> {
        self.tags.iter().map(|(s, id, t)| json!({ "scope": SCOPES[*s as usize], "id": id, "tag": t })).collect()
    }

    /// The top of nation n's feudal chain (itself if it has no liege).
    pub(super) fn realm_of(&self, n: u32) -> u32 {
        let mut m = n;
        for _ in 0..8 {
            let l = self.nation(m).liege;
            if l == 0 {
                break;
            }
            m = l;
        }
        m
    }

    /// The active empire nation n belongs to.
    pub(super) fn empire_of(&self, n: u32) -> Option<usize> {
        if n == 0 {
            return None;
        }
        let top = self.realm_of(n);
        self.empires.iter().position(|e| e.dissolved.is_none() && e.emperor == top)
    }

    /// Whether two nations are members of the same feudal empire.
    pub(super) fn same_realm(&self, a: u32, b: u32) -> bool {
        a > 0 && b > 0 && self.empire_of(a).is_some() && self.empire_of(a) == self.empire_of(b)
    }

    /// Living members of empire k (the emperor and all vassals below him).
    pub(super) fn empire_members(&self, k: usize) -> Vec<u32> {
        (1..=self.nations.len() as u32).filter(|&m| self.alive(m) && self.empire_of(m) == Some(k)).collect()
    }

    /// Create a vassal nation over `land` (taken from its current owner).
    fn enfeoff(&mut self, liege: u32, land: &[usize], rank: u8, name: String) -> Option<u32> {
        let capital = *land.iter().max_by(|&&a, &&b| self.pop[a].total_cmp(&self.pop[b]).then(b.cmp(&a)))?;
        let id = self.nations.len() as u32 + 1;
        let lg = self.nation(liege).clone();
        let primary = if self.major[capital] > 0 { self.major[capital] } else { lg.primary };
        let aggression = 0.6 + 0.8 * self.rng.f64();
        self.nations.push(Nation {
            name,
            color: nation_color(id),
            capital,
            primary,
            founded: self.year,
            ended: None,
            fate: String::new(),
            other: 0,
            parent: liege,
            aggression,
            capital_since: self.t,
            tech: lg.tech,
            era: lg.era,
            treasury: 0.0,
            income: 0.0,
            upkeep: 0.0,
            infra: 0.0,
            debt_until: i64::MIN,
            liege,
            rank,
            next_road: self.t + (id % 7) as i64 * MONTHS,
            next_rail: self.t + (id % 5) as i64 * MONTHS,
            reform_until: i64::MIN,
            reform_model: 0,
        });
        self.members.push(BTreeSet::new());
        self.access.push(HashMap::new());
        self.stale.push(true);
        for &p in land {
            self.take(p, id);
        }
        Some(id)
    }

    /// Make nation n a feudal empire over `places` (its own land among them,
    /// or all its land): kingdoms over regions, duchies over states.
    pub(super) fn create_empire(&mut self, n: u32, places: Vec<usize>, name: Option<String>) -> bool {
        let mine: Vec<usize> = if places.is_empty() { self.members[n as usize].iter().copied().collect() } else { places.into_iter().filter(|&p| self.owner[p] == n).collect() };
        if mine.is_empty() {
            return false;
        }
        if let Some(k) = self.empires.iter().position(|e| e.dissolved.is_none() && e.emperor == n) {
            // Already an empire: the places join its territory.
            self.empires[k].territory.extend(mine);
            return true;
        }
        let cap = self.nation(n).capital;
        let (cap_region, cap_state) = (self.provs[cap].region, self.provs[cap].state);
        let mut regions: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
        for &p in &mine {
            regions.entry(self.provs[p].region).or_default().push(p);
        }
        let mut kings = 0;
        let mut dukes = 0;
        for (r, land) in regions {
            if r == cap_region || land.len() < 2 {
                // The emperor's own region: its other states become duchies under him.
                dukes += self.enfeoff_duchies(n, &land, cap_state);
                continue;
            }
            let rn = self.region_names.get(&r).cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| self.provs[land[0]].name.clone());
            if let Some(k) = self.enfeoff(n, &land, 2, format!("Kingdom of {rn}")) {
                kings += 1;
                let ks = self.provs[self.nation(k).capital].state;
                dukes += self.enfeoff_duchies(k, &land, ks);
            }
        }
        self.nations[n as usize - 1].rank = 1;
        let base = self.nation(n).name.clone();
        let name = name.filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()).unwrap_or_else(|| format!("Empire of {base}"));
        let territory: BTreeSet<usize> = mine.into_iter().collect();
        self.event("empire", n, 0, Some(cap), format!("{base} becomes the {name}: {kings} kingdoms and {dukes} duchies hold their land of the emperor"));
        self.empires.push(Empire { name, emperor: n, territory, founded: self.year, dissolved: None });
        true
    }

    /// Duchies under `liege` over each state of `land` but its own capital state.
    fn enfeoff_duchies(&mut self, liege: u32, land: &[usize], keep_state: u16) -> u32 {
        let mut states: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
        for &p in land {
            if self.owner[p] == liege && self.provs[p].state != keep_state {
                states.entry(self.provs[p].state).or_default().push(p);
            }
        }
        let mut k = 0;
        for (s, list) in states {
            let sn = self.state_names.get(&s).cloned().filter(|x| !x.is_empty()).unwrap_or_else(|| self.provs[list[0]].name.clone());
            if self.enfeoff(liege, &list, 3, format!("Duchy of {sn}")).is_some() {
                k += 1;
            }
        }
        k
    }

    /// Dissolve empire k: every member becomes independent.
    pub(super) fn dissolve_empire(&mut self, k: usize) {
        let members = self.empire_members(k);
        for m in &members {
            let x = &mut self.nations[*m as usize - 1];
            x.liege = 0;
            x.rank = 0;
        }
        self.empires[k].dissolved = Some(self.year);
        let (name, e) = (self.empires[k].name.clone(), self.empires[k].emperor);
        self.event("empire_dissolved", e, 0, None, format!("The {name} is dissolved: its {} members go their own ways", members.len()));
    }

    /// If an emperor's nation has fallen, the largest member is elected emperor.
    pub(super) fn imperial_succession(&mut self) {
        for k in 0..self.empires.len() {
            if self.empires[k].dissolved.is_some() || self.alive(self.empires[k].emperor) {
                continue;
            }
            let old = self.empires[k].emperor;
            // Members: living nations whose chain of lieges reaches the fallen emperor.
            let members: Vec<u32> = (1..=self.nations.len() as u32).filter(|&m| self.alive(m) && self.realm_of(m) == old).collect();
            let Some(&heir) = members.iter().max_by(|&&a, &&b| {
                let pa: f64 = self.members[a as usize].iter().map(|&p| self.pop[p]).sum();
                let pb: f64 = self.members[b as usize].iter().map(|&p| self.pop[p]).sum();
                pa.total_cmp(&pb).then(b.cmp(&a))
            }) else {
                self.empires[k].dissolved = Some(self.year);
                continue;
            };
            for m in &members {
                if self.nation(*m).liege == old {
                    self.nations[*m as usize - 1].liege = heir;
                }
            }
            let x = &mut self.nations[heir as usize - 1];
            x.liege = 0;
            x.rank = 1;
            self.empires[k].emperor = heir;
            let (a, name) = (self.nation(heir).name.clone(), self.empires[k].name.clone());
            self.event("emperor", heir, old, None, format!("{a} is elected emperor of the {name}"));
        }
    }

    /// Tribute: vassals pay a share of their income to their liege.
    pub(super) fn tribute(&mut self, dt: f64) {
        for m in 1..=self.nations.len() {
            let l = self.nations[m - 1].liege;
            if self.nations[m - 1].ended.is_some() || l == 0 || !self.alive(l) {
                continue;
            }
            let t = TRIBUTE * self.nations[m - 1].income.max(0.0) * dt;
            self.nations[m - 1].treasury -= t;
            self.nations[l as usize - 1].treasury += t;
        }
    }

    pub(super) fn empire_rows(&self) -> Vec<Value> {
        self.empires
            .iter()
            .enumerate()
            .map(|(k, e)| {
                let members = if e.dissolved.is_none() { self.empire_members(k) } else { vec![] };
                json!({ "id": k + 1, "name": e.name, "emperor": e.emperor, "founded": e.founded, "dissolved": e.dissolved,
                        "territory_provinces": e.territory.len(), "members": members.len(),
                        "kings": members.iter().filter(|&&m| self.nation(m).rank == 2).count(),
                        "dukes": members.iter().filter(|&&m| self.nation(m).rank == 3).count() })
            })
            .collect()
    }

    /// Every feudal title of the active empires: empire, kingdoms, duchies,
    /// counties (provinces of the imperial territory) and baronies.
    pub(super) fn title_rows(&self) -> Vec<Value> {
        let mut out = Vec::new();
        for (k, e) in self.empires.iter().enumerate() {
            if e.dissolved.is_some() {
                continue;
            }
            out.push(json!({ "level": "empire", "title": e.name, "holder": e.emperor, "liege": 0, "province": Value::Null, "empire": k + 1 }));
            for m in self.empire_members(k) {
                let x = self.nation(m);
                if x.rank >= 2 {
                    out.push(json!({ "level": RANKS[x.rank as usize], "title": x.name, "holder": m, "liege": x.liege, "province": Value::Null, "empire": k + 1 }));
                }
            }
            for &p in &e.territory {
                let holder = self.owner[p];
                let name = &self.provs[p].name;
                out.push(json!({ "level": "count", "title": format!("County of {name}"), "holder": holder, "liege": holder, "province": self.provs[p].id, "empire": k + 1 }));
                let nb = 2 + (self.provs[p].id % 3) as usize;
                for b in 0..nb {
                    let suffix = BARONY_SUFFIX[(self.provs[p].id as usize * 7 + b * 3) % BARONY_SUFFIX.len()];
                    let stem: String = name.chars().take(5).collect();
                    out.push(json!({ "level": "baron", "title": format!("Barony of {stem}{suffix}"), "holder": holder, "liege": holder, "province": self.provs[p].id, "empire": k + 1 }));
                }
            }
        }
        out
    }
}
