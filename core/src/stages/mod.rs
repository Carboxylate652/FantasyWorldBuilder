//! The pipeline: seven Stage 1 steps (make the planet), three Stage 2 steps
//! (states and provinces) and the Stage 3 step (cultures). Each step reads
//! plain fields from earlier steps and writes its own `Fields` plus a JSON
//! `meta` summary.

pub mod biomes;
pub mod climate;
pub mod cultures;
pub mod habitability;
pub mod hydrology;
pub mod partition;
pub mod plates;
pub mod provinces;
pub mod resources;
pub mod sketch;
pub mod states;
pub mod tectonics;
pub mod wind;

use crate::edits::Edits;
use crate::fields::{Field, Fields};
use crate::grid::Grid;
use crate::hash;
use crate::params::WorldParams;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Step {
    Planet = 0,
    Sketch,
    Plates,
    Relief,
    Climate,
    Hydrology,
    Biomes,
    Habitability,
    States,
    Provinces,
    Cultures,
}

pub const N_STEPS: usize = 11;
pub const STEPS: [Step; N_STEPS] = [
    Step::Planet,
    Step::Sketch,
    Step::Plates,
    Step::Relief,
    Step::Climate,
    Step::Hydrology,
    Step::Biomes,
    Step::Habitability,
    Step::States,
    Step::Provinces,
    Step::Cultures,
];
/// The last step (`run` without a target goes this far).
pub const LAST: Step = Step::Cultures;

impl Step {
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn key(self) -> &'static str {
        match self {
            Step::Planet => "planet",
            Step::Sketch => "sketch",
            Step::Plates => "plates",
            Step::Relief => "relief",
            Step::Climate => "climate",
            Step::Hydrology => "hydrology",
            Step::Biomes => "biomes",
            Step::Habitability => "habitability",
            Step::States => "states",
            Step::Provinces => "provinces",
            Step::Cultures => "cultures",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Step::Planet => "Planet parameters",
            Step::Sketch => "Continent sketch",
            Step::Plates => "Plates",
            Step::Relief => "Tectonic relief",
            Step::Climate => "Climate",
            Step::Hydrology => "Hydrology & erosion",
            Step::Biomes => "Biomes",
            Step::Habitability => "Habitability & barriers",
            Step::States => "States",
            Step::Provinces => "Provinces",
            Step::Cultures => "Cultures",
        }
    }
    pub fn from_key(k: &str) -> Option<Step> {
        STEPS.iter().copied().find(|s| s.key() == k)
    }
}

pub struct StepOutput {
    pub fields: Fields,
    pub meta: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct StepData {
    pub hash: u64,
    pub fields: Fields,
    pub meta: serde_json::Value,
    pub millis: u64,
}

/// Read-only view over the outputs of earlier steps. Later steps shadow
/// earlier ones (e.g. hydrology's eroded `elevation` replaces relief's).
pub struct Upstream<'a> {
    pub steps: Vec<&'a StepData>,
}

impl<'a> Upstream<'a> {
    pub fn field(&self, name: &str) -> &'a Field {
        for s in self.steps.iter().rev() {
            if let Some(f) = s.fields.get(name) {
                return f;
            }
        }
        panic!("upstream field `{name}` not available")
    }
    pub fn f32(&self, name: &str) -> &'a [f32] {
        match self.field(name) {
            Field::F32(v) => v,
            _ => panic!("field {name} is not f32"),
        }
    }
    pub fn u8(&self, name: &str) -> &'a [u8] {
        match self.field(name) {
            Field::U8(v) => v,
            _ => panic!("field {name} is not u8"),
        }
    }
    pub fn u16(&self, name: &str) -> &'a [u16] {
        match self.field(name) {
            Field::U16(v) => v,
            _ => panic!("field {name} is not u16"),
        }
    }
    pub fn u32(&self, name: &str) -> &'a [u32] {
        match self.field(name) {
            Field::U32(v) => v,
            _ => panic!("field {name} is not u32"),
        }
    }
    pub fn i32(&self, name: &str) -> &'a [i32] {
        match self.field(name) {
            Field::I32(v) => v,
            _ => panic!("field {name} is not i32"),
        }
    }
    pub fn meta(&self, key: &str) -> &'a serde_json::Value {
        for s in self.steps.iter().rev() {
            if let Some(v) = s.meta.get(key) {
                return v;
            }
        }
        &serde_json::Value::Null
    }
}

pub type ProgressFn<'a> = &'a (dyn Fn(f32, &str) + Sync);

pub struct Ctx<'a> {
    pub grid: &'a Grid,
    pub params: &'a WorldParams,
    pub edits: &'a Edits,
    pub input: Upstream<'a>,
    pub progress_fn: ProgressFn<'a>,
}

impl<'a> Ctx<'a> {
    pub fn progress(&self, f: f32, msg: &str) {
        (self.progress_fn)(f, msg)
    }
}

pub fn run_step(step: Step, ctx: &Ctx) -> StepOutput {
    match step {
        Step::Planet => {
            let g = ctx.grid;
            let r = ctx.params.planet.radius_km;
            StepOutput {
                fields: Fields::default(),
                meta: serde_json::json!({
                    "cells": g.len(),
                    "spacing_km": g.spacing * r,
                    "cell_area_km2": 4.0 * std::f64::consts::PI * r * r / g.len() as f64,
                    "surface_mkm2": 4.0 * std::f64::consts::PI * r * r / 1e6,
                }),
            }
        }
        Step::Sketch => sketch::run(ctx),
        Step::Plates => plates::run(ctx),
        Step::Relief => tectonics::run(ctx),
        Step::Climate => climate::run(ctx),
        Step::Hydrology => hydrology::run(ctx),
        Step::Biomes => biomes::run(ctx),
        Step::Habitability => habitability::run(ctx),
        Step::States => states::run(ctx),
        Step::Provinces => provinces::run(ctx),
        Step::Cultures => cultures::run(ctx),
    }
}

/// Algorithm version of each step. Bump a step's number whenever its model
/// changes, so results cached by an older build are recomputed, not reused.
pub const MODEL_VERSION: [u64; N_STEPS] = [1, 2, 2, 2, 4, 6, 2, 5, 2, 5, 5];

/// Cache keys: each step hashes only the inputs it actually reads, chained to
/// the previous step's key, so a change only invalidates what depends on it.
pub fn input_hashes(p: &WorldParams, e: &Edits) -> [u64; N_STEPS] {
    let pl = &p.planet;
    let h0 = hash::json(&("planet", pl.seed, pl.grid_level, pl.radius_km));
    let h1 = hash::combine(h0, hash::json(&("sketch", &p.sketch, pl.ocean_fraction, e.sketch.auto_base, &e.sketch.strokes)));
    let h2 = hash::combine(h1, hash::json(&("plates", &p.plates, &e.sketch.pins, &e.sketch.arrows, &e.overrides.plates)));
    let h3 = hash::combine(h2, hash::json(&("relief", &p.tectonics, &e.overrides.elevation, &e.imports.elevation)));
    let h4 = hash::combine(
        h3,
        hash::json(&(
            "climate",
            &p.climate,
            [pl.axial_tilt_deg, pl.day_length_h, pl.year_length_days, pl.eccentricity, pl.perihelion_deg, pl.solar_constant, pl.greenhouse],
        )),
    );
    let h5 = hash::combine(h4, hash::json(&("hydrology", &p.hydrology)));
    let h6 = hash::combine(h5, hash::json(&("biomes", &p.biomes, &e.overrides.biomes)));
    let h7 = hash::combine(h6, hash::json(&("habitability", &p.habitability, &e.overrides.barriers, &e.overrides.sites, &e.overrides.fertility)));
    let h8 = hash::combine(h7, hash::json(&("states", &p.states, &e.overrides.states)));
    let h9 = hash::combine(h8, hash::json(&("provinces", &p.provinces, &e.overrides.provinces, &e.imports.provinces)));
    let h10 = hash::combine(h9, hash::json(&("cultures", &p.cultures, &e.overrides.bands, &e.overrides.attraction)));
    let h = [h0, h1, h2, h3, h4, h5, h6, h7, h8, h9, h10];
    // Fold in the model versions, preserving the chain (a version bump in one
    // step invalidates every later step too).
    let mut out = [0u64; N_STEPS];
    let mut carry = 0u64;
    for k in 0..N_STEPS {
        carry = hash::combine(carry, MODEL_VERSION[k]);
        out[k] = hash::combine(h[k], carry);
    }
    out
}
