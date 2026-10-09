//! Resources and trade goods per province, from data the pipeline already has.
//!
//! - Metals: copper, gold and silver in mountain belts (tectonic stress, the
//!   arcs and collision ranges where ore forms); iron, and some gold, in old
//!   shields (continental crust far from any plate boundary).
//! - Coal in humid lowland basins on continental crust (old swamp forests).
//! - Salt in dry basins: playas, salt lakes and closed basins.
//! - Fish on coastal and shelf seas, richest in cool waters.
//! - Oil in sedimentary basins (lowlands on old continental crust, and the
//!   salt basins where domes trap it): worth little before the start date's
//!   industrial era, but the reason for many of its towns and wars.
//! - A trade good from the land itself: grain on plains and floodplains,
//!   wine on Mediterranean coasts, horses on the steppe, furs in the taiga,
//!   dates at desert springs, and so on.
//! - Cash crops of the plantation era, by climate rather than terrain:
//!   rubber and sugar in the wet tropics, coffee in tropical highlands, tea on
//!   humid subtropical hills, cotton on warm river plains, silk and tobacco in
//!   warm temperate country. A fitting province grows one with a chance, so
//!   the old staple goods stay common.
//!
//! Each deposit is drawn with a probability that grows with its signal, from
//! (seed, province id), so results are deterministic.

use super::biomes::terrain;
use crate::rng::hash_unit;

pub const METALS: [&str; 4] = ["copper", "gold", "silver", "iron"];

/// Deposits, in the order the goods editor numbers them (101–107 add,
/// 201–207 remove). Append only: the numbers are stored in edit strokes.
pub const DEPOSITS: [&str; 7] = ["copper", "gold", "silver", "iron", "coal", "salt", "oil"];

/// Trade goods, in the order of the `trade_good` field (value = index + 1;
/// 0 = none). Append only: the values are stored in goods-editor strokes.
/// Keep in sync with TRADE_GOODS in app/src/layers.ts.
pub const TRADE_GOODS: [&str; 22] = [
    "grain", "wine", "horses", "wool", "cattle", "wood", "furs", "spices", "fish", "stone", "metals", "dates", "salt", "camels", "none",
    "cotton", "sugar", "coffee", "tea", "tobacco", "rubber", "silk",
];

/// Value of "none" in the goods editor's numbering (not a paintable good).
pub const NONE_VALUE: usize = 15;

/// Value of a trade good in the `trade_good` field.
pub fn good_index(good: &str) -> u8 {
    TRADE_GOODS.iter().position(|g| *g == good).map_or(0, |k| if good == "none" { 0 } else { k as u8 + 1 })
}

/// Category of a good or deposit, for tables and legends.
pub fn category(name: &str, kind: &str) -> &'static str {
    match (kind, name) {
        ("sea zone", _) | (_, "fish") => "sea",
        (_, "cotton" | "sugar" | "coffee" | "tea" | "tobacco" | "rubber" | "silk" | "spices" | "dates" | "wine") => "cash crop",
        (_, "horses" | "wool" | "cattle" | "camels") => "livestock",
        (_, "wood" | "furs") => "forest",
        (_, "coal" | "oil") => "energy",
        (_, "copper" | "gold" | "silver" | "iron" | "metals" | "stone" | "salt") => "mineral",
        _ => "staple",
    }
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
    /// Sedimentary basin: lowland on continental crust away from plate
    /// boundaries, whatever the rainfall.
    pub sediment: f64,
    /// Mean annual temperature (°C) and elevation (m) of the land.
    pub temp_c: f64,
    pub elev_m: f64,
}

/// Stream for resource draws.
const STREAM: u64 = 18;

/// Deposits of a land or wasteland province, by draw against its signals.
pub fn deposits(seed: u64, id: u32, s: &Signals) -> Vec<&'static str> {
    let chances: [(&str, f64); 7] = [
        ("copper", 0.35 * s.orogen),
        ("gold", 0.15 * s.orogen + 0.03 * s.shield),
        ("silver", 0.12 * s.orogen),
        ("iron", 0.1 * s.shield + 0.1 * s.orogen),
        ("coal", 0.15 * s.coal),
        ("salt", 2.5 * s.salt),
        ("oil", 0.05 * s.sediment + 0.2 * s.salt),
    ];
    chances
        .iter()
        .enumerate()
        .filter(|(k, (_, c))| hash_unit(seed, STREAM, id as u64 * 16 + *k as u64) < c.clamp(0.0, 0.9))
        .map(|(_, (name, _))| *name)
        .collect()
}

/// The main trade good of a land province: a cash crop when its climate fits
/// one and the draw allows, otherwise the staple of its dominant terrain.
pub fn trade_good(seed: u64, id: u32, dominant: u8, s: &Signals, deposits: &[&str]) -> &'static str {
    cash_crop(seed, id, dominant, s).unwrap_or_else(|| staple(dominant, s, deposits))
}

/// A cash crop the province's climate supports, drawn with that crop's chance.
/// Candidates are tried in order; each has its own draw.
pub fn cash_crop(seed: u64, id: u32, dominant: u8, s: &Signals) -> Option<&'static str> {
    use terrain::*;
    let (t, p, e) = (s.temp_c, s.rain_mm, s.elev_m);
    let lowland = e < 600.0;
    let farmland = matches!(dominant, PLAINS | FLOODPLAINS | SAVANNA | FOREST | JUNGLE | HILLS | MEDITERRANEAN | WETLANDS);
    let candidates: [(&'static str, bool, f64); 7] = [
        ("rubber", dominant == JUNGLE && t > 23.0 && p > 1800.0, 0.5),
        ("coffee", farmland | matches!(dominant, HIGHLANDS) && (16.0..25.0).contains(&t) && p > 1100.0 && (350.0..2400.0).contains(&e), 0.6),
        ("sugar", matches!(dominant, JUNGLE | SAVANNA | PLAINS | FLOODPLAINS | WETLANDS) && t > 21.0 && p > 1000.0 && lowland, 0.4),
        ("tea", farmland | matches!(dominant, HIGHLANDS) && (11.0..22.0).contains(&t) && p > 1150.0 && e > 150.0, 0.5),
        ("cotton", matches!(dominant, PLAINS | FLOODPLAINS | SAVANNA | DRYLANDS) && t > 17.0 && lowland && (450.0..1300.0).contains(&p), 0.35),
        ("silk", matches!(dominant, PLAINS | FLOODPLAINS | HILLS | FOREST | MEDITERRANEAN) && (12.0..22.0).contains(&t) && p > 800.0, 0.3),
        ("tobacco", farmland && (14.0..24.0).contains(&t) && (750.0..1600.0).contains(&p) && lowland, 0.2),
    ];
    candidates
        .iter()
        .enumerate()
        .find(|(k, (_, fits, chance))| *fits && hash_unit(seed, STREAM, id as u64 * 16 + 8 + *k as u64) < *chance)
        .map(|(_, (name, _, _))| *name)
}

/// The staple good of a land province, from its dominant terrain.
pub fn staple(dominant: u8, s: &Signals, deposits: &[&str]) -> &'static str {
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
