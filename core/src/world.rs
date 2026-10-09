//! The world: parameters, edits, the grid and cached step results, plus the
//! project folder format.
//!
//! Project folder layout:
//! ```text
//! world.json            seed, parameters, step cache keys and metadata
//! sketch.json           continent sketch strokes, plate pins, motion arrows
//! overrides/*.json      user edit layers (plates, elevation, biomes, barriers, sites, fertility, states, provinces, bands, attraction)
//! imports.json          files that replace a stage result (e.g. an edited heightmap)
//! fields/<step>/*.bin   one little-endian binary file per field
//! export/               the Paradox-style map package
//! ```

use crate::edits::{Edits, Overrides, Sketch};
use crate::fields::{Field, Fields};
use crate::grid::Grid;
use crate::hash;
use crate::params::WorldParams;
use crate::stages::{self, Ctx, Step, StepData, Upstream, STEPS};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

pub const FORMAT: &str = "fantasy-world-maker/1";

#[derive(Serialize, Clone, Debug)]
pub struct StepStatus {
    pub key: &'static str,
    pub title: &'static str,
    /// "done", "stale" or "empty".
    pub state: &'static str,
    pub millis: u64,
    pub meta: serde_json::Value,
}

pub struct World {
    pub params: WorldParams,
    pub edits: Edits,
    grid: Option<Arc<Grid>>,
    pub steps: Vec<Option<StepData>>,
}

impl World {
    pub fn new(params: WorldParams) -> World {
        World { params, edits: Edits::default(), grid: None, steps: vec![None; STEPS.len()] }
    }

    pub fn grid(&mut self) -> Arc<Grid> {
        let level = self.params.planet.grid_level.clamp(3, 10);
        if self.grid.as_ref().map(|g| g.level) != Some(level) {
            self.grid = Some(Arc::new(Grid::new(level)));
        }
        self.grid.clone().unwrap()
    }

    pub fn grid_if_built(&self) -> Option<Arc<Grid>> {
        self.grid.clone()
    }

    pub fn expected_hashes(&self) -> [u64; stages::N_STEPS] {
        stages::input_hashes(&self.params, &self.edits)
    }

    pub fn is_fresh(&self, step: Step) -> bool {
        let h = self.expected_hashes();
        matches!(&self.steps[step.index()], Some(d) if d.hash == h[step.index()])
    }

    pub fn status(&self) -> Vec<StepStatus> {
        let h = self.expected_hashes();
        STEPS
            .iter()
            .map(|&s| {
                let (state, millis, meta) = match &self.steps[s.index()] {
                    Some(d) if d.hash == h[s.index()] => ("done", d.millis, summary_meta(&d.meta)),
                    Some(d) => ("stale", d.millis, summary_meta(&d.meta)),
                    None => ("empty", 0, serde_json::Value::Null),
                };
                StepStatus { key: s.key(), title: s.title(), state, millis, meta }
            })
            .collect()
    }

    /// Run every step up to and including `target` that is not fresh.
    pub fn run_to(&mut self, target: Step, progress: &(dyn Fn(Step, f32, &str) + Sync)) -> Vec<Step> {
        let grid = self.grid();
        let hashes = self.expected_hashes();
        let mut ran = Vec::new();
        for &s in STEPS.iter().take(target.index() + 1) {
            let k = s.index();
            if matches!(&self.steps[k], Some(d) if d.hash == hashes[k]) {
                continue;
            }
            progress(s, 0.0, "Starting");
            let t0 = Instant::now();
            let out = {
                let input = Upstream { steps: self.steps[..k].iter().map(|d| d.as_ref().unwrap()).collect() };
                let pf = |f: f32, m: &str| progress(s, f, m);
                let ctx = Ctx { grid: &grid, params: &self.params, edits: &self.edits, input, progress_fn: &pf };
                stages::run_step(s, &ctx)
            };
            self.steps[k] = Some(StepData { hash: hashes[k], fields: out.fields, meta: out.meta, millis: t0.elapsed().as_millis() as u64 });
            progress(s, 1.0, "Done");
            ran.push(s);
        }
        ran
    }

    /// Latest available version of a field (later steps shadow earlier ones).
    /// Returns the field, the step that produced it, and whether that step is stale.
    pub fn field(&self, name: &str) -> Option<(&Field, Step, bool)> {
        let n = self.grid.as_ref().map(|g| g.len())?;
        let h = self.expected_hashes();
        for &s in STEPS.iter().rev() {
            if let Some(d) = &self.steps[s.index()] {
                if let Some(f) = d.fields.get(name) {
                    if f.len() == n || f.len() == 12 * n {
                        return Some((f, s, d.hash != h[s.index()]));
                    }
                }
            }
        }
        None
    }

    pub fn meta(&self, step: Step) -> Option<&serde_json::Value> {
        self.steps[step.index()].as_ref().map(|d| &d.meta)
    }

    // ------------------------------------------------------------ persistence

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir.join("overrides"))?;
        std::fs::create_dir_all(dir.join("fields"))?;
        let mut steps_json = Vec::new();
        for &s in STEPS.iter() {
            let Some(d) = &self.steps[s.index()] else { continue };
            let sdir = dir.join("fields").join(s.key());
            std::fs::create_dir_all(&sdir)?;
            let mut fields_json = Vec::new();
            for (name, f) in &d.fields.0 {
                let file = format!("fields/{}/{}.bin", s.key(), name);
                let bytes = f.to_bytes();
                std::fs::write(dir.join(&file), &bytes)?;
                fields_json.push(serde_json::json!({
                    "name": name, "type": f.type_name(), "len": f.len(), "file": file,
                    "content_hash": hash::hex(hash::bytes(&bytes)),
                }));
            }
            steps_json.push(serde_json::json!({
                "step": s.key(), "hash": hash::hex(d.hash), "millis": d.millis, "meta": d.meta, "fields": fields_json,
            }));
        }
        let world = serde_json::json!({
            "format": FORMAT,
            "params": self.params,
            "steps": steps_json,
        });
        write_json(&dir.join("world.json"), &world)?;
        write_json(&dir.join("sketch.json"), &self.edits.sketch)?;
        write_json(&dir.join("overrides").join("plates.json"), &self.edits.overrides.plates)?;
        write_json(&dir.join("overrides").join("elevation.json"), &self.edits.overrides.elevation)?;
        write_json(&dir.join("overrides").join("biomes.json"), &self.edits.overrides.biomes)?;
        write_json(&dir.join("overrides").join("barriers.json"), &self.edits.overrides.barriers)?;
        write_json(&dir.join("overrides").join("states.json"), &self.edits.overrides.states)?;
        write_json(&dir.join("overrides").join("provinces.json"), &self.edits.overrides.provinces)?;
        write_json(&dir.join("overrides").join("sites.json"), &self.edits.overrides.sites)?;
        write_json(&dir.join("overrides").join("fertility.json"), &self.edits.overrides.fertility)?;
        write_json(&dir.join("overrides").join("bands.json"), &self.edits.overrides.bands)?;
        write_json(&dir.join("overrides").join("attraction.json"), &self.edits.overrides.attraction)?;
        write_json(&dir.join("imports.json"), &self.edits.imports)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<World, String> {
        let world: serde_json::Value = read_json(&dir.join("world.json"))?;
        if world["format"].as_str() != Some(FORMAT) {
            return Err(format!("{} is not a {FORMAT} project", dir.display()));
        }
        let params: WorldParams = serde_json::from_value(world["params"].clone()).map_err(|e| e.to_string())?;
        let sketch: Sketch = read_json(&dir.join("sketch.json")).unwrap_or_default();
        let overrides = Overrides {
            plates: read_json(&dir.join("overrides").join("plates.json")).unwrap_or_default(),
            elevation: read_json(&dir.join("overrides").join("elevation.json")).unwrap_or_default(),
            biomes: read_json(&dir.join("overrides").join("biomes.json")).unwrap_or_default(),
            barriers: read_json(&dir.join("overrides").join("barriers.json")).unwrap_or_default(),
            states: read_json(&dir.join("overrides").join("states.json")).unwrap_or_default(),
            provinces: read_json(&dir.join("overrides").join("provinces.json")).unwrap_or_default(),
            sites: read_json(&dir.join("overrides").join("sites.json")).unwrap_or_default(),
            fertility: read_json(&dir.join("overrides").join("fertility.json")).unwrap_or_default(),
            bands: read_json(&dir.join("overrides").join("bands.json")).unwrap_or_default(),
            attraction: read_json(&dir.join("overrides").join("attraction.json")).unwrap_or_default(),
        };
        let mut w = World::new(params);
        let imports = read_json(&dir.join("imports.json")).unwrap_or_default();
        w.edits = Edits { sketch, overrides, imports };
        if let Some(steps) = world["steps"].as_array() {
            'steps: for sj in steps {
                let Some(step) = sj["step"].as_str().and_then(Step::from_key) else { continue };
                let Some(h) = sj["hash"].as_str().and_then(|s| u64::from_str_radix(s, 16).ok()) else { continue };
                let mut fields = Fields::default();
                for fj in sj["fields"].as_array().into_iter().flatten() {
                    let (Some(name), Some(ty), Some(file)) = (fj["name"].as_str(), fj["type"].as_str(), fj["file"].as_str()) else {
                        continue 'steps;
                    };
                    let Ok(bytes) = std::fs::read(dir.join(file)) else { continue 'steps };
                    // A field edited or truncated outside the app invalidates the step.
                    if fj["content_hash"].as_str() != Some(&hash::hex(hash::bytes(&bytes))) {
                        continue 'steps;
                    }
                    let Some(f) = Field::from_bytes(ty, &bytes) else { continue 'steps };
                    fields.put(name, f);
                }
                w.steps[step.index()] = Some(StepData {
                    hash: h,
                    fields,
                    meta: sj["meta"].clone(),
                    millis: sj["millis"].as_u64().unwrap_or(0),
                });
            }
        }
        Ok(w)
    }

    /// Content hash of every stored field, for determinism and round-trip checks.
    pub fn fingerprint(&self) -> Vec<(String, String)> {
        let mut v = Vec::new();
        for &s in STEPS.iter() {
            if let Some(d) = &self.steps[s.index()] {
                for (name, f) in &d.fields.0 {
                    v.push((format!("{}/{}", s.key(), name), hash::hex(f.content_hash())));
                }
            }
        }
        v
    }
}

/// Step metadata for status replies: large per-entity tables (key `table`) are
/// left out and fetched separately with the `political` command.
fn summary_meta(m: &serde_json::Value) -> serde_json::Value {
    match m {
        serde_json::Value::Object(o) if o.contains_key("table") => {
            let mut o = o.clone();
            o.remove("table");
            serde_json::Value::Object(o)
        }
        _ => m.clone(),
    }
}

fn write_json<T: Serialize>(path: &Path, v: &T) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(v).map_err(std::io::Error::other)?;
    std::fs::write(path, s)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("{}: {e}", path.display()))
}
