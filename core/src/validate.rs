//! Consistency checks on a generated world, for automated tests and for
//! `worldgen validate`: the political tables agree with each other and with
//! the grid fields, trade goods are known names, cultures point at living
//! cultures, and every override edit still applies.
//!
//! Errors mean the world is broken (a bug, or a hand-edited project file);
//! warnings mean something is out of date or worth a look.

use crate::fields::Field;
use crate::stages::resources::{DEPOSITS, TRADE_GOODS};
use crate::stages::{Step, STEPS};
use crate::world::World;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Default, Debug)]
pub struct Report {
    pub ok: bool,
    /// Names of the checks that ran (a check is skipped when its step has not run).
    pub checks: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Report {
    fn check(&mut self, name: &str) {
        self.checks.push(name.to_string());
    }
    /// Record an error, capping repeats of one kind so the report stays short.
    fn err(&mut self, kind: &str, msg: String, seen: &mut HashMap<String, usize>) {
        let n = seen.entry(kind.to_string()).or_insert(0);
        *n += 1;
        if *n <= 5 {
            self.errors.push(msg);
        } else if *n == 6 {
            self.errors.push(format!("{kind}: more errors of this kind not listed"));
        }
    }
}

pub fn validate(w: &mut World) -> Report {
    let mut r = Report::default();
    let mut seen = HashMap::new();

    for st in STEPS {
        if w.steps[st.index()].is_some() && !w.is_fresh(st) {
            r.warnings.push(format!("step {} is stale: run it again to bring it up to date", st.key()));
        }
    }

    let table = w.meta(Step::Provinces).map(|m| m["table"].clone()).filter(|t| !t.is_null());
    if let Some(t) = &table {
        political(w, t, &mut r, &mut seen);
    }
    if let (Some(t), Some(m)) = (&table, w.meta(Step::Cultures).cloned()) {
        if m["cultures"].as_u64().unwrap_or(0) > 0 {
            cultures(t, &m["table"], &mut r, &mut seen);
        }
    }

    let rep = crate::api::overrides_report(w);
    r.check("override edits apply");
    for u in rep["unapplied"].as_array().into_iter().flatten() {
        r.warnings.push(format!(
            "override edit #{} in layer {} ({}) has no effect: {}",
            u["index"], u["layer"].as_str().unwrap_or("?"), u["tool"].as_str().unwrap_or("?"), u["reason"].as_str().unwrap_or("")
        ));
    }

    r.ok = r.errors.is_empty();
    r
}

fn ids(v: &Value) -> Vec<u64> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_u64()).collect()).unwrap_or_default()
}

fn political(w: &mut World, t: &Value, r: &mut Report, seen: &mut HashMap<String, usize>) {
    let empty = vec![];
    let provs = t["provinces"].as_array().unwrap_or(&empty);
    let states = t["states"].as_array().unwrap_or(&empty);

    r.check("province ids and colours unique");
    let mut pid: HashMap<u64, &Value> = HashMap::new();
    let mut colors = HashSet::new();
    for p in provs {
        let id = p["id"].as_u64().unwrap_or(0);
        if id == 0 || pid.insert(id, p).is_some() {
            r.err("province id", format!("province id {id} is zero or used twice"), seen);
        }
        if !colors.insert(p["color"].to_string()) {
            r.err("province colour", format!("province {id}: colour {} is used twice", p["color"]), seen);
        }
    }

    r.check("provinces belong to states");
    let sid: HashMap<u64, &Value> = states.iter().map(|s| (s["id"].as_u64().unwrap_or(0), s)).collect();
    for p in provs {
        let kind = p["kind"].as_str().unwrap_or("");
        let st = p["state"].as_u64().unwrap_or(0);
        if matches!(kind, "land" | "wasteland") && !sid.contains_key(&st) {
            r.err("province state", format!("{kind} province {} is in state {st}, which does not exist", p["id"]), seen);
        }
    }
    for s in states {
        let id = s["id"].as_u64().unwrap_or(0);
        let members = ids(&s["provinces"]);
        for m in &members {
            match pid.get(m) {
                Some(p) if p["state"].as_u64() == Some(id) => {}
                _ => r.err("state members", format!("state {id} lists province {m}, which is not in it"), seen),
            }
        }
        let cap = s["capital_province"].as_u64().unwrap_or(0);
        if !members.is_empty() && !members.contains(&cap) {
            r.err("state capital", format!("state {id}: capital province {cap} is not one of its provinces"), seen);
        }
    }

    r.check("adjacency table");
    let mut pairs = HashSet::new();
    for a in t["adjacency"].as_array().unwrap_or(&empty) {
        let (f, to) = (a["from"].as_u64().unwrap_or(0), a["to"].as_u64().unwrap_or(0));
        if f == to || !pid.contains_key(&f) || !pid.contains_key(&to) {
            r.err("adjacency", format!("adjacency {f}–{to} refers to itself or to a missing province"), seen);
        }
        if !pairs.insert((f.min(to), f.max(to))) {
            r.err("adjacency duplicate", format!("adjacency {f}–{to} is listed twice"), seen);
        }
    }

    r.check("province field matches the table");
    if let Some((Field::U32(field), _, _)) = w.field("province") {
        let mut cells: HashMap<u64, usize> = HashMap::new();
        for &v in field.iter() {
            if v != 0 {
                *cells.entry(v as u64).or_insert(0) += 1;
            }
        }
        for id in cells.keys() {
            if !pid.contains_key(id) {
                r.err("province field", format!("grid cells carry province {id}, which is not in the table"), seen);
            }
        }
        for id in pid.keys() {
            if !cells.contains_key(id) {
                r.err("empty province", format!("province {id} has no grid cells"), seen);
            }
        }
    }

    r.check("trade goods and deposits are known");
    for p in provs {
        let kind = p["kind"].as_str().unwrap_or("");
        if let Some(g) = p["trade_good"].as_str() {
            if !TRADE_GOODS.contains(&g) {
                r.err("trade good", format!("province {}: unknown trade good `{g}`", p["id"]), seen);
            }
        } else if kind == "land" {
            r.err("trade good", format!("land province {} has no trade good", p["id"]), seen);
        }
        for d in p["resources"].as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
            let known = if kind == "sea" { d == "fish" } else { DEPOSITS.contains(&d) };
            if !known {
                r.err("deposit", format!("province {}: unknown resource `{d}`", p["id"]), seen);
            }
        }
    }
}

fn cultures(t: &Value, c: &Value, r: &mut Report, seen: &mut HashMap<String, usize>) {
    r.check("cultures and groups consistent");
    let empty = vec![];
    let alive: HashMap<u64, u64> = c["cultures"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter(|x| x["alive"].as_bool() == Some(true))
        .map(|x| (x["id"].as_u64().unwrap_or(0), x["group"].as_u64().unwrap_or(0)))
        .collect();
    let groups: HashSet<u64> = c["groups"].as_array().unwrap_or(&empty).iter().filter_map(|g| g["id"].as_u64()).collect();
    for (id, g) in &alive {
        if !groups.contains(g) {
            r.err("culture group", format!("culture {id} is in group {g}, which does not exist"), seen);
        }
    }
    let known: HashSet<u64> = t["provinces"].as_array().unwrap_or(&empty).iter().filter_map(|p| p["id"].as_u64()).collect();
    for p in c["provinces"].as_array().unwrap_or(&empty) {
        let id = p["id"].as_u64().unwrap_or(0);
        if !known.contains(&id) {
            r.err("culture province", format!("culture table has province {id}, which the provinces step does not"), seen);
        }
        let cu = p["culture"].as_u64().unwrap_or(0);
        if cu != 0 && !alive.contains_key(&cu) {
            r.err("province culture", format!("province {id}: majority culture {cu} is not a living culture"), seen);
        }
    }
}
