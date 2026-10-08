//! Resources and trade goods per province, from data the pipeline already has.
//!
//! - Metals: copper, gold and silver in mountain belts (tectonic stress, the
//!   arcs and collision ranges where ore forms); iron, and some gold, in old
//!   shields (continental crust far from any plate boundary).
//! - Coal in humid lowland basins on continental crust (old swamp forests).
//! - Salt in dry basins: playas, salt lakes and closed basins.
//! - Fish on coastal and shelf seas, richest in cool waters.
//! - A trade good from the land itself: grain on plains and floodplains,
//!   wine on Mediterranean coasts, horses on the steppe, furs in the taiga,
//!   dates at desert springs, and so on.
//!
//! Each deposit is drawn with a probability that grows with its signal, from
//! (seed, province id), so results are deterministic.

use super::biomes::terrain;
use crate::rng::hash_unit;

pub const METALS: [&str; 4] = ["copper", "gold", "silver", "iron"];

/// Deposits, in the order the goods editor numbers them (101–106 add, 201–206 remove).
pub const DEPOSITS: [&str; 6] = ["copper", "gold", "silver", "iron", "coal", "salt"];

/// Trade goods, in the order of the `trade_good` field (value = index + 1;
/// 0 = none). Keep in sync with TRADE_GOODS in app/src/layers.ts.
pub const TRADE_GOODS: [&str; 16] = [
    "grain", "wine", "horses", "wool", "cattle", "wood", "furs", "spices", "fish", "stone", "metals", "dates", "salt", "camels", "none", "",
];

/// Value of a trade good in the `trade_good` field.
pub fn good_index(good: &str) -> u8 {
    TRADE_GOODS.iter().position(|g| *g == good).map_or(0, |k| if good == "none" { 0 } else { k as u8 + 1 })
}

/// Per-province signals, each an area-weighted mean over its cells (0–1).
#[derive(Clone, Default, Debug)]
pub struct Signals {
    /// Tectonic stress (mountain belts).
    pub orogen: f64,
    /// Continental crust with no tectonic stress (old shields).
    pub shield: f64,
    /// Humid lowland basin on continental crust.
    pub coal: f64,
    /// Playa, salt lake shore or closed basin.
    pub salt: f64,
    /// Strongest spring or site in the province.
    pub site: f64,
    /// Mean annual rain (mm).
    pub rain_mm: f64,
    /// Mean latitude (degrees, absolute).
    pub abs_lat: f64,
}

/// Stream for resource draws.
const STREAM: u64 = 18;

/// Deposits of a land or wasteland province, by draw against its signals.
pub fn deposits(seed: u64, id: u32, s: &Signals) -> Vec<&'static str> {
    let chances: [(&str, f64); 6] = [
        ("copper", 0.35 * s.orogen),
        ("gold", 0.15 * s.orogen + 0.03 * s.shield),
        ("silver", 0.12 * s.orogen),
        ("iron", 0.1 * s.shield + 0.1 * s.orogen),
        ("coal", 0.15 * s.coal),
        ("salt", 2.5 * s.salt),
    ];
    chances
        .iter()
        .enumerate()
        .filter(|(k, (_, c))| hash_unit(seed, STREAM, id as u64 * 16 + *k as u64) < c.clamp(0.0, 0.9))
        .map(|(_, (name, _))| *name)
        .collect()
}

/// The main trade good of a land province, from its dominant terrain.
pub fn trade_good(dominant: u8, s: &Signals, deposits: &[&str]) -> &'static str {
    match dominant {
        terrain::PLAINS | terrain::FLOODPLAINS => "grain",
        terrain::MEDITERRANEAN => "wine",
        terrain::STEPPE => "horses",
        terrain::DRYLANDS => "wool",
        terrain::SAVANNA => "cattle",
        terrain::FOREST => "wood",
        terrain::TAIGA | terrain::TUNDRA => "furs",
        terrain::JUNGLE => "spices",
        terrain::WETLANDS => "fish",
        terrain::HILLS | terrain::HIGHLANDS => "wool",
        terrain::MOUNTAINS => {
            if deposits.iter().any(|d| METALS.contains(d)) {
                "metals"
            } else {
                "stone"
            }
        }
        terrain::DESERT => {
            if s.site > 0.3 {
                "dates"
            } else if deposits.contains(&"salt") {
                "salt"
            } else {
                "camels"
            }
        }
        terrain::GLACIER => "none",
        _ => "grain",
    }
}

/// Fish stock of a sea zone: coastal and shelf seas, richest in cool waters.
pub fn sea_fish(seed: u64, id: u32, band: u8, abs_lat: f64) -> bool {
    if band > 1 {
        return false;
    }
    let cool = 1.0 - ((abs_lat - 50.0).abs() / 35.0).clamp(0.0, 1.0);
    hash_unit(seed, STREAM, id as u64 * 16 + 15) < 0.25 + 0.6 * cool
}
