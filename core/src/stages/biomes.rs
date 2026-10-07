//! Step 7 — biomes. Köppen–Geiger classes from monthly temperature and
//! precipitation (Peel et al. 2007 rules), then a smaller set of game terrains
//! that also considers relief, rivers and lakes. Biome paint overrides win.

use super::{Ctx, StepOutput};
use crate::edits::{stroke_coverage, Tool};
use crate::fields::{Field, Fields};
use rayon::prelude::*;

pub const KOPPEN: [(&str, [u8; 3]); 32] = [
    ("Ocean", [24, 52, 96]),
    ("Af", [0, 0, 255]),
    ("Am", [0, 120, 255]),
    ("Aw", [70, 170, 250]),
    ("BWh", [255, 0, 0]),
    ("BWk", [255, 150, 150]),
    ("BSh", [245, 165, 0]),
    ("BSk", [255, 220, 100]),
    ("Csa", [255, 255, 0]),
    ("Csb", [200, 200, 0]),
    ("Csc", [150, 150, 0]),
    ("Cwa", [150, 255, 150]),
    ("Cwb", [100, 200, 100]),
    ("Cwc", [50, 150, 50]),
    ("Cfa", [200, 255, 80]),
    ("Cfb", [100, 255, 80]),
    ("Cfc", [50, 200, 0]),
    ("Dsa", [255, 0, 255]),
    ("Dsb", [200, 0, 200]),
    ("Dsc", [150, 50, 150]),
    ("Dsd", [150, 100, 150]),
    ("Dwa", [170, 175, 255]),
    ("Dwb", [90, 120, 220]),
    ("Dwc", [75, 80, 180]),
    ("Dwd", [50, 0, 135]),
    ("Dfa", [0, 255, 255]),
    ("Dfb", [55, 200, 255]),
    ("Dfc", [0, 125, 125]),
    ("Dfd", [0, 70, 95]),
    ("ET", [178, 178, 178]),
    ("EF", [102, 102, 102]),
    ("Lake", [60, 110, 190]),
];

pub const TERRAIN: [(&str, [u8; 3]); 18] = [
    ("Ocean", [30, 60, 110]),
    ("Lake", [70, 120, 200]),
    ("Glacier", [235, 240, 248]),
    ("Tundra", [160, 170, 150]),
    ("Taiga", [40, 90, 70]),
    ("Forest", [50, 125, 50]),
    ("Plains", [150, 190, 90]),
    ("Steppe", [200, 190, 110]),
    ("Desert", [235, 205, 140]),
    ("Drylands", [205, 160, 95]),
    ("Mediterranean", [170, 165, 70]),
    ("Savanna", [190, 175, 70]),
    ("Jungle", [20, 100, 30]),
    ("Wetlands", [80, 120, 100]),
    ("Floodplains", [120, 165, 60]),
    ("Hills", [140, 120, 90]),
    ("Mountains", [110, 95, 85]),
    ("Highlands", [150, 135, 110]),
];

pub mod terrain {
    pub const OCEAN: u8 = 0;
    pub const LAKE: u8 = 1;
    pub const GLACIER: u8 = 2;
    pub const TUNDRA: u8 = 3;
    pub const TAIGA: u8 = 4;
    pub const FOREST: u8 = 5;
    pub const PLAINS: u8 = 6;
    pub const STEPPE: u8 = 7;
    pub const DESERT: u8 = 8;
    pub const DRYLANDS: u8 = 9;
    pub const MEDITERRANEAN: u8 = 10;
    pub const SAVANNA: u8 = 11;
    pub const JUNGLE: u8 = 12;
    pub const WETLANDS: u8 = 13;
    pub const FLOODPLAINS: u8 = 14;
    pub const HILLS: u8 = 15;
    pub const MOUNTAINS: u8 = 16;
    pub const HIGHLANDS: u8 = 17;
}

fn code(name: &str) -> u8 {
    KOPPEN.iter().position(|(n, _)| *n == name).unwrap() as u8
}

/// Köppen–Geiger class for one land cell. `north` selects the summer half-year.
pub fn koppen(t: &[f64; 12], p: &[f64; 12], north: bool, cd_threshold: f64) -> u8 {
    let mat = t.iter().sum::<f64>() / 12.0;
    let map: f64 = p.iter().sum();
    let thot = t.iter().cloned().fold(f64::MIN, f64::max);
    let tcold = t.iter().cloned().fold(f64::MAX, f64::min);
    let tmon10 = t.iter().filter(|&&x| x > 10.0).count();
    let pdry = p.iter().cloned().fold(f64::MAX, f64::min);
    let summer = |m: usize| if north { (3..9).contains(&m) } else { !(3..9).contains(&m) };
    let (mut psdry, mut pswet, mut pwdry, mut pwwet, mut ps) = (f64::MAX, 0.0f64, f64::MAX, 0.0f64, 0.0);
    for m in 0..12 {
        if summer(m) {
            psdry = psdry.min(p[m]);
            pswet = pswet.max(p[m]);
            ps += p[m];
        } else {
            pwdry = pwdry.min(p[m]);
            pwwet = pwwet.max(p[m]);
        }
    }
    let pw = map - ps;
    let pth = if pw >= 0.7 * map {
        2.0 * mat
    } else if ps >= 0.7 * map {
        2.0 * mat + 28.0
    } else {
        2.0 * mat + 14.0
    };

    if thot < 10.0 {
        return code(if thot > 0.0 { "ET" } else { "EF" });
    }
    if map < 10.0 * pth {
        let w = map < 5.0 * pth;
        let h = mat >= 18.0;
        return code(match (w, h) {
            (true, true) => "BWh",
            (true, false) => "BWk",
            (false, true) => "BSh",
            (false, false) => "BSk",
        });
    }
    if tcold >= 18.0 {
        return code(if pdry >= 60.0 {
            "Af"
        } else if pdry >= 100.0 - map / 25.0 {
            "Am"
        } else {
            "Aw"
        });
    }
    let second = if psdry < 40.0 && psdry < pwwet / 3.0 {
        's'
    } else if pwdry < pswet / 10.0 {
        'w'
    } else {
        'f'
    };
    let group = if tcold > cd_threshold { 'C' } else { 'D' };
    let third = if thot >= 22.0 {
        'a'
    } else if tmon10 >= 4 {
        'b'
    } else if group == 'D' && tcold < -38.0 {
        'd'
    } else {
        'c'
    };
    code(&format!("{group}{second}{third}"))
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let g = ctx.grid;
    let n = g.len();
    let bp = &ctx.params.biomes;
    let elev = ctx.input.f32("elevation");
    let temp = ctx.input.f32("temp");
    let precip = ctx.input.f32("precip");
    let adj = ctx.input.f32("temp_adjust");
    let lake = ctx.input.u8("lake");
    let river = ctx.input.u8("river");
    let p_ann = ctx.input.f32("p_ann");
    // Hydrology's surface map: dry basins below sea level are land, lakes are water.
    let water = ctx.input.u8("water");
    let is_lake = |i: usize| lake[i] == crate::stages::hydrology::lake::FRESH || lake[i] == crate::stages::hydrology::lake::SALT;

    ctx.progress(0.05, "Classifying climates");
    let kop: Vec<u8> = (0..n)
        .into_par_iter()
        .map(|i| {
            if water[i] == 1 {
                return 0;
            }
            if is_lake(i) {
                return 31;
            }
            let t: [f64; 12] = std::array::from_fn(|m| (temp[m * n + i] + adj[i]) as f64);
            let p: [f64; 12] = std::array::from_fn(|m| precip[m * n + i] as f64);
            koppen(&t, &p, g.lat[i] >= 0.0, bp.cd_threshold_c)
        })
        .collect();

    // Local relief over the 2-ring neighbourhood.
    ctx.progress(0.4, "Measuring relief");
    let relief: Vec<f32> = (0..n)
        .into_par_iter()
        .map(|i| {
            let (mut lo, mut hi) = (elev[i], elev[i]);
            for &a in g.neighbors(i) {
                for &b in g.neighbors(a as usize) {
                    let e = elev[b as usize].max(0.0);
                    lo = lo.min(e);
                    hi = hi.max(e);
                }
            }
            hi - lo
        })
        .collect();

    ctx.progress(0.6, "Assigning terrain");
    use terrain::*;
    let mut ter: Vec<u8> = (0..n)
        .into_par_iter()
        .map(|i| {
            let e = elev[i] as f64;
            if water[i] == 1 {
                return OCEAN;
            }
            if is_lake(i) {
                return LAKE;
            }
            let k = KOPPEN[kop[i] as usize].0;
            let rel = relief[i] as f64;
            if k == "EF" {
                return GLACIER;
            }
            if (e > bp.mountain_elev_m && rel > bp.mountain_relief_m * 0.5) || rel > bp.mountain_relief_m * 1.6 {
                return MOUNTAINS;
            }
            if e > bp.mountain_elev_m * 0.8 && rel < bp.hill_relief_m * 1.5 {
                return HIGHLANDS;
            }
            if rel > bp.hill_relief_m && e > 250.0 {
                return HILLS;
            }
            let pa = p_ann[i] as f64;
            let first = k.as_bytes()[0];
            if river[i] >= 3 && first == b'B' {
                return FLOODPLAINS;
            }
            if rel < 60.0 && e < 200.0 && pa > 1100.0 && (river[i] >= 2 || g.neighbors(i).iter().any(|&j| is_lake(j as usize))) {
                return WETLANDS;
            }
            match k {
                "ET" => TUNDRA,
                "BWh" | "BWk" => DESERT,
                "BSh" => DRYLANDS,
                "BSk" => STEPPE,
                "Af" | "Am" => JUNGLE,
                "Aw" => SAVANNA,
                "Csa" | "Csb" | "Csc" => MEDITERRANEAN,
                "Dfc" | "Dfd" | "Dwc" | "Dwd" | "Dsc" | "Dsd" => TAIGA,
                _ => {
                    if pa >= bp.forest_precip_mm {
                        FOREST
                    } else {
                        PLAINS
                    }
                }
            }
        })
        .collect();

    // Biome paint overrides.
    let ter0 = ter.clone();
    let mut scratch = Vec::new();
    let mut overridden = 0;
    for s in &ctx.edits.overrides.biomes {
        let id = s.value.round() as i64;
        for (c, w) in stroke_coverage(g, s, ctx.params.planet.radius_km, &mut scratch) {
            let c = c as usize;
            if w < 0.5 || water[c] != 0 {
                continue;
            }
            match s.tool {
                Tool::BiomePaint if (2..TERRAIN.len() as i64).contains(&id) => {
                    ter[c] = id as u8;
                    overridden += 1;
                }
                Tool::BiomeErase => ter[c] = ter0[c],
                _ => {}
            }
        }
    }

    // Area share of each class over land.
    let r2 = ctx.params.planet.radius_km.powi(2);
    let mut k_area = vec![0.0f64; KOPPEN.len()];
    let mut t_area = vec![0.0f64; TERRAIN.len()];
    let mut land = 0.0;
    for i in 0..n {
        if water[i] == 0 {
            k_area[kop[i] as usize] += g.area[i] * r2;
            t_area[ter[i] as usize] += g.area[i] * r2;
            land += g.area[i] * r2;
        }
    }
    let share = |v: &[f64], names: &[(&str, [u8; 3])]| -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for (k, a) in v.iter().enumerate() {
            if *a > 0.0 {
                m.insert(names[k].0.to_string(), serde_json::json!(a / land.max(1.0)));
            }
        }
        serde_json::Value::Object(m)
    };

    let mut f = Fields::default();
    f.put("koppen", Field::U8(kop));
    f.put("terrain", Field::U8(ter));
    f.put("relief", Field::F32(relief));
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "koppen_share": share(&k_area, &KOPPEN),
            "terrain_share": share(&t_area, &TERRAIN),
            "overridden_cells": overridden,
        }),
    }
}
