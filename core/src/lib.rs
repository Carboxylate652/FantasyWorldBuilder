#![recursion_limit = "256"]
//! Fantasy World Maker simulation core: a deterministic, headless pipeline that
//! builds a planet on a geodesic sphere grid in ten checkpointed steps: seven
//! for the planet (Stage 1) and three for states and provinces (Stage 2).

pub mod api;
pub mod community;
pub mod directives;
pub mod edits;
pub mod export;
pub mod export_clean;
pub mod fields;
pub mod graph;
pub mod grid;
pub mod hash;
pub mod import;
pub mod names;
pub mod province_import;
pub mod noise;
pub mod params;
pub mod rng;
pub mod stages;
pub mod validate;
pub mod vec3;
pub mod world;

pub use stages::Step;
pub use world::World;
