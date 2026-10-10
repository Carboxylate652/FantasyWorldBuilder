//! Ports (Stage 4): harbour quality of each coastal province, ports that
//! open there once its owner sails the oceans, and the sea lanes between them.
//!
//! A good harbour has calm winds, deep water close to the shore, shelter (a
//! bay rather than an open cape), a river mouth leading inland, and no winter
//! ice. A province with a good enough harbour and enough people around it
//! (its hinterland) becomes a port with a chance each year once its owner is
//! in the ocean-shipping era. Sea lanes, colonies and the spread of
//! institutions across the sea run through ports.

use super::*;

/// Building a port (times the builder's productivity) and keeping it per year.
pub const PORT_BUILD: f64 = 20_000.0;
pub const PORT_UPKEEP: f64 = 400.0;
/// How many of the nearest ports each port trades with (institutions cross the sea along these lanes).
const PORT_NEIGHBOURS: usize = 8;

fn upstream_f32<'a>(ctx: &Ctx<'a>, name: &str) -> Option<&'a [f32]> {
    ctx.input.steps.iter().rev().find_map(|s| match s.fields.get(name) {
        Some(Field::F32(v)) => Some(v.as_slice()),
        _ => None,
    })
}

fn upstream_u8<'a>(ctx: &Ctx<'a>, name: &str) -> Option<&'a [u8]> {
    ctx.input.steps.iter().rev().find_map(|s| match s.fields.get(name) {
        Some(Field::U8(v)) => Some(v.as_slice()),
        _ => None,
    })
}

/// Harbour quality (0–1) of every province: 0 for provinces with no sea coast.
pub(super) fn harbour_quality(ctx: &Ctx, provs: &[Prov], index: &HashMap<u32, usize>) -> Vec<f64> {
    let n = provs.len();
    let mut q = vec![0.0; n];
    let pf = ctx.input.u32("province");
    let (Some(kind), Some(elev)) = (upstream_u8(ctx, "province_kind"), upstream_f32(ctx, "elevation")) else { return q };
    let cells = pf.len();
    if kind.len() != cells || elev.len() != cells {
        return q;
    }
    let river = upstream_u8(ctx, "river").filter(|r| r.len() == cells);
    let cold = upstream_f32(ctx, "t_cold").filter(|r| r.len() == cells);
    let (wu, wv) = (upstream_f32(ctx, "wind_u").filter(|w| w.len() == 12 * cells), upstream_f32(ctx, "wind_v").filter(|w| w.len() == 12 * cells));
    #[derive(Default, Clone)]
    struct Acc {
        coast: u32,
        sea_links: u32,
        links: u32,
        depth: f64,
        depth_n: u32,
        wind: f64,
        mouth: bool,
        cold: f64,
    }
    let mut acc = vec![Acc { cold: f64::INFINITY, ..Default::default() }; n];
    for i in 0..cells {
        let Some(&p) = index.get(&pf[i]) else { continue };
        if !provs[p].land {
            continue;
        }
        let nb = ctx.grid.neighbors(i);
        let mut sea = 0;
        for &j in nb {
            let j = j as usize;
            if kind[j] == 3 {
                sea += 1;
                acc[p].depth += (-(elev[j] as f64)).max(0.0);
                acc[p].depth_n += 1;
            }
        }
        if sea == 0 {
            continue;
        }
        let a = &mut acc[p];
        a.coast += 1;
        a.sea_links += sea;
        a.links += nb.len() as u32;
        if let (Some(u), Some(v)) = (wu, wv) {
            a.wind += (0..12).map(|m| ((u[m * cells + i] as f64).powi(2) + (v[m * cells + i] as f64).powi(2)).sqrt()).sum::<f64>() / 12.0;
        }
        if river.is_some_and(|r| r[i] > 0) {
            a.mouth = true;
        }
        if let Some(c) = cold {
            a.cold = a.cold.min(c[i] as f64);
        }
    }
    for p in 0..n {
        let a = &acc[p];
        if a.coast == 0 {
            continue;
        }
        let depth = a.depth / a.depth_n.max(1) as f64;
        let draft = ((depth - 20.0) / 150.0).clamp(0.0, 1.0);
        let wind = if wu.is_some() { (1.0 - (a.wind / a.coast as f64 - 2.0) / 5.0).clamp(0.0, 1.0) } else { 0.5 };
        let shelter = (1.0 - (a.sea_links as f64 / a.links.max(1) as f64 - 0.1) / 0.3).clamp(0.0, 1.0);
        let ice = if a.cold < -8.0 {
            0.3
        } else if a.cold < -2.0 {
            0.7
        } else {
            1.0
        };
        q[p] = ((0.35 * wind + 0.3 * draft + 0.35 * shelter) * ice + if a.mouth { 0.2 } else { 0.0 }).clamp(0.0, 1.0);
    }
    q
}

impl NationSim {
    /// Harbour quality a nation needs for a port (merchant republics settle for less).
    fn port_threshold(&self, n: u32) -> f64 {
        self.np.port_quality - if self.has_nation_tag(n, "merchant_republic") { 0.15 } else { 0.0 }
    }

    /// People a province and half its land neighbours hold: the trade a port would serve.
    fn hinterland(&self, p: usize) -> f64 {
        self.pop[p] + 0.5 * self.adj[p].iter().filter(|e| self.provs[e.to as usize].land).map(|e| self.pop[e.to as usize]).sum::<f64>()
    }

    /// Ports open, with a chance each year, in coastal provinces of nations
    /// in the ocean-shipping era that have a good enough harbour and hinterland.
    pub(super) fn port_growth(&mut self, dt: f64) {
        for &p in &self.coastal_land.clone() {
            let o = self.owner[p];
            if o == 0 || self.ports.contains_key(&p) || self.nation(o).era < era::SHIP || self.harbour[p] < self.port_threshold(o) {
                continue;
            }
            if self.hinterland(p) < self.np.port_people {
                continue;
            }
            let chance = (dt / 15.0 * (0.5 + self.harbour[p])).min(1.0);
            if self.rng.f64() < chance {
                self.open_port(o, p);
            }
        }
    }

    pub(super) fn open_port(&mut self, n: u32, p: usize) {
        let first = self.ports.is_empty();
        let own_first = !self.ports.keys().any(|&q| self.owner[q] == n);
        self.ports.insert(p, self.year);
        self.port_near = None;
        self.stale[n as usize] = true;
        if first || (own_first && self.ports.len() <= 12) {
            let (a, b) = (self.nation(n).name.clone(), self.provs[p].name.clone());
            self.event("port", n, 0, Some(p), format!("{b} becomes {}port of {a}", if first { "the world's first ocean " } else { "the first ocean " }));
        }
    }

    /// The nearest ports of every port (sea lanes for institutions), cached until ports change.
    pub(super) fn port_lanes(&mut self) -> &Vec<(usize, Vec<usize>)> {
        if self.port_near.is_none() {
            let ports: Vec<usize> = self.ports.keys().copied().collect();
            let range = self.np.overseas_km.max(500.0);
            let lanes = ports
                .iter()
                .map(|&a| {
                    let mut near: Vec<(f64, usize)> = ports.iter().filter(|&&b| b != a).map(|&b| (self.km(a, b), b)).filter(|x| x.0 <= range).collect();
                    near.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
                    (a, near.into_iter().take(PORT_NEIGHBOURS).map(|x| x.1).collect())
                })
                .collect();
            self.port_near = Some(lanes);
        }
        self.port_near.as_ref().unwrap()
    }

    /// Sea lanes of nation n for its access: each of its ports to its main
    /// port (the capital if it is a port, else its largest port), cheaper
    /// between good harbours.
    pub(super) fn sea_links(&self, n: u32) -> Vec<(usize, usize, f64)> {
        let mine: Vec<usize> = self.ports.keys().copied().filter(|&p| self.owner[p] == n).collect();
        let cap = self.nation(n).capital;
        let main = if self.ports.contains_key(&cap) { Some(cap) } else { mine.iter().copied().max_by(|&a, &b| self.pop[a].total_cmp(&self.pop[b]).then(b.cmp(&a))) };
        let Some(m) = main else { return vec![] };
        mine.iter()
            .filter(|&&p| p != m)
            .map(|&p| (m, p, self.km(m, p) * SEA_FACTOR * (1.25 - 0.5 * (self.harbour[m] + self.harbour[p]) / 2.0) + SEA_PENALTY))
            .collect()
    }

    pub(super) fn port_rows(&self) -> Vec<Value> {
        self.ports
            .iter()
            .map(|(&p, &y)| json!({ "province": self.provs[p].id, "name": self.provs[p].name, "owner": self.owner[p], "opened": y, "harbour": (self.harbour[p] * 100.0).round() / 100.0 }))
            .collect()
    }
}
