//! Command interface shared by the Tauri app and the headless HTTP server.
//! Every command takes JSON arguments and returns either JSON or raw bytes.

use crate::edits::{EditLayer, Edits, MotionArrow, PlatePin, Stroke};
use crate::export::{export, ExportOptions};
use crate::fields::Field;
use crate::params::WorldParams;
use crate::stages::{climate, Step, STEPS};
use crate::world::World;
use serde_json::{json, Value};
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
    undo: Vec<Edits>,
    redo: Vec<Edits>,
    pub dirty: bool,
}

impl Default for Session {
    fn default() -> Self {
        Session::new(WorldParams::default())
    }
}

impl Session {
    pub fn new(params: WorldParams) -> Session {
        Session { world: World::new(params), path: None, undo: vec![], redo: vec![], dirty: false }
    }

    fn snapshot(&mut self) {
        self.undo.push(self.world.edits.clone());
        if self.undo.len() > 500 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
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
                "elevation_import": e.imports.elevation,
                "province_import": e.imports.provinces,
            },
            "grid": { "level": grid.level, "cells": grid.len(), "triangles": grid.tris.len(), "spacing_km": grid.spacing * self.world.params.planet.radius_km },
            "path": self.path.as_ref().map(|p| p.display().to_string()),
            "can_undo": !self.undo.is_empty(),
            "can_redo": !self.redo.is_empty(),
            "dirty": self.dirty,
        })
    }
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
            s.snapshot();
            s.world.edits.add_stroke(st);
            Ok(Reply::Json(s.status()))
        }
        "add_pin" => {
            let p: PlatePin = arg(&args, "pin")?;
            s.snapshot();
            s.world.edits.sketch.pins.push(p);
            Ok(Reply::Json(s.status()))
        }
        "add_arrow" => {
            let a: MotionArrow = arg(&args, "arrow")?;
            s.snapshot();
            s.world.edits.sketch.arrows.push(a);
            Ok(Reply::Json(s.status()))
        }
        "remove_pin" | "remove_arrow" => {
            let i: usize = arg(&args, "index")?;
            s.snapshot();
            let sk = &mut s.world.edits.sketch;
            if cmd == "remove_pin" && i < sk.pins.len() {
                sk.pins.remove(i);
            } else if cmd == "remove_arrow" && i < sk.arrows.len() {
                sk.arrows.remove(i);
            }
            Ok(Reply::Json(s.status()))
        }
        "clear_layer" => {
            let layer: EditLayer = arg(&args, "layer")?;
            s.snapshot();
            s.world.edits.clear_layer(layer);
            Ok(Reply::Json(s.status()))
        }
        "set_auto_base" => {
            let v: bool = arg(&args, "value")?;
            if v != s.world.edits.sketch.auto_base {
                s.snapshot();
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
            s.snapshot();
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
            s.snapshot();
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
            let pick = |st: Step| s.world.meta(st).map(|m| m["table"].clone()).filter(|t| !t.is_null());
            let t = pick(Step::Provinces).or_else(|| pick(Step::States)).unwrap_or(Value::Null);
            Ok(Reply::Json(t))
        }
        "undo" | "redo" => {
            let (from, to) = if cmd == "undo" { (&mut s.undo, &mut s.redo) } else { (&mut s.redo, &mut s.undo) };
            if let Some(e) = from.pop() {
                to.push(std::mem::replace(&mut s.world.edits, e));
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
        _ => Err(format!("unknown command `{cmd}`")),
    }
}
