//! Command interface shared by the Tauri app and the headless HTTP server.
//! Every command takes JSON arguments and returns either JSON or raw bytes.

use crate::edits::{EditLayer, Edits, Imports, MotionArrow, PlatePin, Sketch, Stroke};
use crate::export::{export, ExportOptions};
use crate::fields::Field;
use crate::params::WorldParams;
use crate::stages::cultures::CultureSim;
use crate::stages::nations::NationSim;
use crate::stages::{climate, Step, STEPS};
use crate::world::World;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub enum Reply {
    Json(Value),
    Bytes(Vec<u8>),
}

#[derive(Default, Clone, serde::Serialize)]
pub struct ProgressState {
    pub running: bool,
    pub task: String,
    pub step: String,
    pub frac: f32,
    pub msg: String,
}

pub struct Session {
    pub world: World,
    pub path: Option<PathBuf>,
    undo: VecDeque<EditHistory>,
    redo: VecDeque<EditHistory>,
    pub dirty: bool,
    /// A Stage 3 or 4 simulation running step by step.
    pub live: Option<Live>,
}

/// Undo stores only the part an action can change instead of cloning all edit
/// layers for every brush stroke. Applying an entry yields its redo inverse.
enum EditHistory {
    Layers(Vec<(EditLayer, LayerState)>),
    Pins(Vec<PlatePin>),
    Arrows(Vec<MotionArrow>),
    AutoBase(bool),
    Imports(Imports),
}

enum LayerState {
    Sketch(Sketch),
    Strokes(Vec<Stroke>),
    Directives(Vec<crate::directives::Directive>),
}

impl LayerState {
    fn capture(edits: &Edits, layer: EditLayer) -> LayerState {
        match layer {
            EditLayer::Sketch => LayerState::Sketch(edits.sketch.clone()),
            EditLayer::Directives => LayerState::Directives(edits.overrides.directives.clone()),
            _ => LayerState::Strokes(edits.strokes(layer).clone()),
        }
    }
    fn restore(self, edits: &mut Edits, layer: EditLayer) {
        match (layer, self) {
            (EditLayer::Sketch, LayerState::Sketch(v)) => edits.sketch = v,
            (EditLayer::Directives, LayerState::Directives(v)) => edits.overrides.directives = v,
            (EditLayer::Plates, LayerState::Strokes(v)) => edits.overrides.plates = v,
            (EditLayer::Elevation, LayerState::Strokes(v)) => edits.overrides.elevation = v,
            (EditLayer::Biomes, LayerState::Strokes(v)) => edits.overrides.biomes = v,
            (EditLayer::Barriers, LayerState::Strokes(v)) => edits.overrides.barriers = v,
            (EditLayer::States, LayerState::Strokes(v)) => edits.overrides.states = v,
            (EditLayer::Provinces, LayerState::Strokes(v)) => edits.overrides.provinces = v,
            (EditLayer::Sites, LayerState::Strokes(v)) => edits.overrides.sites = v,
            (EditLayer::Fertility, LayerState::Strokes(v)) => edits.overrides.fertility = v,
            (EditLayer::Bands, LayerState::Strokes(v)) => edits.overrides.bands = v,
            (EditLayer::Attraction, LayerState::Strokes(v)) => edits.overrides.attraction = v,
            _ => unreachable!("history state does not match edit layer"),
        }
    }
}

impl EditHistory {
    fn swap(self, edits: &mut Edits) -> EditHistory {
        match self {
            EditHistory::Layers(saved) => {
                let mut inverse = Vec::with_capacity(saved.len());
                for (layer, state) in saved {
                    inverse.push((layer, LayerState::capture(edits, layer)));
                    state.restore(edits, layer);
                }
                EditHistory::Layers(inverse)
            }
            EditHistory::Pins(mut saved) => { std::mem::swap(&mut saved, &mut edits.sketch.pins); EditHistory::Pins(saved) }
            EditHistory::Arrows(mut saved) => { std::mem::swap(&mut saved, &mut edits.sketch.arrows); EditHistory::Arrows(saved) }
            EditHistory::AutoBase(saved) => EditHistory::AutoBase(std::mem::replace(&mut edits.sketch.auto_base, saved)),
            EditHistory::Imports(mut saved) => { std::mem::swap(&mut saved, &mut edits.imports); EditHistory::Imports(saved) }
        }
    }
}

pub enum LiveSim {
    Cultures(Box<CultureSim>),
    Nations(Box<NationSim>),
}

/// A live simulation and what it was started from, so a commit can tell
/// whether its result is still the one a fresh run would give.
pub struct Live {
    pub sim: LiveSim,
    step: Step,
    upstream_hash: u64,
    inputs: Value,
    millis: u64,
    cache: Option<(Value, crate::fields::Fields)>,
}

impl Live {
    pub fn stage(&self) -> &'static str {
        self.step.key()
    }

    pub fn done(&self) -> bool {
        match &self.sim {
            LiveSim::Cultures(c) => c.done(),
            LiveSim::Nations(n) => n.done(),
        }
    }

    /// Where the run stands: the next generation (Stage 3) or year (Stage 4).
    pub fn position(&self) -> f64 {
        match &self.sim {
            LiveSim::Cultures(c) => c.t as f64,
            LiveSim::Nations(n) => n.time(),
        }
    }

    fn step_once(&mut self, progress: &dyn Fn(f32, &str)) {
        let t0 = std::time::Instant::now();
        match &mut self.sim {
            LiveSim::Cultures(c) => c.tick(progress),
            LiveSim::Nations(n) => n.step(progress),
        }
        self.millis += t0.elapsed().as_millis() as u64;
        self.cache = None;
    }

    /// Era label at the current position (stepping "to the next era" stops when it changes).
    fn era(&self) -> String {
        match &self.sim {
            LiveSim::Cultures(c) => format!("{}", c.era_of(c.t.min(c.ticks().saturating_sub(1)))),
            LiveSim::Nations(n) => n.era().to_string(),
        }
    }

    pub fn snapshot(&mut self) -> &(Value, crate::fields::Fields) {
        if self.cache.is_none() {
            let (mut v, f) = match &self.sim {
                LiveSim::Cultures(c) => c.snapshot(),
                LiveSim::Nations(n) => n.snapshot(),
            };
            v["done"] = json!(self.done());
            v["position"] = json!(self.position());
            self.cache = Some((v, f));
        }
        self.cache.as_ref().unwrap()
    }

    fn info(&self) -> Value {
        let (pos, end, unit) = match &self.sim {
            LiveSim::Cultures(c) => (c.t as f64, c.ticks() as f64, "generation"),
            LiveSim::Nations(n) => (n.time(), n.end_year() as f64, "year"),
        };
        json!({ "stage": self.stage(), "position": pos, "end": end, "unit": unit, "done": self.done() })
    }
}

/// The inputs a live run depends on besides upstream steps and its directives.
fn live_inputs(w: &World, step: Step) -> Value {
    match step {
        Step::Cultures => json!([w.params.cultures, w.edits.overrides.bands, w.edits.overrides.attraction]),
        _ => json!([w.params.nations]),
    }
}

impl Default for Session {
    fn default() -> Self {
        Session::new(WorldParams::default())
    }
}

impl Session {
    pub fn new(params: WorldParams) -> Session {
        Session { world: World::new(params), path: None, undo: VecDeque::new(), redo: VecDeque::new(), dirty: false, live: None }
    }

    fn remember(&mut self, state: EditHistory) {
        self.undo.push_back(state);
        if self.undo.len() > 500 {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.dirty = true;
    }

    fn snapshot_layers(&mut self, layers: &[EditLayer]) {
        let mut unique = Vec::new();
        for &layer in layers {
            if !unique.contains(&layer) { unique.push(layer); }
        }
        let state = EditHistory::Layers(unique.into_iter().map(|l| (l, LayerState::capture(&self.world.edits, l))).collect());
        self.remember(state);
    }

    pub fn status(&mut self) -> Value {
        let grid = self.world.grid();
        let e = &self.world.edits;
        json!({
            "steps": self.world.status(),
            "params": self.world.params,
            "edits": {
                "auto_base": e.sketch.auto_base,
                "sketch_strokes": e.sketch.strokes.len(),
                "pins": e.sketch.pins,
                "arrows": e.sketch.arrows,
                "plate_strokes": e.overrides.plates.len(),
                "elevation_strokes": e.overrides.elevation.len(),
                "biome_strokes": e.overrides.biomes.len(),
                "barrier_strokes": e.overrides.barriers.len(),
                "state_strokes": e.overrides.states.len(),
                "province_strokes": e.overrides.provinces.len(),
                "site_pins": e.overrides.sites.len(),
                "fertility_strokes": e.overrides.fertility.len(),
                "attraction_strokes": e.overrides.attraction.len(),
                "band_pins": e.overrides.bands.iter().filter_map(|s| s.points.first().map(|p| json!({ "lat": p[0], "lon": p[1], "bands": s.value }))).collect::<Vec<_>>(),
                "sites": e.overrides.sites.iter().filter_map(|s| s.points.first().map(|p| json!({ "lat": p[0], "lon": p[1], "population": s.value }))).collect::<Vec<_>>(),
                "elevation_import": e.imports.elevation,
                "province_import": e.imports.provinces,
            },
            "grid": { "level": grid.level, "cells": grid.len(), "triangles": grid.tris.len(), "spacing_km": grid.spacing * self.world.params.planet.radius_km },
            "override_edits": EditLayer::ALL.iter().map(|&l| e.count(l)).sum::<usize>(),
            "path": self.path.as_ref().map(|p| p.display().to_string()),
            "can_undo": !self.undo.is_empty(),
            "can_redo": !self.redo.is_empty(),
            "dirty": self.dirty,
            "live": self.live.as_ref().map(|l| l.info()),
        })
    }
}

/// Every override layer with its edit count and the step it feeds, plus the
/// edits the latest run could not apply (their target is gone after a seed,
/// sketch or upstream change).
pub fn overrides_report(w: &World) -> Value {
    let layers: Vec<Value> = EditLayer::ALL
        .iter()
        .map(|&l| json!({ "layer": l.key(), "title": l.title(), "step": l.step().key(), "edits": w.edits.count(l) }))
        .collect();
    let mut unapplied = Vec::new();
    for st in [Step::States, Step::Provinces] {
        if let Some(m) = w.meta(st) {
            if let Some(a) = m["unapplied_edits"].as_array() {
                // Indices refer to the run's edits: drop entries the layer no longer has.
                let fresh = w.is_fresh(st);
                unapplied.extend(a.iter().filter(|_| fresh).cloned());
            }
        }
    }
    json!({ "layers": layers, "unapplied": unapplied, "total": EditLayer::ALL.iter().map(|&l| w.edits.count(l)).sum::<usize>() })
}

/// Layers named in a request, or by default every layer that has edits.
fn layer_list(w: &World, names: Option<Vec<String>>) -> Result<Vec<EditLayer>, String> {
    match names {
        Some(v) => v.iter().map(|k| EditLayer::from_key(k).ok_or(format!("unknown layer `{k}`"))).collect(),
        None => Ok(EditLayer::ALL.into_iter().filter(|&l| w.edits.count(l) > 0).collect()),
    }
}

fn live_unit_is_year(sim: &LiveSim) -> bool {
    matches!(sim, LiveSim::Nations(_))
}

/// The live simulation's summary (for the UI and the AI guide) and the session status.
fn sim_state(s: &mut Session) -> Result<Reply, String> {
    let live = s.live.as_mut().ok_or("no live simulation")?;
    let summary = live.snapshot().0.clone();
    let mut st = s.status();
    st["sim"] = summary;
    Ok(Reply::Json(st))
}

fn arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null)).map_err(|e| format!("argument `{key}`: {e}"))
}

fn set_progress(p: &Arc<Mutex<ProgressState>>, f: impl FnOnce(&mut ProgressState)) {
    if let Ok(mut g) = p.lock() {
        f(&mut g);
    }
}

/// Binary grid layout (little endian):
/// u32 magic "GRD1", u32 cells, u32 triangles, u32 neighbour entries,
/// f32×3·cells positions, u32×3·triangles, u32×(cells+1) neighbour offsets, u32 neighbours.
fn grid_bytes(world: &mut World) -> Vec<u8> {
    let g = world.grid();
    let (off, nbr) = g.neighbor_csr();
    let mut b = Vec::with_capacity(16 + g.len() * 16 + g.tris.len() * 12 + nbr.len() * 4);
    b.extend_from_slice(b"GRD1");
    b.extend_from_slice(&(g.len() as u32).to_le_bytes());
    b.extend_from_slice(&(g.tris.len() as u32).to_le_bytes());
    b.extend_from_slice(&(nbr.len() as u32).to_le_bytes());
    for p in &g.pos {
        for v in [p.x, p.y, p.z] {
            b.extend_from_slice(&(v as f32).to_le_bytes());
        }
    }
    for t in &g.tris {
        for v in t {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    for v in off {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for v in nbr {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

/// Binary field layout: u8 type (0 f32, 1 u8, 2 u16, 3 u32, 4 i32), u8 stale,
/// u16 step index, u32 count, then the values.
fn field_bytes(f: &Field, stale: bool, step: Step, month: Option<usize>, n: usize) -> Vec<u8> {
    let (code, data) = match (f, month) {
        (Field::F32(v), Some(m)) if v.len() == 12 * n => (0u8, Field::F32(v[m * n..(m + 1) * n].to_vec()).to_bytes()),
        (Field::F32(v), _) if v.len() == 12 * n => {
            // Annual mean of a monthly field.
            let mut s = vec![0f32; n];
            for m in 0..12 {
                for i in 0..n {
                    s[i] += v[m * n + i] / 12.0;
                }
            }
            (0u8, Field::F32(s).to_bytes())
        }
        (Field::F32(_), _) => (0, f.to_bytes()),
        (Field::U8(_), _) => (1, f.to_bytes()),
        (Field::U16(_), _) => (2, f.to_bytes()),
        (Field::U32(_), _) => (3, f.to_bytes()),
        (Field::I32(_), _) => (4, f.to_bytes()),
    };
    let count = (data.len() / if code == 1 { 1 } else if code == 2 { 2 } else { 4 }) as u32;
    let mut b = Vec::with_capacity(8 + data.len());
    b.push(code);
    b.push(stale as u8);
    b.extend_from_slice(&(step.index() as u16).to_le_bytes());
    b.extend_from_slice(&count.to_le_bytes());
    b.extend_from_slice(&data);
    b
}

pub fn handle(session: &Mutex<Session>, progress: &Arc<Mutex<ProgressState>>, cmd: &str, args: Value) -> Result<Reply, String> {
    if cmd == "progress" {
        let p = progress.lock().map_err(|e| e.to_string())?.clone();
        return Ok(Reply::Json(serde_json::to_value(p).unwrap()));
    }
    let mut s = session.lock().map_err(|_| "session lock poisoned".to_string())?;
    let s = &mut *s;
    match cmd {
        "status" => Ok(Reply::Json(s.status())),
        "grid" => Ok(Reply::Bytes(grid_bytes(&mut s.world))),
        "field" => {
            let name: String = arg(&args, "name")?;
            let month: Option<usize> = arg::<Option<i64>>(&args, "month")?.filter(|m| (0..12).contains(m)).map(|m| m as usize);
            let n = s.world.grid().len();
            if name == "temp" || name == "temp_final" {
                // Monthly/annual temperature including the hydrology lapse-rate correction.
                let Some((Field::F32(t), step, stale)) = s.world.field("temp") else { return Err("temperature not computed yet".into()) };
                let adj = match s.world.field("temp_adjust") {
                    Some((Field::F32(a), _, false)) => Some(a.clone()),
                    _ => None,
                };
                let mut v = match month {
                    Some(m) => t[m * n..(m + 1) * n].to_vec(),
                    None => {
                        let mut a = vec![0f32; n];
                        for m in 0..12 {
                            for i in 0..n {
                                a[i] += t[m * n + i] / 12.0;
                            }
                        }
                        a
                    }
                };
                if let Some(a) = adj {
                    v.iter_mut().zip(&a).for_each(|(x, d)| *x += d);
                }
                return Ok(Reply::Bytes(field_bytes(&Field::F32(v), stale, step, None, n)));
            }
            if let Some(live) = s.live.as_mut() {
                let step = live.step;
                if let Some(f) = live.snapshot().1.get(&name) {
                    return Ok(Reply::Bytes(field_bytes(f, false, step, month, n)));
                }
            }
            match s.world.field(&name) {
                Some((f, step, stale)) => Ok(Reply::Bytes(field_bytes(f, stale, step, month, n))),
                None => Err(format!("field `{name}` is not available")),
            }
        }
        "wind" => {
            let month: usize = arg::<i64>(&args, "month").unwrap_or(0).clamp(0, 11) as usize;
            // Interleaved (east, north) m/s per cell: the climate step's winds when
            // computed, otherwise the zonal belts alone.
            let g = s.world.grid();
            let n = g.len();
            let mut b = Vec::with_capacity(n * 8);
            if let (Some((Field::F32(u), _, _)), Some((Field::F32(v), _, _))) = (s.world.field("wind_u"), s.world.field("wind_v")) {
                for i in 0..n {
                    b.extend_from_slice(&u[month * n + i].to_le_bytes());
                    b.extend_from_slice(&v[month * n + i].to_le_bytes());
                }
            } else {
                for i in 0..n {
                    let (u, v) = climate::wind_at(&s.world.params, g.lat[i], month);
                    b.extend_from_slice(&((u * crate::stages::wind::W_REF) as f32).to_le_bytes());
                    b.extend_from_slice(&((v * crate::stages::wind::W_REF) as f32).to_le_bytes());
                }
            }
            Ok(Reply::Bytes(b))
        }
        "probe" => {
            let cell: usize = arg(&args, "cell")?;
            let month: Option<usize> = arg::<Option<i64>>(&args, "month")?.filter(|m| (0..12).contains(m)).map(|m| m as usize);
            let g = s.world.grid();
            if cell >= g.len() {
                return Err("cell out of range".into());
            }
            let n = g.len();
            let mut out = serde_json::Map::new();
            out.insert("cell".into(), json!(cell));
            out.insert("lat".into(), json!(g.lat[cell].to_degrees()));
            out.insert("lon".into(), json!(g.lon[cell].to_degrees()));
            for &st in STEPS.iter() {
                let Some(d) = &s.world.steps[st.index()] else { continue };
                for (name, f) in &d.fields.0 {
                    let v = match f {
                        Field::F32(v) if v.len() == 12 * n => match month {
                            Some(m) => json!(v[m * n + cell]),
                            None => json!((0..12).map(|m| v[m * n + cell]).collect::<Vec<_>>()),
                        },
                        Field::F32(v) if v.len() == n => json!(v[cell]),
                        Field::U8(v) if v.len() == n => json!(v[cell]),
                        Field::U16(v) if v.len() == n => json!(v[cell]),
                        Field::U32(v) if v.len() == n => json!(v[cell]),
                        Field::I32(v) if v.len() == n => json!(v[cell]),
                        _ => continue,
                    };
                    out.insert(name.clone(), v);
                }
            }
            // A live simulation's values shadow the committed ones.
            if let Some(live) = s.live.as_mut() {
                for (name, f) in &live.snapshot().1 .0 {
                    let v = match f {
                        Field::F32(v) if v.len() == n => json!(v[cell]),
                        Field::U8(v) if v.len() == n => json!(v[cell]),
                        Field::U16(v) if v.len() == n => json!(v[cell]),
                        Field::U32(v) if v.len() == n => json!(v[cell]),
                        _ => continue,
                    };
                    out.insert(name.clone(), v);
                }
            }
            Ok(Reply::Json(Value::Object(out)))
        }
        "set_params" => {
            let p: WorldParams = arg(&args, "params")?;
            if p != s.world.params {
                s.world.params = p;
                s.dirty = true;
            }
            Ok(Reply::Json(s.status()))
        }
        "add_stroke" => {
            let st: Stroke = arg(&args, "stroke")?;
            if st.points.is_empty() {
                return Err("empty stroke".into());
            }
            s.snapshot_layers(&[st.tool.layer()]);
            s.world.edits.add_stroke(st);
            Ok(Reply::Json(s.status()))
        }
        "add_pin" => {
            let p: PlatePin = arg(&args, "pin")?;
            s.remember(EditHistory::Pins(s.world.edits.sketch.pins.clone()));
            s.world.edits.sketch.pins.push(p);
            Ok(Reply::Json(s.status()))
        }
        "add_arrow" => {
            let a: MotionArrow = arg(&args, "arrow")?;
            s.remember(EditHistory::Arrows(s.world.edits.sketch.arrows.clone()));
            s.world.edits.sketch.arrows.push(a);
            Ok(Reply::Json(s.status()))
        }
        "remove_pin" | "remove_arrow" => {
            let i: usize = arg(&args, "index")?;
            if cmd == "remove_pin" && i < s.world.edits.sketch.pins.len() {
                s.remember(EditHistory::Pins(s.world.edits.sketch.pins.clone()));
                s.world.edits.sketch.pins.remove(i);
            } else if cmd == "remove_arrow" && i < s.world.edits.sketch.arrows.len() {
                s.remember(EditHistory::Arrows(s.world.edits.sketch.arrows.clone()));
                s.world.edits.sketch.arrows.remove(i);
            }
            Ok(Reply::Json(s.status()))
        }
        "clear_layer" => {
            let layer: EditLayer = arg(&args, "layer")?;
            if s.world.edits.count(layer) > 0 {
                s.snapshot_layers(&[layer]);
                s.world.edits.clear_layer(layer);
            }
            Ok(Reply::Json(s.status()))
        }
        "overrides" => Ok(Reply::Json(overrides_report(&s.world))),
        "remove_edits" => {
            // { layer, indices }: drop strokes, e.g. the unapplied ones.
            let layer: EditLayer = arg(&args, "layer")?;
            let idx: Vec<usize> = arg(&args, "indices")?;
            let len = if layer == EditLayer::Directives { s.world.edits.overrides.directives.len() } else { s.world.edits.strokes(layer).len() };
            let removed = if idx.iter().any(|&i| i < len) {
                s.snapshot_layers(&[layer]);
                s.world.edits.remove_strokes(layer, &idx)
            } else { 0 };
            let mut st = s.status();
            st["removed"] = json!(removed);
            Ok(Reply::Json(st))
        }
        "export_overrides" => {
            // { path, layers? }: write an override bundle (all layers with edits by default).
            let path: String = arg(&args, "path")?;
            let layers = layer_list(&s.world, arg(&args, "layers")?)?;
            let b = s.world.edits.bundle(&layers, s.world.params.planet.seed);
            std::fs::write(&path, serde_json::to_string_pretty(&b).unwrap()).map_err(|e| format!("{path}: {e}"))?;
            Ok(Reply::Json(json!({ "path": path, "layers": layers.iter().map(|l| json!({ "layer": l.key(), "edits": s.world.edits.count(*l) })).collect::<Vec<_>>() })))
        }
        "import_overrides" => {
            // { path, layers?, replace? }: add (or replace) layers from a bundle.
            let path: String = arg(&args, "path")?;
            let only: Option<Vec<String>> = arg(&args, "layers")?;
            let only = only.map(|v| v.iter().map(|k| EditLayer::from_key(k).ok_or(format!("unknown layer `{k}`"))).collect::<Result<Vec<_>, _>>()).transpose()?;
            let replace = arg::<Option<bool>>(&args, "replace")?.unwrap_or(false);
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            let b: Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
            let mut edits = s.world.edits.clone();
            let taken = edits.apply_bundle(&b, only.as_deref(), replace)?;
            if !taken.is_empty() {
                s.snapshot_layers(&taken.iter().map(|x| x.0).collect::<Vec<_>>());
                s.world.edits = edits;
            }
            let mut st = s.status();
            st["imported"] = json!(taken.iter().map(|(l, n)| json!({ "layer": l.key(), "edits": n })).collect::<Vec<_>>());
            Ok(Reply::Json(st))
        }
        "set_auto_base" => {
            let v: bool = arg(&args, "value")?;
            if v != s.world.edits.sketch.auto_base {
                s.remember(EditHistory::AutoBase(s.world.edits.sketch.auto_base));
                s.world.edits.sketch.auto_base = v;
            }
            Ok(Reply::Json(s.status()))
        }
        "import_heightmap" => {
            // { path, encoding, lat_min, lat_max, blend } or { path: null } to remove.
            let path: Option<String> = arg(&args, "path")?;
            let imp = match path {
                Some(p) => {
                    let enc: crate::edits::HeightEncoding =
                        arg::<Option<_>>(&args, "encoding")?.unwrap_or(crate::edits::HeightEncoding::Heightmap16);
                    let lat_min = arg::<Option<f64>>(&args, "lat_min")?.unwrap_or(-90.0);
                    let lat_max = arg::<Option<f64>>(&args, "lat_max")?.unwrap_or(90.0);
                    let blend = arg::<Option<f64>>(&args, "blend")?.unwrap_or(1.0);
                    Some(crate::import::describe(&p, enc, lat_min, lat_max, blend)?)
                }
                None => None,
            };
            s.remember(EditHistory::Imports(s.world.edits.imports.clone()));
            s.world.edits.imports.elevation = imp;
            Ok(Reply::Json(s.status()))
        }
        "import_provinces" => {
            // { png, csv?, lat_min?, lat_max? } or { png: null } to remove. Files with
            // errors are refused; warnings are returned with the new status.
            let png: Option<String> = arg(&args, "png")?;
            let (imp, report) = match png {
                Some(p) => {
                    let csv: Option<String> = arg(&args, "csv")?;
                    let (imp, rep) = crate::province_import::describe(&p, csv.as_deref(), arg(&args, "lat_min")?, arg(&args, "lat_max")?)?;
                    if !rep.ok {
                        return Err(format!("Import refused: {}", rep.errors.join(" | ")));
                    }
                    (Some(imp), json!(rep))
                }
                None => (None, Value::Null),
            };
            s.remember(EditHistory::Imports(s.world.edits.imports.clone()));
            s.world.edits.imports.provinces = imp;
            let mut st = s.status();
            st["report"] = report;
            Ok(Reply::Json(st))
        }
        "validate_provinces" => {
            let png: String = arg(&args, "png")?;
            let csv: Option<String> = arg(&args, "csv")?;
            let img = crate::province_import::read_png(&png)?;
            let defs = match &csv {
                Some(c) => Some(crate::province_import::read_definition(c)?),
                None => None,
            };
            let (rep, _) = crate::province_import::validate(&img, defs.as_deref(), 8);
            Ok(Reply::Json(json!(rep)))
        }
        "political" => {
            // States, regions, continents, provinces and straits of the latest run
            // (provinces when computed, else states only).
            // With up-to-date cultures: their names, and the culture tables.
            let pick = |st: Step| s.world.meta(st).map(|m| m["table"].clone()).filter(|t| !t.is_null());
            let mut t = pick(Step::Provinces).or_else(|| pick(Step::States)).unwrap_or(Value::Null);
            if s.world.is_fresh(Step::Cultures) && !t.is_null() {
                if let Some(c) = pick(Step::Cultures) {
                    crate::stages::cultures::apply_names(&mut t, &c);
                    t["cultures"] = c["cultures"].clone();
                    t["culture_groups"] = c["groups"].clone();
                    t["culture_events"] = c["events"].clone();
                }
                if s.world.is_fresh(Step::Nations) {
                    if let Some(nm) = pick(Step::Nations) {
                        t["nations"] = nm["nations"].clone();
                        t["nation_events"] = nm["events"].clone();
                        t["railways"] = nm["railways"].clone();
                        t["city_pins"] = nm["pins"].clone();
                        t["cities"] = nm["cities"].clone();
                    }
                }
            }
            Ok(Reply::Json(t))
        }
        "undo" | "redo" => {
            // A live run cannot take back what it already simulated: undo ends it.
            s.live = None;
            let entry = if cmd == "undo" { s.undo.pop_back() } else { s.redo.pop_back() };
            if let Some(entry) = entry {
                let inverse = entry.swap(&mut s.world.edits);
                if cmd == "undo" { s.redo.push_back(inverse); } else { s.undo.push_back(inverse); }
                s.dirty = true;
            }
            Ok(Reply::Json(s.status()))
        }
        "run" => {
            let to: String = arg(&args, "to")?;
            let step = Step::from_key(&to).ok_or_else(|| format!("unknown step `{to}`"))?;
            set_progress(progress, |p| *p = ProgressState { running: true, task: "run".into(), ..Default::default() });
            let pr = progress.clone();
            let ran = s.world.run_to(step, &move |st: Step, f: f32, m: &str| {
                set_progress(&pr, |p| {
                    p.step = st.key().into();
                    p.frac = f;
                    p.msg = m.into();
                })
            });
            if !ran.is_empty() {
                s.dirty = true;
            }
            set_progress(progress, |p| p.running = false);
            let mut st = s.status();
            st["ran"] = json!(ran.iter().map(|r| r.key()).collect::<Vec<_>>());
            Ok(Reply::Json(st))
        }
        "new" => {
            let p: WorldParams = arg::<Option<WorldParams>>(&args, "params")?.unwrap_or_default();
            *s = Session::new(p);
            Ok(Reply::Json(s.status()))
        }
        "open" => {
            let path: String = arg(&args, "path")?;
            let w = World::load(std::path::Path::new(&path))?;
            *s = Session::new(w.params.clone());
            s.world = w;
            s.path = Some(PathBuf::from(path));
            Ok(Reply::Json(s.status()))
        }
        "save" => {
            let path: Option<String> = arg(&args, "path")?;
            let p = match path {
                Some(p) => PathBuf::from(p),
                None => s.path.clone().ok_or("no project path; use Save As")?,
            };
            s.world.save(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            s.path = Some(p);
            s.dirty = false;
            Ok(Reply::Json(s.status()))
        }
        "export" => {
            let opts: ExportOptions = arg::<Option<ExportOptions>>(&args, "options")?.unwrap_or_default();
            let dir: PathBuf = match arg::<Option<String>>(&args, "path")? {
                Some(p) => PathBuf::from(p),
                None => s.path.as_ref().map(|p| p.join("export")).ok_or("save the project first or choose an export folder")?,
            };
            set_progress(progress, |p| *p = ProgressState { running: true, task: "export".into(), step: "export".into(), ..Default::default() });
            let pr = progress.clone();
            let res = export(&mut s.world, &dir, &opts, &move |f: f32, m: &str| {
                set_progress(&pr, |p| {
                    p.frac = f;
                    p.msg = m.into();
                })
            });
            set_progress(progress, |p| p.running = false);
            Ok(Reply::Json(serde_json::to_value(res?).unwrap()))
        }
        "fingerprint" => Ok(Reply::Json(json!(s.world.fingerprint()))),
        "sim_actions" => {
            // The directives a stage offers (also the AI guide's tools).
            let stage: String = arg(&args, "stage")?;
            Ok(Reply::Json(json!(crate::directives::actions_for(&stage).iter().map(|a| json!({ "name": a.name, "description": a.description, "schema": a.schema })).collect::<Vec<_>>())))
        }
        "sim_start" => {
            // { stage: "cultures" | "nations" }: earlier steps are brought up to date first.
            let stage: String = arg(&args, "stage")?;
            let step = match stage.as_str() {
                "cultures" => Step::Cultures,
                "nations" => Step::Nations,
                _ => return Err(format!("`{stage}` has no step-by-step simulation (cultures, nations)")),
            };
            set_progress(progress, |p| *p = ProgressState { running: true, task: "sim".into(), ..Default::default() });
            let pr = progress.clone();
            let sim = s.world.with_ctx(step, &move |st: Step, f: f32, m: &str| set_progress(&pr, |p| {
                p.step = st.key().into();
                p.frac = f;
                p.msg = m.into();
            }), |ctx| match step {
                Step::Cultures => CultureSim::new(ctx).map(|c| LiveSim::Cultures(Box::new(c))),
                _ => NationSim::new(ctx).map(|n| LiveSim::Nations(Box::new(n))),
            });
            set_progress(progress, |p| p.running = false);
            let sim = sim.ok_or("nothing to simulate: no land can hold people")?;
            let k = step.index();
            s.live = Some(Live { sim, step, upstream_hash: s.world.expected_hashes()[k - 1], inputs: live_inputs(&s.world, step), millis: 0, cache: None });
            s.dirty = true;
            sim_state(s)
        }
        "sim_step" => {
            // { steps?, years?, to?: "era" | "end", stop_at_choice? } — one
            // step (a generation, or the current step length in years) by
            // default. With stop_at_choice, Stage 4 stops before an
            // institution is born so the user can choose its birthplace.
            let live = s.live.as_mut().ok_or("no live simulation; start one first")?;
            let years: Option<f64> = arg(&args, "years")?;
            let to: Option<String> = arg(&args, "to")?;
            let stop_at_choice = arg::<Option<bool>>(&args, "stop_at_choice")?.unwrap_or(false);
            let mut steps: u64 = arg::<Option<u64>>(&args, "steps")?.unwrap_or(1);
            // Stage 4 steps vary in length: step until the time is reached.
            let mut target: Option<f64> = None;
            if let Some(y) = years {
                match &live.sim {
                    LiveSim::Cultures(c) => steps = (y / c.years_per_tick().max(1e-9)).ceil().max(1.0) as u64,
                    LiveSim::Nations(n) => {
                        target = Some(n.time() + y);
                        steps = u64::MAX;
                    }
                }
            }
            set_progress(progress, |p| *p = ProgressState { running: true, task: "sim".into(), step: live.stage().into(), ..Default::default() });
            let era0 = live.era();
            let mut k = 0u64;
            let pr = progress.clone();
            let report = move |_f: f32, m: &str| set_progress(&pr, |p| p.msg = m.to_string());
            while !live.done() {
                if stop_at_choice && matches!(&live.sim, LiveSim::Nations(n) if n.pending_institution().is_some()) {
                    break;
                }
                match to.as_deref() {
                    Some("end") => {}
                    Some("era") if live.era() != era0 => break,
                    Some("era") => {}
                    _ if k >= steps => break,
                    _ if target.is_some_and(|t| live.position() + 1e-9 >= t) => break,
                    _ => {}
                }
                live.step_once(&report);
                k += 1;
                if k % 5 == 0 {
                    let pos = live.position();
                    set_progress(progress, |p| {
                        p.frac = 0.0;
                        p.msg = format!("{} {}", if live_unit_is_year(&live.sim) { "Year" } else { "Generation" }, (pos * 10.0).round() / 10.0);
                    });
                }
            }
            set_progress(progress, |p| p.running = false);
            sim_state(s)
        }
        "sim_state" => sim_state(s),
        "sim_directive" => {
            // { action, args, note?, by? }: applies from the next step on, and
            // is stored with the other directives so re-runs replay it.
            let live = s.live.as_ref().ok_or("no live simulation; start one first")?;
            let d = crate::directives::Directive {
                stage: live.stage().into(),
                at: live.position(),
                action: arg(&args, "action")?,
                args: args.get("args").cloned().unwrap_or(json!({})),
                note: arg::<Option<String>>(&args, "note")?.unwrap_or_default(),
                by: arg::<Option<String>>(&args, "by")?.unwrap_or_else(|| "user".into()),
            };
            crate::directives::validate(&d)?;
            if live.done() {
                return Err("the simulation has reached its end; restart it to steer it".into());
            }
            s.snapshot_layers(&[EditLayer::Directives]);
            s.world.edits.overrides.directives.push(d.clone());
            let live = s.live.as_mut().unwrap();
            match &mut live.sim {
                LiveSim::Cultures(c) => c.add_directive(d),
                LiveSim::Nations(n) => n.add_directive(d),
            }
            live.cache = None;
            sim_state(s)
        }
        "sim_commit" => {
            // Run to the end and keep the result as the step's output.
            let mut live = s.live.take().ok_or("no live simulation")?;
            set_progress(progress, |p| *p = ProgressState { running: true, task: "sim".into(), step: live.stage().into(), ..Default::default() });
            let pr = progress.clone();
            let report = move |f: f32, m: &str| set_progress(&pr, |p| {
                p.frac = f;
                p.msg = m.into();
            });
            while !live.done() {
                live.step_once(&report);
            }
            let step = live.step;
            let k = step.index();
            // Same inputs as a fresh run (upstream, parameters, edits, directives)? Then keep the result.
            let fresh_dirs: Vec<crate::directives::Directive> = crate::directives::of_stage(&s.world.edits.overrides.directives, step.key()).into_iter().cloned().collect();
            let sim_dirs = match &live.sim {
                LiveSim::Cultures(c) => c.directives().to_vec(),
                LiveSim::Nations(n) => n.directives().to_vec(),
            };
            let same = s.world.expected_hashes()[k - 1] == live.upstream_hash && live_inputs(&s.world, step) == live.inputs && fresh_dirs == sim_dirs;
            if same {
                let millis = live.millis;
                let out = match live.sim {
                    LiveSim::Cultures(c) => c.finish(&report),
                    LiveSim::Nations(n) => n.finish(),
                };
                s.world.put_step(step, out, millis);
            } else {
                let pr = progress.clone();
                s.world.run_to(step, &move |st: Step, f: f32, m: &str| set_progress(&pr, |p| {
                    p.step = st.key().into();
                    p.frac = f;
                    p.msg = m.into();
                }));
            }
            set_progress(progress, |p| p.running = false);
            s.dirty = true;
            let mut st = s.status();
            st["committed"] = json!(step.key());
            st["replayed"] = json!(!same);
            Ok(Reply::Json(st))
        }
        "sim_cancel" => {
            s.live = None;
            Ok(Reply::Json(s.status()))
        }
        _ => Err(format!("unknown command `{cmd}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::Tool;

    #[test]
    fn layer_history_swaps_only_the_changed_layer() {
        let mut edits = Edits::default();
        edits.add_stroke(Stroke { tool: Tool::PlatePaint, value: 1.0, points: vec![[0.0, 0.0]], ..Default::default() });
        edits.add_stroke(Stroke { tool: Tool::Raise, value: 100.0, points: vec![[1.0, 1.0]], ..Default::default() });
        let saved = EditHistory::Layers(vec![(EditLayer::Plates, LayerState::capture(&edits, EditLayer::Plates))]);
        edits.add_stroke(Stroke { tool: Tool::PlatePaint, value: 2.0, points: vec![[2.0, 2.0]], ..Default::default() });
        edits.add_stroke(Stroke { tool: Tool::Raise, value: 200.0, points: vec![[3.0, 3.0]], ..Default::default() });

        let redo = saved.swap(&mut edits);
        assert_eq!(edits.overrides.plates.len(), 1);
        assert_eq!(edits.overrides.elevation.len(), 2, "unrelated layer changed by undo");
        let _undo = redo.swap(&mut edits);
        assert_eq!(edits.overrides.plates.len(), 2);
        assert_eq!(edits.overrides.elevation.len(), 2);
    }
}
