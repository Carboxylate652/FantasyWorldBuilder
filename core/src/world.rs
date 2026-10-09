//! The world: parameters, edits, the grid and cached step results, plus the
//! project folder format.
//!
//! Project folder layout:
//! ```text
//! world.json            seed, parameters, step cache keys and metadata
//! sketch.json           continent sketch strokes, plate pins, motion arrows
//! overrides/*.json      user edit layers (plates, elevation, biomes, barriers, sites, fertility, states, provinces, bands, attraction, directives)
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
use std::io::Write;
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

    /// Bring every step before `step` up to date, then call `f` with the
    /// context `step` would run in (for a live, step-by-step simulation).
    pub fn with_ctx<R>(&mut self, step: Step, progress: &(dyn Fn(Step, f32, &str) + Sync), f: impl FnOnce(&Ctx) -> R) -> R {
        if step.index() > 0 {
            self.run_to(STEPS[step.index() - 1], progress);
        }
        let grid = self.grid();
        let k = step.index();
        let input = Upstream { steps: self.steps[..k].iter().map(|d| d.as_ref().unwrap()).collect() };
        let pf = |fr: f32, m: &str| progress(step, fr, m);
        let ctx = Ctx { grid: &grid, params: &self.params, edits: &self.edits, input, progress_fn: &pf };
        f(&ctx)
    }

    /// Store a step's result computed outside `run_to` (a live simulation run
    /// to its end), under the current input hash. The run must be the one
    /// `run_to` would compute for the same inputs.
    pub fn put_step(&mut self, step: Step, out: stages::StepOutput, millis: u64) {
        let h = self.expected_hashes();
        self.steps[step.index()] = Some(StepData { hash: h[step.index()], fields: out.fields, meta: out.meta, millis });
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
                write_atomic(&dir.join(&file), &bytes)?;
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
        // Inputs first, manifest last: world.json is the commit marker for a
        // save. Each file is replaced atomically, so an interrupted save
        // leaves either the previous file or the complete new file.
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
        write_json(&dir.join("overrides").join("directives.json"), &self.edits.overrides.directives)?;
        write_json(&dir.join("imports.json"), &self.edits.imports)?;
        write_json(&dir.join("world.json"), &world)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<World, String> {
        let world: serde_json::Value = read_json(&dir.join("world.json"))?;
        if world["format"].as_str() != Some(FORMAT) {
            return Err(format!("{} is not a {FORMAT} project", dir.display()));
        }
        let params: WorldParams = serde_json::from_value(world["params"].clone()).map_err(|e| e.to_string())?;
        let sketch: Sketch = read_json_optional(&dir.join("sketch.json"))?;
        let overrides = Overrides {
            plates: read_json_optional(&dir.join("overrides").join("plates.json"))?,
            elevation: read_json_optional(&dir.join("overrides").join("elevation.json"))?,
            biomes: read_json_optional(&dir.join("overrides").join("biomes.json"))?,
            barriers: read_json_optional(&dir.join("overrides").join("barriers.json"))?,
            states: read_json_optional(&dir.join("overrides").join("states.json"))?,
            provinces: read_json_optional(&dir.join("overrides").join("provinces.json"))?,
            sites: read_json_optional(&dir.join("overrides").join("sites.json"))?,
            fertility: read_json_optional(&dir.join("overrides").join("fertility.json"))?,
            bands: read_json_optional(&dir.join("overrides").join("bands.json"))?,
            attraction: read_json_optional(&dir.join("overrides").join("attraction.json"))?,
            directives: read_json_optional(&dir.join("overrides").join("directives.json"))?,
        };
        let mut w = World::new(params);
        let imports = read_json_optional(&dir.join("imports.json"))?;
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
                    let rel = Path::new(file);
                    let safe = !rel.is_absolute()
                        && rel.components().all(|c| matches!(c, std::path::Component::Normal(_)))
                        && rel.starts_with(Path::new("fields").join(step.key()));
                    if !safe {
                        return Err(format!("unsafe field path `{file}` in {}", dir.join("world.json").display()));
                    }
                    let Ok(bytes) = std::fs::read(dir.join(rel)) else { continue 'steps };
                    // A field edited or truncated outside the app invalidates the step.
                    if fj["content_hash"].as_str() != Some(&hash::hex(hash::bytes(&bytes))) {
                        continue 'steps;
                    }
                    let Some(f) = Field::from_bytes(ty, &bytes) else { continue 'steps };
                    if fj["len"].as_u64() != Some(f.len() as u64) {
                        continue 'steps;
                    }
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
        // Do not clone the (potentially very large) province/culture/nation
        // table just to throw it away. Status fetches tables separately.
        serde_json::Value::Object(o) if o.contains_key("table") => serde_json::Value::Object(
            o.iter().filter(|(k, _)| k.as_str() != "table").map(|(k, v)| (k.clone(), v.clone())).collect(),
        ),
        _ => m.clone(),
    }
}

fn write_json<T: Serialize>(path: &Path, v: &T) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(v).map_err(std::io::Error::other)?;
    write_atomic(path, s.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| std::io::Error::other("project file has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_json_optional<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
