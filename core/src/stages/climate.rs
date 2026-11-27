//! Step 5 — climate estimator. Ad-hoc but physically motivated, in the spirit
//! of the geometrian climate simulator, extended with seasons. Its constants
//! are tuned on Earth's real terrain (`core/examples/earth_check.rs`).
//!
//! 1. Daily-mean insolation by latitude for every day of the year.
//! 2. A 1-D Budyko–Sellers energy balance in x = sin(lat), integrated through
//!    the year until periodic. The tropics mix their heat (Hadley cells) and
//!    transport weakens toward the poles. Sea ice forms where the sea freezes;
//!    ice sheets last on land whose summer stays below freezing, build their
//!    own height and cool the latitudes around them. Three runs: ocean-like
//!    and land-like heat capacity give the seasonal swing for maritime and
//!    continental places; a land-fraction-weighted run gives the annual mean.
//! 3. Local temperature: continentality from maritime air carried along each
//!    half-year's wind (and held back by mountains), sea ice that makes a
//!    frozen sea continental, warm/cold coastal currents, colder interiors at
//!    high latitudes, temperature carried downwind, and three lapse rates
//!    (free air on peaks, a gentler one on broad plateaus, a steeper one on
//!    ice sheets). Dry ground swings further with the seasons.
//! 4. Winds: three belts whose convergence line follows the heat in each
//!    longitude sector (monsoons: trades that cross the equator turn into
//!    westerlies), blended so they never break, plus thermal winds (wind.rs).
//! 5. Moisture: humidity carried along the wind as fluxes across cell edges
//!    (conserving water), evaporation from the sea and, as a share of the
//!    rain, from land; showery and layered rain from relative humidity, wide
//!    mixing by storms, orographic cloud and rain shadows.
//! 6. Foehn: air that rained out climbing a range warms the land below the
//!    lee slope.
//!
//! Temperature, winds and rain depend on each other, so the step solves
//! twice: a rough pass (four months) finds where the ground is dry, then
//! temperature and winds are redone with it and the full pass follows.
//!
//! The solve runs on a coarser grid (level 7 by default) and is interpolated
//! onto the world grid, where the lapse rate is reapplied with full detail.

use super::{Ctx, StepOutput};
use crate::fields::{Field, Fields};
use crate::grid::{Grid, GridTransfer};
use crate::params::WorldParams;
use crate::vec3::Vec3;
use rayon::prelude::*;
use std::f64::consts::{PI, TAU};
use std::sync::{Arc, Mutex, OnceLock};

const NB: usize = 90; // EBM bands, uniform in x = sin(lat)
const A_OLR: f64 = 192.4;
const B_OLR: f64 = 2.09;
const C_OCEAN: f64 = 1.0e8; // ~25 m mixed layer, J/m²/K
const C_LAND: f64 = 2.5e7; // land + the atmosphere column mixing over it
const C_ICE: f64 = 3.0e7; // sea ice shuts the mixed layer off from the air
/// Planetary albedo of sea ice and of an ice sheet (snow on land ice is
/// brighter and does not open into leads).
const ALBEDO_SEA_ICE: f64 = 0.62;
const ALBEDO_ICE_SHEET: f64 = 0.80;
/// Heat an ice sheet loses beyond the albedo effect, W/m² per °C that its
/// surface is colder than sea level (that is, per ~150 m of height).
const ICE_SHEET_SINK: f64 = 14.0;
/// How much warmer the summer of land surrounded by sea is than the mean of its latitude (°C).
const LAND_SUMMER_EXCESS_C: f64 = 10.0;
/// The most heat all ice sheets together drain, as a global mean (W/m²).
const ICE_SHEET_SINK_MAX: f64 = 7.5;
/// Height (m) to which an ice sheet builds its own surface, on whatever
/// ground it covers.
const ICE_DOME_M: f64 = 2200.0;
/// Latitude span (degrees) over which that loss is felt.
const ICE_SHEET_REACH_DEG: f64 = 45.0;
/// Extra mixing of heat inside the tropics, as a multiple of the base
/// diffusivity: the Hadley cells keep the tropics nearly uniform, so the
/// temperature gradient sits in the mid-latitudes. It acts within
/// TROPIC_WIDTH of the Hadley cells' width from the equator.
const TROPIC_MIX: f64 = 3.0;
const TROPIC_WIDTH: f64 = 0.72;
/// Share of the heat transport lost toward the poles (less ocean transport
/// under ice, weaker storms).
const POLAR_DAMP: f64 = 0.44;
const DAY_S: f64 = 86400.0;
/// Parched ground and the dry air over it swing further with the seasons
/// (°C at `dry_heat` = 1): this much warmer over the year, and this much
/// more again at the height of summer, less in the depth of winter.
const DRY_HEAT_MEAN_C: f64 = 1.5;
const DRY_HEAT_SWING_C: f64 = 4.0;
/// Moist ground, cooled by evaporation and cloud, is this much cooler than the sea at its latitude.
const WET_LAND_COOL_C: f64 = 1.2;
/// Scale over which heights are averaged into "broad" highlands (km).
const BROAD_HEIGHT_KM: f64 = 250.0;
/// Distance (km) from an ice sheet's edge over which it reaches its full thickness.
const ICE_DOME_EDGE_KM: f64 = 500.0;
/// Temperature drop per km of height on an ice sheet (°C).
const ICE_SHEET_LAPSE: f64 = 8.5;
/// Seas narrower than about this (km) are weak sources of maritime air.
const OPEN_SEA_KM: f64 = 400.0;
/// Share of the maritime air around a place that reaches it against or across the wind.
const MARITIME_ANY_SIDE: f64 = 0.3;
/// The most continental the seasons of a sea cell can be.
const SEA_SWING_MAX: f64 = 0.4;
/// How much of the land's own summer warmth a fully maritime place keeps.
const LAND_SUMMER: f64 = 0.5;
/// Sea water freezes at this sea-level air temperature, and is fully ice-covered at the second.
const SEA_FREEZE_C: f64 = -1.8;
const SEA_ICE_FULL_C: f64 = -7.0;
/// How continental the air over full sea ice is (1 = like land).
const SEA_ICE_CONTINENTAL: f64 = 0.8;
/// Fraction of the year at which the northern spring equinox falls (Earth: ~20 March).
const EQUINOX_FRAC: f64 = 79.5 / 365.25;

/// Maritime influence (0 continental … 1 open sea) and the current offset it
/// carries, for the half-year around January (`_a`), around July (`_b`) and
/// the year; `cbar` is the zonal mean continentality per band.
struct SeaAir {
    mar_a: Vec<f64>,
    cur_a: Vec<f64>,
    mar_b: Vec<f64>,
    cur_b: Vec<f64>,
    mar: Vec<f64>,
    cur: Vec<f64>,
    cbar: [f64; NB],
}

struct ClimateGrid {
    grid: Grid,
    transfer: GridTransfer,
}

/// Climate is solved again for lake feedback. These structures depend only on
/// the two grid levels, so retain the most recently used pair.
fn cached_climate_grid(fine: &Grid, level: u32) -> Option<Arc<ClimateGrid>> {
    if fine.level == level {
        return None;
    }
    type Entry = Option<((u32, u32), Arc<ClimateGrid>)>;
    static CACHE: OnceLock<Mutex<Entry>> = OnceLock::new();
    let key = (fine.level, level);
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached_key, value)) = &*guard {
        if *cached_key == key {
            return Some(value.clone());
        }
    }
    let grid = Grid::new(level);
    let transfer = GridTransfer::new(fine, &grid);
    let value = Arc::new(ClimateGrid { grid, transfer });
    *guard = Some((key, value.clone()));
    Some(value)
}

#[inline]
fn gauss(d: f64, c: f64, w: f64) -> f64 {
    let t = (d - c) / w;
    (-t * t).exp()
}
#[inline]
fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---------------------------------------------------------------- insolation

/// Solar longitude (radians) at a fraction of the year (0 = 1 January).
fn solar_longitude(year_frac: f64) -> f64 {
    TAU * (year_frac - EQUINOX_FRAC)
}

pub fn declination(p: &WorldParams, year_frac: f64) -> f64 {
    let tilt = p.planet.axial_tilt_deg.to_radians();
    (tilt.sin() * solar_longitude(year_frac).sin()).asin()
}

/// Daily-mean top-of-atmosphere insolation, W/m².
pub fn insolation(p: &WorldParams, lat: f64, year_frac: f64) -> f64 {
    let pl = &p.planet;
    let lam = solar_longitude(year_frac);
    let e = pl.eccentricity.clamp(0.0, 0.9);
    let nu = lam - pl.perihelion_deg.to_radians();
    let dist_factor = ((1.0 + e * nu.cos()) / (1.0 - e * e)).powi(2);
    let dec = declination(p, year_frac);
    let c = -lat.tan() * dec.tan();
    let h0 = if c >= 1.0 {
        0.0
    } else if c <= -1.0 {
        PI
    } else {
        c.acos()
    };
    pl.solar_constant / PI * dist_factor * (h0 * lat.sin() * dec.sin() + lat.cos() * dec.cos() * h0.sin())
}

// ---------------------------------------------------------------- EBM

/// Albedo with North's latitude dependence (clouds, low sun) and ice-albedo
/// feedback, for a band that is part sea, part land. The sea freezes by its
/// sea-level temperature; land ices over by the temperature at its mean
/// height, so a high continent holds an ice sheet where a low one would not.
/// Ice lasts through the summer where last year's warmest day stayed near
/// freezing (a cold but dry interior with warm summers has only winter snow;
/// land among seas has warmer summers than its band's mean).
/// Also returns the share of the band under snow or ice on land, and the
/// share of its land under ice that lasts the year (an ice sheet).
fn albedo(t: f64, x: f64, t_summer_prev: f64, land: f64, land_drop: f64) -> (f64, f64, f64) {
    let p2 = 0.5 * (3.0 * x * x - 1.0);
    let base = 1.0 - (0.697 - 0.0779 * p2);
    let snow = |t: f64| 1.0 - smoothstep(-10.0, -2.0, t);
    let sea = snow(t).max(0.85 * (1.0 - smoothstep(0.0, 4.0, t_summer_prev)));
    let lasting = 0.85 * (1.0 - smoothstep(0.0, 8.0, t_summer_prev + LAND_SUMMER_EXCESS_C * (1.0 - land) - land_drop));
    let sheet = snow(t - land_drop).max(lasting);
    (base + (1.0 - land) * (ALBEDO_SEA_ICE - base).max(0.0) * sea + land * (ALBEDO_ICE_SHEET - base).max(0.0) * sheet, land * sheet, lasting)
}

/// Sea-ice cover of open water at a sea-level temperature.
fn sea_ice(t: f64) -> f64 {
    1.0 - smoothstep(-10.0, -2.0, t)
}

/// What the energy-balance model knows of the surface: per band, the share of
/// land and how much colder the land's surface is than sea level (°C, from
/// its mean height).
struct Bands {
    land: [f64; NB],
    land_drop: [f64; NB],
    /// Ice sheets build their own height and drain heat (the real world);
    /// off for the all-land and all-sea reference worlds.
    ice_domes: bool,
}

/// Result of an EBM run: monthly band temperatures [12][NB].
struct Ebm {
    month: Vec<[f64; NB]>,
}

impl Ebm {
    fn annual(&self, k: usize) -> f64 {
        self.month.iter().map(|m| m[k]).sum::<f64>() / 12.0
    }
}

fn band_x(k: usize) -> f64 {
    -1.0 + (k as f64 + 0.5) * 2.0 / NB as f64
}

fn run_ebm(p: &WorldParams, bands: &Bands) -> Ebm {
    let pl = &p.planet;
    let days = pl.year_length_days.clamp(20.0, 5000.0).round() as usize;
    let dx = 2.0 / NB as f64;
    let rot = (pl.day_length_h / 24.0).clamp(0.05, 50.0);
    let d = p.climate.heat_diffusion * rot.powf(0.5).clamp(0.4, 3.0);
    let forcing = 30.0 * pl.greenhouse.max(0.05).ln() / 2f64.ln();
    let lat: Vec<f64> = (0..NB).map(|k| band_x(k).asin()).collect();
    // Edge diffusivities (1 - x²) at k+1/2, raised inside the Hadley cells and
    // lowered toward the poles.
    let tropic = (TROPIC_WIDTH * wind_belts(p).hadley).to_radians();
    let edge: Vec<f64> = (0..=NB).map(|k| {
        let x = -1.0 + k as f64 * dx;
        let lat = x.clamp(-1.0, 1.0).asin();
        let mix = 1.0 + TROPIC_MIX * (-(lat / tropic).powi(4)).exp();
        let polar = 1.0 - POLAR_DAMP * smoothstep(42f64.to_radians(), 80f64.to_radians(), lat.abs());
        (1.0 - x * x) * mix * polar
    }).collect();
    // Precompute insolation for each day and band.
    let q: Vec<[f64; NB]> = (0..days)
        .map(|day| {
            let f = (day as f64 + 0.5) / days as f64;
            std::array::from_fn(|k| insolation(p, lat[k], f))
        })
        .collect();

    let mut t = [12.0f64; NB];
    // Warmest day of the previous year and of this one so far.
    let mut t_summer_prev = [12.0f64; NB];
    let mut t_summer = [f64::MIN; NB];
    let years = 40;
    let mut month_sum = vec![[0.0f64; NB]; 12];
    let mut month_cnt = [0usize; 12];
    let (mut a, mut b, mut c, mut r) = ([0.0; NB], [0.0; NB], [0.0; NB], [0.0; NB]);
    // An ice sheet's cold reaches the seas around it (cold air draining off
    // it, sea ice carried away): its heat loss is spread over nearby
    // latitudes. Bands have equal area, so each column of weights sums to 1.
    let spread: Vec<[f64; NB]> = (0..NB)
        .map(|j| {
            let w: [f64; NB] = std::array::from_fn(|k| gauss(lat[k].to_degrees(), lat[j].to_degrees(), ICE_SHEET_REACH_DEG));
            let sum: f64 = w.iter().sum();
            w.map(|x| x / sum)
        })
        .collect();
    let has_sheet = bands.ice_domes && (0..NB).any(|k| bands.land[k] > 0.0);
    let mut sink = [0.0f64; NB];
    // How much colder than sea level each band's land surface is: its own
    // height, or the height of the ice sheet that has grown on it.
    let mut land_drop = bands.land_drop;
    let dome_drop = p.climate.lapse_rate_c_per_km * ICE_DOME_M / 1000.0;
    for year in 0..years {
        for day in 0..days {
            if has_sheet {
                sink = [0.0; NB];
                for j in 0..NB {
                    let (_, sheet, lasting) = albedo(t[j], band_x(j), t_summer_prev[j], bands.land[j], land_drop[j]);
                    if bands.land[j] > 0.0 {
                        // The ice thickens (and thins) over centuries; here, over a few model years.
                        let target = bands.land_drop[j].max(dome_drop * lasting);
                        land_drop[j] += (target - land_drop[j]) / (2.0 * days as f64);
                    }
                    let lost = ICE_SHEET_SINK * sheet * land_drop[j];
                    if lost > 0.0 {
                        for k in 0..NB {
                            sink[k] += lost * spread[j][k];
                        }
                    }
                }
                // However much ice there is, it can only drain so much heat
                // from the planet (bands have equal area).
                let total = sink.iter().sum::<f64>() / NB as f64;
                if total > ICE_SHEET_SINK_MAX {
                    sink.iter_mut().for_each(|x| *x *= ICE_SHEET_SINK_MAX / total);
                }
            }
            for k in 0..NB {
                let lf = bands.land[k];
                let cdt = (lf * C_LAND + (1.0 - lf) * (C_OCEAN + (C_ICE - C_OCEAN) * sea_ice(t[k]))) / DAY_S;
                let (alb, _, _) = albedo(t[k], band_x(k), t_summer_prev[k], lf, land_drop[k]);
                let asr = q[day][k] * (1.0 - alb) - sink[k];
                let kl = d * edge[k] / (dx * dx);
                let kr = d * edge[k + 1] / (dx * dx);
                a[k] = if k > 0 { -kl } else { 0.0 };
                c[k] = if k + 1 < NB { -kr } else { 0.0 };
                b[k] = cdt + B_OLR - a[k] - c[k];
                r[k] = cdt * t[k] + asr - A_OLR + forcing;
            }
            // Thomas algorithm.
            for k in 1..NB {
                let m = a[k] / b[k - 1];
                b[k] -= m * c[k - 1];
                r[k] -= m * r[k - 1];
            }
            t[NB - 1] = r[NB - 1] / b[NB - 1];
            for k in (0..NB - 1).rev() {
                t[k] = (r[k] - c[k] * t[k + 1]) / b[k];
            }
            for k in 0..NB {
                t_summer[k] = t_summer[k].max(t[k]);
            }
            if year == years - 1 {
                let m = ((day * 12) / days).min(11);
                for k in 0..NB {
                    month_sum[m][k] += t[k];
                }
                month_cnt[m] += 1;
            }
        }
        t_summer_prev = t_summer;
        t_summer = [f64::MIN; NB];
    }
    let month = (0..12)
        .map(|m| std::array::from_fn(|k| month_sum[m][k] / month_cnt[m].max(1) as f64))
        .collect();
    Ebm { month }
}

/// Linear interpolation of a band profile at a latitude.
fn band_interp(v: &[f64; NB], lat: f64) -> f64 {
    let x = lat.sin();
    let f = (x + 1.0) / 2.0 * NB as f64 - 0.5;
    let k0 = f.floor().clamp(0.0, (NB - 1) as f64) as usize;
    let k1 = (k0 + 1).min(NB - 1);
    let t = (f - k0 as f64).clamp(0.0, 1.0);
    v[k0] * (1.0 - t) + v[k1] * t
}

// ---------------------------------------------------------------- winds

pub struct WindBelts {
    pub hadley: f64,
    pub ferrel: f64,
}

pub fn wind_belts(p: &WorldParams) -> WindBelts {
    let rot = (p.planet.day_length_h / 24.0).clamp(0.05, 50.0);
    let hadley = (30.0 * rot.powf(0.35)).clamp(12.0, 50.0);
    let ferrel = (hadley + 30.0 * rot.powf(0.2)).clamp(hadley + 10.0, 82.0);
    WindBelts { hadley, ferrel }
}

/// Latitude of the ITCZ (degrees) in a month.
pub fn itcz_lat(p: &WorldParams, month: usize) -> f64 {
    // Lags the sun by about one month.
    let f = (month as f64 + 0.5 - 1.0) / 12.0;
    0.35 * declination(p, f).to_degrees()
}

/// Surface wind (east, north), unit-ish magnitude, at a latitude in a month,
/// for the zonal-mean belts (the convergence line at the same latitude all
/// round the planet).
pub fn wind_at(p: &WorldParams, lat_rad: f64, month: usize) -> (f64, f64) {
    let cz = itcz_lat(p, month);
    belt_wind(&wind_belts(p), lat_rad.to_degrees(), cz, cz)
}

/// Position inside the belts: degrees from the convergence line, stretched so
/// that the trade-wind belts always end at the Hadley edges (`cz` ± hadley)
/// wherever the local line `c` is; beyond the edges, degrees from `cz`.
/// Also the side (+1 north of the line, −1 south of it).
fn belt_coord(belts: &WindBelts, lat: f64, c: f64, cz: f64) -> (f64, f64) {
    let h = belts.hadley;
    let (north, south) = (cz + h, cz - h);
    if lat >= north || lat <= south {
        ((lat - cz).abs(), if lat >= cz { 1.0 } else { -1.0 })
    } else if lat >= c {
        ((lat - c) / (north - c).max(1.0) * h, 1.0)
    } else {
        ((c - lat) / (c - south).max(1.0) * h, -1.0)
    }
}

/// Belt wind (east, north) at a latitude (degrees) where the convergence line
/// lies at `c` degrees at this longitude and at `cz` in the zonal mean. Air
/// flowing toward the equator turns west (the trades); air that crosses the
/// equator toward a convergence line far in the summer hemisphere turns east
/// (the monsoon westerlies).
pub fn belt_wind(belts: &WindBelts, lat: f64, c: f64, cz: f64) -> (f64, f64) {
    let (h, f) = (belts.hadley, belts.ferrel);
    let (a, s) = belt_coord(belts, lat, c, cz);
    // The westerlies blow harder in the winter hemisphere (the side the
    // convergence line has moved away from), where the temperature contrast
    // between tropics and pole is largest.
    let winter = (-s * cz / 8.0).clamp(-1.0, 1.0);
    if a < h {
        // A trade-wind belt squeezed between a displaced line and the Hadley
        // edge is weak in proportion, and leaves the field to the thermal winds.
        let (north, south) = (cz + h, cz - h);
        let room = if lat >= c { (north - c) / h } else { (c - south) / h };
        let w = (PI * a / h).sin() * room.clamp(0.0, 1.0);
        let v = -s * 0.45 * w;
        // +1 when moving toward the equator, −1 when moving away from it.
        let toward = (s * lat / 6.0).tanh();
        (-w * if toward >= 0.0 { toward } else { 0.8 * toward }, v)
    } else if a < f {
        let w = (PI * (a - h) / (f - h)).sin() * (1.0 + WESTERLY_SEASON * winter);
        (w, s * 0.3 * w)
    } else {
        let span = (90.0 + 10.0 - f).max(5.0);
        let w = (PI * ((a - f) / span).min(1.0)).sin();
        (-0.55 * w, -s * 0.25 * w)
    }
}

/// How far the rain band follows the convergence line away from its zonal-mean latitude.
const RAIN_BAND_FOLLOW: f64 = 0.3;
/// Share by which the westerlies strengthen in midwinter and slacken in midsummer.
const WESTERLY_SEASON: f64 = 0.3;
/// How far from the equator the convergence line can sit, as degrees short of the Hadley edge.
const ITCZ_EDGE_GAP: f64 = 5.0;

/// Latitude (degrees) of the tropical convergence line at every cell, for each
/// month. It follows the heat: over a continent that heats up in summer it
/// moves far into the summer hemisphere and the winds on its equator side
/// turn onshore (a monsoon); over the open ocean it stays near the equator.
/// `follow` = 0 keeps the zonal-mean line everywhere.
fn local_itcz(p: &WorldParams, g: &Grid, t_sl: &[Vec<f32>], follow: f64) -> Vec<Vec<f32>> {
    const NS: usize = 72;
    let belts = wind_belts(p);
    let limit = (belts.hadley + 5.0).min(45.0);
    let reach = (belts.hadley - ITCZ_EDGE_GAP).max(2.0);
    let sector = |lon: f64| (((lon + PI) / TAU * NS as f64) as usize).min(NS - 1);
    // Soft arg-max of temperature over the tropics of each longitude sector.
    let raw: Vec<[f64; NS]> = (0..12)
        .map(|m| {
            let mut tmax = [f64::MIN; NS];
            for i in 0..g.len() {
                if g.lat[i].to_degrees().abs() <= limit {
                    let k = sector(g.lon[i]);
                    tmax[k] = tmax[k].max(t_sl[m][i] as f64);
                }
            }
            let (mut sw, mut sl) = ([0.0f64; NS], [0.0f64; NS]);
            for i in 0..g.len() {
                let lat = g.lat[i].to_degrees();
                if lat.abs() <= limit {
                    let k = sector(g.lon[i]);
                    let w = g.area[i] * ((t_sl[m][i] as f64 - tmax[k]) / 1.5).exp();
                    sw[k] += w;
                    sl[k] += w * lat;
                }
            }
            std::array::from_fn(|k| if sw[k] > 0.0 { sl[k] / sw[k] } else { 0.0 })
        })
        .collect();
    (0..12)
        .map(|m| {
            let cz = itcz_lat(p, m);
            // Smooth over neighbouring months and sectors, then hold inside the belts.
            let line: [f64; NS] = std::array::from_fn(|k| {
                let mut sum = 0.0;
                for (dm, wm) in [(11usize, 1.0), (0, 2.0), (1, 1.0)] {
                    for (dk, wk) in [(NS - 2, 1.0), (NS - 1, 2.0), (0, 3.0), (1, 2.0), (2, 1.0)] {
                        sum += wm * wk * raw[(m + dm) % 12][(k + dk) % NS];
                    }
                }
                let c = cz + follow.clamp(0.0, 1.0) * (sum / 36.0 - cz);
                c.clamp((-reach).max(cz - reach), reach.min(cz + reach))
            });
            (0..g.len())
                .map(|i| {
                    let f = (g.lon[i] + PI) / TAU * NS as f64 - 0.5;
                    let k0 = f.floor();
                    let t = f - k0;
                    let k0 = (k0 as i64).rem_euclid(NS as i64) as usize;
                    (line[k0] * (1.0 - t) + line[(k0 + 1) % NS] * t) as f32
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------- main

#[inline]
fn qsat(t: f64) -> f64 {
    (0.07 * t.clamp(-60.0, 45.0)).exp()
}

pub fn run(ctx: &Ctx) -> StepOutput {
    let elev = ctx.input.f32("elevation");
    // First pass: lakes are not known yet, so only cells at or below sea level are water.
    // Hydrology re-solves the climate with its lakes as open water (see hydrology.rs).
    let water: Vec<u8> = elev.iter().map(|&e| (e <= 0.0) as u8).collect();
    solve(ctx, elev, &water)
}

/// Solve the climate for a surface. `elev_f` is the surface height (the water
/// level on lakes); `water_f` marks each cell 0 = land, 1 = ocean, 2 = lake.
/// Lakes evaporate like open water and moderate temperatures, in proportion to
/// the share of each climate cell they cover; currents and sea-ice limits apply
/// to the ocean only.
pub fn solve(ctx: &Ctx, elev_f: &[f32], water_f: &[u8]) -> StepOutput {
    let p = ctx.params;
    let cp = &p.climate;
    let fine = ctx.grid;
    let r_km = p.planet.radius_km;

    // ---- coarse grid
    ctx.progress(0.02, "Preparing climate grid");
    let cl = p.climate_level();
    let cached = cached_climate_grid(fine, cl);
    let coarse: &Grid = cached.as_ref().map_or(fine, |c| &c.grid);
    let transfer: Option<&GridTransfer> = cached.as_ref().map(|c| &c.transfer);
    let nc = coarse.len();
    let elev_c: Vec<f32> = match transfer {
        Some(t) => t.down(fine, nc, elev_f),
        None => elev_f.to_vec(),
    };
    let frac = |pred: &dyn Fn(u8) -> bool| -> Vec<f64> {
        let v: Vec<f32> = water_f.iter().map(|&w| pred(w) as u8 as f32).collect();
        let c = match transfer {
            Some(t) => t.down(fine, nc, &v),
            None => v,
        };
        c.into_iter().map(|x| x as f64).collect()
    };
    // Share of each climate cell that is open water (ocean or lake), and ocean only.
    let water_frac = frac(&|w| w > 0);
    let sea_frac = frac(&|w| w == 1);
    let lake_frac: Vec<f64> = (0..nc).map(|i| (water_frac[i] - sea_frac[i]).max(0.0)).collect();
    // `ocean_c`: the cell is sea (for currents, sea-ice limit and the wind fetch).
    let ocean_c: Vec<bool> = sea_frac.iter().map(|&x| x > 0.5).collect();
    let spacing_km = coarse.spacing * r_km;

    // ---- energy balance
    ctx.progress(0.08, "Solving energy balance");
    let lapse = cp.lapse_rate_c_per_km / 1000.0;
    let mut land_frac = [0.0f64; NB];
    let mut land_h = [0.0f64; NB];
    let mut band_area = [0.0f64; NB];
    for i in 0..nc {
        let k = (((coarse.lat[i].sin() + 1.0) / 2.0 * NB as f64) as usize).min(NB - 1);
        let land = coarse.area[i] * (1.0 - water_frac[i]);
        band_area[k] += coarse.area[i];
        land_frac[k] += land;
        land_h[k] += land * elev_c[i].max(0.0) as f64;
    }
    let mut world = Bands { land: [0.0; NB], land_drop: [0.0; NB], ice_domes: true };
    for k in 0..NB {
        world.land[k] = if band_area[k] > 0.0 { land_frac[k] / band_area[k] } else { 0.0 };
        world.land_drop[k] = if land_frac[k] > 0.0 { lapse * land_h[k] / land_frac[k] } else { 0.0 };
    }
    // The world as it is (annual means), and all-sea and all-land worlds (for
    // the seasonal swing of maritime and continental places).
    let cases = [world, Bands { land: [0.0; NB], land_drop: [0.0; NB], ice_domes: false }, Bands { land: [1.0; NB], land_drop: [0.0; NB], ice_domes: false }];
    let runs: Vec<Ebm> = cases.par_iter().map(|c| run_ebm(p, c)).collect();
    let (mix, ocean_run, land_run) = (&runs[0], &runs[1], &runs[2]);
    let mix_ann: [f64; NB] = std::array::from_fn(|k| mix.annual(k));
    let anom = |run: &Ebm, m: usize| -> [f64; NB] {
        std::array::from_fn(|k| run.month[m][k] - run.annual(k))
    };
    let anom_o: Vec<[f64; NB]> = (0..12).map(|m| anom(ocean_run, m)).collect();
    let anom_l: Vec<[f64; NB]> = (0..12).map(|m| anom(land_run, m)).collect();

    // ---- base winds: zonal three-cell belts (unitless; 1 ≈ W_REF m/s)
    ctx.progress(0.2, "Blowing winds");
    let belts = wind_belts(p);
    let belt_winds = |line: Option<&Vec<Vec<f32>>>| -> Vec<Vec<Vec3>> {
        (0..12)
            .into_par_iter()
            .map(|m| {
                let cz = itcz_lat(p, m);
                (0..nc)
                    .map(|i| {
                        let (u, v) = belt_wind(&belts, coarse.lat[i].to_degrees(), line.map_or(cz, |l| l[m][i] as f64), cz);
                        let (e, n) = coarse.pos[i].east_north();
                        e * u + n * v
                    })
                    .collect()
            })
            .collect()
    };
    let wind_zonal = belt_winds(None);

    // ---- coastal currents (ocean cells)
    ctx.progress(0.25, "Routing ocean currents");
    let current_c: Vec<f64> = (0..nc)
        .into_par_iter()
        .map(|i| {
            if !ocean_c[i] {
                return 0.0;
            }
            let lat = coarse.lat[i];
            let coslat = lat.cos().max(0.05);
            let step = coarse.spacing / coslat;
            let max_steps = ((4000.0 / r_km) / coarse.spacing).ceil() as usize;
            let march = |dir: f64| -> f64 {
                let mut hint = i;
                for k in 1..=max_steps {
                    let q = Vec3::from_lat_lon(lat, coarse.lon[i] + dir * step * k as f64);
                    let c = coarse.nearest(q, Some(hint));
                    hint = c;
                    if !ocean_c[c] {
                        return k as f64 * spacing_km;
                    }
                }
                f64::INFINITY
            };
            let de = march(1.0);
            let dw = march(-1.0);
            if !de.is_finite() && !dw.is_finite() {
                return 0.0;
            }
            let (de2, dw2) = (de.min(6000.0), dw.min(6000.0));
            let s = (dw2 - de2) / (dw2 + de2);
            // Cold eastern-boundary currents hug the subtropical coast; the warm
            // drift of the high latitudes spreads over much of the basin.
            let near = de.min(dw);
            let a = lat.to_degrees().abs();
            // The upwelling cold side is stronger than the warm side across the basin.
            let subtropical = -gauss(a, 24.0, 16.0) * (-near / 1200.0).exp() * if s > 0.0 { 1.4 } else { 0.9 };
            let subpolar = 1.4 * gauss(a, 58.0, 12.0) * (-near / 2500.0).exp();
            cp.ocean_current_c * s * (subtropical + subpolar)
        })
        .collect();

    // ---- maritime influence along the wind, then sea-level temperatures.
    // Run twice: with the belt winds, then with the thermally driven winds.
    let h_c: Vec<f64> = elev_c.iter().map(|&e| e.max(0.0) as f64).collect();
    // Maritime air is shallow and stays on the windward side of a range: the air
    // that comes down the far side has lost part of it for each metre it descends,
    // so the land behind a range is more continental than the coast before it.
    // Windward slopes and summits keep it.
    let block_k = cp.sea_air_blocking.max(0.0) * std::f64::consts::LN_2 / 2000.0;
    // How open the sea is around each cell (0 in a lake or narrow sea, 1 well
    // offshore): small waters moderate the air much less than an ocean.
    let open_sea: Vec<f64> = {
        let passes = (OPEN_SEA_KM / spacing_km).powi(2).ceil().clamp(2.0, 60.0) as usize;
        let mut v = sea_frac.clone();
        for _ in 0..passes {
            v = smooth(coarse, &v);
        }
        v.into_iter().map(|x| smoothstep(0.45, 0.9, x)).collect()
    };
    // Sea ice: how much of the year each sea cell is frozen over, from the
    // open-water temperature. Ice-covered sea is no source of mild air, and
    // the air over it swings almost as on land.
    let ice_c: Vec<f64> = (0..nc)
        .map(|i| {
            if !ocean_c[i] {
                return 0.0;
            }
            let lat = coarse.lat[i];
            let ann = band_interp(&mix_ann, lat) + current_c[i];
            (0..12).map(|m| 1.0 - smoothstep(SEA_ICE_FULL_C, SEA_FREEZE_C, ann + band_interp(&anom_o[m], lat))).sum::<f64>() / 12.0
        })
        .collect();
    // Unit direction from each neighbour toward the cell, aligned with the
    // grid's neighbour lists (used by everything the wind carries).
    let (nbr_off, nbr_idx) = coarse.neighbor_csr();
    let (_, nbr_rev) = coarse.neighbor_geometry();
    let dir_in: Vec<Vec3> = (0..nc)
        .flat_map(|i| (nbr_off[i] as usize..nbr_off[i + 1] as usize).map(move |e| (i, e)))
        .map(|(i, e)| (coarse.pos[i] - coarse.pos[nbr_idx[e] as usize]).normalized())
        .collect();
    // Cell boundaries: for each edge, the vector across it into the cell (its
    // length is the length of the boundary between the two cell corners), and
    // a mixing weight that is the same from both sides.
    let (nbr_len, _) = coarse.neighbor_geometry();
    let edge_in: Vec<Vec3> = {
        let mut corners = vec![[u32::MAX; 2]; nbr_idx.len()];
        for (t, tri) in coarse.tris.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (tri[k] as usize, tri[(k + 1) % 3] as usize);
                for e in [coarse.edge_index(a, b), coarse.edge_index(b, a)].into_iter().flatten() {
                    let slot = if corners[e][0] == u32::MAX { 0 } else { 1 };
                    corners[e][slot] = t as u32;
                }
            }
        }
        (0..nc)
            .flat_map(|i| (nbr_off[i] as usize..nbr_off[i + 1] as usize).map(move |e| (i, e)))
            .map(|(i, e)| {
                let [c1, c2] = corners[e];
                if c2 == u32::MAX {
                    return Vec3::ZERO;
                }
                let side = coarse.corner_pos(c2 as usize) - coarse.corner_pos(c1 as usize);
                let n = side.cross(coarse.pos[i]);
                if n.dot(dir_in[e]) >= 0.0 { n } else { n * -1.0 }
            })
            .collect()
    };
    let edge_mix: Vec<f64> = (0..nbr_idx.len()).map(|e| 0.25 * coarse.spacing * coarse.spacing * edge_in[e].len() / nbr_len[e].max(1e-12)).collect();
    // Time step of the moisture transport: a wind of `W_REF` moves
    // TRANSPORT_STEP of a typical cell's water on in one pass.
    let step = {
        let mean: f64 = (0..nc)
            .map(|i| {
                let (east, _) = coarse.pos[i].east_north();
                (nbr_off[i] as usize..nbr_off[i + 1] as usize).map(|e| (-east.dot(edge_in[e])).max(0.0)).sum::<f64>() / coarse.area[i]
            })
            .sum::<f64>()
            / nc as f64;
        TRANSPORT_STEP / mean
    };
    // A coarser grid of the same family, on which travelling storms spread
    // moisture over hundreds of km at a time.
    let storm_grid = Grid::new(cl.saturating_sub(2).max(3));
    let storm_xfer = GridTransfer::new(coarse, &storm_grid);
    let wide_area: Vec<f64> = {
        let mut a = vec![0.0f64; storm_grid.len()];
        for i in 0..nc {
            a[storm_xfer.nearest[i] as usize] += coarse.area[i];
        }
        a
    };
    // Maritime air (and the offset of the current it crossed) carried inland by one wind field.
    let maritime = |wind: &[Vec3]| -> (Vec<f64>, Vec<f64>) {
        let reach = cp.maritime_reach_km.max(50.0);
        let decay_adv = (-spacing_km / reach).exp();
        let decay_iso = (-spacing_km / (reach * 0.22)).exp();
        // Lakes are weaker maritime sources than the ocean (smaller, freeze, no currents).
        let source: Vec<f64> = (0..nc).map(|i| if ocean_c[i] { open_sea[i] * (1.0 - SEA_ICE_CONTINENTAL * ice_c[i]) } else { 0.6 * lake_frac[i] }).collect();
        // Per edge: how much of the neighbour's air the wind brings, and how
        // much of its maritime air survives the descent from it.
        let inflow: Vec<f64> = (0..nbr_idx.len()).into_par_iter().map(|e| wind[nbr_idx[e] as usize].dot(dir_in[e]).max(0.0)).collect();
        let block: Vec<f64> = (0..nc)
            .flat_map(|i| (nbr_off[i] as usize..nbr_off[i + 1] as usize).map(move |e| (i, e)))
            .map(|(i, e)| (-block_k * (h_c[nbr_idx[e] as usize] - h_c[i]).max(0.0)).exp())
            .collect();
        let mut mar = source.clone();
        // Maritime air that reaches a place from any side (sea breezes, passing
        // storms): fades with distance from the sea alone, whatever the wind.
        let mut near_sea = source.clone();
        let mut cur: Vec<f64> = current_c.clone();
        let passes = ((reach * 2.5) / spacing_km).ceil() as usize + 4;
        for _ in 0..passes {
            let next: Vec<(f64, f64, f64)> = (0..nc)
                .into_par_iter()
                .map(|i| {
                    let (mut aw, mut am, mut ac) = (0.0, 0.0, 0.0);
                    let mut iso: f64 = 0.0;
                    let mut iso_c = 0.0;
                    for e in nbr_off[i] as usize..nbr_off[i + 1] as usize {
                        let j = nbr_idx[e] as usize;
                        let a = inflow[e];
                        let mj = mar[j] * block[e];
                        aw += a;
                        am += a * mj;
                        ac += a * mj * cur[j];
                        let nj = near_sea[j] * block[e];
                        if nj > iso {
                            iso = nj;
                            iso_c = cur[j];
                        }
                    }
                    let adv = if aw > 0.0 { am / aw } else { 0.0 };
                    let adv_c = if am > 0.0 { ac / am } else { 0.0 };
                    // Mostly what the wind brings, so a coast the wind blows off
                    // is still continental.
                    let va = adv * decay_adv;
                    let vi = (iso * decay_iso).max(source[i]);
                    let m = (va + (1.0 - va) * MARITIME_ANY_SIDE * vi).max(source[i]);
                    if ocean_c[i] {
                        (m, vi, current_c[i])
                    } else if va >= MARITIME_ANY_SIDE * vi {
                        (m, vi, adv_c)
                    } else {
                        (m, vi, iso_c)
                    }
                })
                .collect();
            for (i, (m, v, c)) in next.into_iter().enumerate() {
                mar[i] = m;
                near_sea[i] = v;
                cur[i] = c;
            }
        }
        (mar, cur)
    };
    // Where the wind turns with the seasons (monsoon coasts), a place can be
    // continental in one half of the year and maritime in the other: one field
    // for the months around January, one for those around July.
    let sea_air = |winds: &[Vec<Vec3>]| -> SeaAir {
        let half_year = |months: [usize; 6]| -> Vec<Vec3> { (0..nc).map(|i| months.iter().fold(Vec3::ZERO, |s, &m| s + winds[m][i]) * (1.0 / 6.0)).collect() };
        let ((mar_a, cur_a), (mar_b, cur_b)) = rayon::join(|| maritime(&half_year([10, 11, 0, 1, 2, 3])), || maritime(&half_year([4, 5, 6, 7, 8, 9])));
        let mar: Vec<f64> = (0..nc).map(|i| 0.5 * (mar_a[i] + mar_b[i])).collect();
        let cur: Vec<f64> = (0..nc).map(|i| if mar[i] > 0.0 { 0.5 * (cur_a[i] * mar_a[i] + cur_b[i] * mar_b[i]) / mar[i] } else { 0.0 }).collect();
        // Zonal mean continentality: at high latitudes sea-facing places are
        // warmer over the year than the mean of their latitude, and interiors
        // colder (deep winter cold under snow outweighs the summer warmth).
        let mut cs = [0.0f64; NB];
        let mut cw = [0.0f64; NB];
        for i in 0..nc {
            let k = (((coarse.lat[i].sin() + 1.0) / 2.0 * NB as f64) as usize).min(NB - 1);
            cs[k] += coarse.area[i] * (1.0 - mar[i]);
            cw[k] += coarse.area[i];
        }
        let cbar_raw: [f64; NB] = std::array::from_fn(|k| if cw[k] > 0.0 { cs[k] / cw[k] } else { 0.0 });
        let cbar: [f64; NB] = std::array::from_fn(|k| {
            let (a, b) = (k.saturating_sub(3), (k + 3).min(NB - 1));
            cbar_raw[a..=b].iter().sum::<f64>() / (b - a + 1) as f64
        });
        SeaAir { mar_a, cur_a, mar_b, cur_b, mar, cur, cbar }
    };
    // Largest seasonal swing of the continental profile, per band.
    let swing_l: [f64; NB] = std::array::from_fn(|k| anom_l.iter().map(|a| a[k].abs()).fold(0.0, f64::max));
    // Sea-level temperatures: each cell's own balance (with the heat of dry
    // ground, if known), then carried by the wind (if given).
    let temperatures = |sea: &SeaAir, winds: Option<&[Vec<Vec3>]>, dry: Option<&[Vec<f32>]>| -> Vec<Vec<f32>> {
        (0..12)
            .into_par_iter()
            .map(|m| {
                let local: Vec<f64> = (0..nc)
                    .map(|i| {
                        let lat = coarse.lat[i];
                        let wa = 0.5 + 0.5 * (TAU * (m as f64 - 0.5) / 12.0).cos();
                        let mar_m = wa * sea.mar_a[i] + (1.0 - wa) * sea.mar_b[i];
                        let cont = 1.0 - mar_m;
                        // In summer land heats the air over it whatever the wind
                        // brings; in winter sea air stays mild far inland.
                        let (ao, al) = (band_interp(&anom_o[m], lat), band_interp(&anom_l[m], lat));
                        let land = 1.0 - water_frac[i];
                        // Even a land-locked sea swings far less than land.
                        let cont = cont.min(1.0 - (1.0 - SEA_SWING_MAX) * water_frac[i]);
                        let cont_m = cont + (1.0 - cont) * LAND_SUMMER * land * smoothstep(-3.0, 3.0, al);
                        let a = ao * (1.0 - cont_m) + al * cont_m;
                        let off = if ocean_c[i] { current_c[i] } else { wa * sea.cur_a[i] * sea.mar_a[i] + (1.0 - wa) * sea.cur_b[i] * sea.mar_b[i] };
                        let cooling = cp.continental_cooling * smoothstep(30.0, 62.0, lat.to_degrees().abs());
                        // Dry ground cannot cool by evaporation: hotter, most of all in summer.
                        let heat = dry.map_or(0.0, |d| {
                            let summer = (0.5 + 0.5 * al / (band_interp(&swing_l, lat) + 1.0)).clamp(0.0, 1.0);
                            let dry = d[m][i] as f64;
                            // (Not in the cold: frozen or snow-covered ground does neither.)
                            let thawed = smoothstep(2.0, 14.0, band_interp(&mix_ann, lat) + a);
                            cp.dry_heat * land * thawed * (dry * (DRY_HEAT_MEAN_C + DRY_HEAT_SWING_C * (2.0 * summer - 1.0)) - (1.0 - dry) * WET_LAND_COOL_C)
                        });
                        band_interp(&mix_ann, lat) + cooling * (band_interp(&sea.cbar, lat) - (1.0 - sea.mar[i])) + a + off + heat
                    })
                    .collect();
                // The wind carries warmth and cold with it.
                let carried = match winds {
                    Some(w) => advect_temperature(coarse, &w[m], &dir_in, &local, &water_frac, spacing_km),
                    None => local,
                };
                (0..nc)
                    .map(|i| {
                        let mut t = carried[i];
                        if ocean_c[i] {
                            // Open water does not cool below freezing; ice lets the
                            // air above it get colder, and holds summers near 0 °C.
                            let ice = ice_c[i];
                            t = t.max(SEA_FREEZE_C - 60.0 * ice).min(SEA_FREEZE_C + 2.0 + 30.0 * (1.0 - ice).powi(2));
                        }
                        t as f32
                    })
                    .collect()
            })
            .collect()
    };
    ctx.progress(0.3, "Spreading maritime air");
    // First guess: zonal belts, no carrying.
    let sea0 = sea_air(&wind_zonal);
    let t0 = temperatures(&sea0, None, None);

    // ---- height: broad highlands warm the air above them, so they are less
    // cold than a peak of the same height; ice sheets are colder (the air
    // over them is thin and dry, and cools from the ice up).
    let lapse_plateau = cp.plateau_lapse_c_per_km.min(cp.lapse_rate_c_per_km) / 1000.0;
    let broad_c: Vec<f64> = {
        let passes = (BROAD_HEIGHT_KM / spacing_km).powi(2).ceil().clamp(2.0, 40.0) as usize;
        let mut v = h_c.clone();
        for _ in 0..passes {
            v = smooth(coarse, &v);
        }
        v
    };
    let sheet_c: Vec<f64> = (0..nc)
        .map(|i| {
            let warmest = (0..12).map(|m| t0[m][i] as f64).fold(f64::MIN, f64::max) - lapse_plateau * broad_c[i] - lapse * (h_c[i] - broad_c[i]);
            (1.0 - water_frac[i]) * (1.0 - smoothstep(-3.0, 3.0, warmest))
        })
        .collect();
    // An ice sheet's surface is the ice, not the ground under it: thick in
    // its interior, thin at its edge.
    let dome_c: Vec<f64> = {
        let passes = (ICE_DOME_EDGE_KM / spacing_km).powi(2).ceil().clamp(2.0, 60.0) as usize;
        let mut v = sheet_c.clone();
        for _ in 0..passes {
            v = smooth(coarse, &v);
        }
        (0..nc).map(|i| ICE_DOME_M * sheet_c[i] * smoothstep(0.35, 0.95, v[i])).collect()
    };
    // °C that the broad surface is colder than sea level.
    let broad_drop_c: Vec<f64> = (0..nc).map(|i| (lapse_plateau + (ICE_SHEET_LAPSE / 1000.0 - lapse_plateau).max(0.0) * sheet_c[i]) * broad_c[i].max(dome_c[i])).collect();
    let drop_c: Vec<f64> = (0..nc).map(|i| broad_drop_c[i] + lapse * (h_c[i] - broad_c[i])).collect();

    // ---- winds: the convergence line follows the heat, the belts follow the
    // line, and thermal pressure systems add to them.
    ctx.progress(0.34, "Building pressure systems");
    let grad = super::wind::Gradient::new(coarse, r_km * 1000.0);
    let winds_for = |t_sl: &[Vec<f32>]| -> (Vec<Vec<f32>>, Vec<Vec<Vec3>>, Vec<Vec<Vec3>>) {
        let line = local_itcz(p, coarse, t_sl, cp.monsoon);
        let ms = super::wind::dynamic_winds(p, coarse, &grad, &belt_winds(Some(&line)), t_sl, &h_c, &water_frac);
        let unit = ms.iter().map(|w| w.iter().map(|v| *v * (1.0 / super::wind::W_REF)).collect()).collect();
        (line, ms, unit)
    };

    // ---- moisture and precipitation (coarse, months in parallel): raw rain
    // of the listed months.
    let moisture = |wind_m: &[Vec<Vec3>], wind_ms: &[Vec<Vec3>], t_sl: &[Vec<f32>], t_air: &[Vec<f32>], line: &[Vec<f32>], sea: &SeaAir, soil_water: Option<&[Vec<f64>]>, dry: Option<&[Vec<f32>]>, months: &[usize], passes: usize| -> Vec<Vec<f64>> {
        months
            .par_iter()
            .map(|&m| {
                let wind = &wind_m[m];
                let (mar, cur) = (&sea.mar, &sea.cur);
                let cz = itcz_lat(p, m);
                // Low-level convergence (1/s) lifts air and makes rain; divergence suppresses it.
                let conv: Vec<f64> = (0..nc).map(|i| -grad.divergence(coarse, i, &wind_ms[m])).collect();
                let temp: Vec<f64> = (0..nc).map(|i| t_sl[m][i] as f64 - drop_c[i]).collect();
                // Share of each cell's water that leaves through each of its
                // edges in one pass: the wind's flux through the edge (taken at
                // the edge, so what one cell sends its neighbour receives),
                // plus mixing by storms and eddies, strongest in the storm belts.
                let mix: Vec<f64> = (0..nc)
                    .map(|i| {
                        let (a, _) = belt_coord(&belts, coarse.lat[i].to_degrees(), line[m][i] as f64, cz);
                        EDDY_MIX + STORM_MIX * gauss(a, belts.ferrel - 10.0, 15.0)
                    })
                    .collect();
                // Vapour travels a little above the surface, where even a light
                // wind keeps the air moving.
                let carry: Vec<Vec3> = wind.iter().map(|w| *w * ((CARRY_FLOOR + (1.0 - CARRY_FLOOR) * w.len()) / w.len().max(0.05))).collect();
                let storms: Vec<f64> = (0..nc)
                    .map(|i| {
                        let (a, _) = belt_coord(&belts, coarse.lat[i].to_degrees(), line[m][i] as f64, cz);
                        STORM_SPREAD * gauss(a, belts.ferrel - 14.0, 24.0)
                    })
                    .collect();
                let mut out_w = vec![0.0f64; nbr_idx.len()];
                let mut stay = vec![0.0; nc];
                for j in 0..nc {
                    let range = nbr_off[j] as usize..nbr_off[j + 1] as usize;
                    let mut sent = 0.0;
                    for e in range.clone() {
                        let i = nbr_idx[e] as usize;
                        // `edge_in[e]` points into j, so outflow is against it.
                        let flux = (-(carry[j] + carry[i]).dot(edge_in[e]) * 0.5).max(0.0) * step / coarse.area[j];
                        let eddy = 0.5 * (mix[i] + mix[j]) * edge_mix[e] / coarse.area[j];
                        out_w[e] = flux + eddy;
                        sent += out_w[e];
                    }
                    if sent > 0.85 {
                        for e in range {
                            out_w[e] *= 0.85 / sent;
                        }
                        sent = 0.85;
                    }
                    stay[j] = 1.0 - sent;
                }
                // What arrives from each neighbour, per unit of what it holds:
                // cells differ in area, and water is carried, not concentration.
                let in_w: Vec<f64> = (0..nc)
                    .flat_map(|i| (nbr_off[i] as usize..nbr_off[i + 1] as usize).map(move |e| (i, e)))
                    .map(|(i, e)| out_w[nbr_rev[e] as usize] * coarse.area[nbr_idx[e] as usize] / coarse.area[i])
                    .collect();
                // Per cell: how much vapour the air can hold, how readily it
                // rains, how fast open water moistens it and how much of the
                // rain the land gives back.
                let mut rain_rate = vec![0.0; nc];
                let mut sat = vec![0.0; nc];
                let mut evap = vec![0.0; nc];
                let mut recycle = vec![0.0; nc];
                let mut soil = vec![0.0; nc];
                let mut showery = vec![0.0; nc];
                let mut lift = vec![0.0; nc];
                let mut dissolve = vec![0.0; nc];
                for i in 0..nc {
                    let mut up = 0.0;
                    let mut down = 0.0;
                    let mut wsum = 0.0;
                    for e in nbr_off[i] as usize..nbr_off[i + 1] as usize {
                        let j = nbr_idx[e] as usize;
                        let w = out_w[nbr_rev[e] as usize];
                        up += w * (h_c[i] - h_c[j]).max(0.0);
                        down += w * (h_c[j] - h_c[i]).max(0.0);
                        wsum += w;
                    }
                    if wsum > 0.0 {
                        up /= wsum;
                        down /= wsum;
                    }
                    let lat = coarse.lat[i].to_degrees();
                    let line = line[m][i] as f64;
                    let (a, _) = belt_coord(&belts, lat, line, cz);
                    // Rising air along the convergence line and in the storm
                    // belts, sinking air under the subtropical highs.
                    // The rain band lies on the equator side of a line the
                    // heat has drawn far over a continent (hot, dry air on the
                    // far side of it caps the rain there).
                    let band = cz + RAIN_BAND_FOLLOW * (line - cz);
                    let mut lm = 1.0 + 1.2 * gauss(lat - band, 0.0, 7.0);
                    lm *= 1.0 + 0.6 * (conv[i] / 3.0e-6).clamp(-0.7, 2.0 - water_frac[i]);
                    // Under the subtropical highs air sinks where it flows toward
                    // the equator (their east side: dry west coasts) and rises
                    // where it flows poleward (their west side: wet east coasts).
                    let (_, north) = coarse.pos[i].east_north();
                    let poleward = wind[i].dot(north) * lat.signum();
                    // Warm ground heats the air from below and breaks that
                    // cap: there the air's own humidity decides (cool sea air
                    // over hot land is far from saturated: dry summers on
                    // west coasts; warm sea air is not: wet ones on east coasts).
                    let uncapped = LAND_BREAKS_CAP * (1.0 - water_frac[i]) * smoothstep(12.0, 24.0, temp[i]);
                    lm *= 1.0 - 0.8 * gauss(a, belts.hadley, 9.0) * (0.6 - 0.8 * poleward).clamp(0.1, 1.0) * (1.0 - uncapped);
                    // The trade-wind belts between the rain band and the highs
                    // are capped by sinking air too.
                    if a < belts.hadley {
                        lm *= 1.0 - TRADE_CAP * smoothstep(6.0, 16.0, (lat - band).abs()) * (1.0 - uncapped);
                    }
                    lm *= 1.0 + STORM_RAIN * gauss(a, belts.ferrel - 10.0, 15.0);
                    // Cold air is wrung out by fronts however little it holds.
                    lm *= 1.0 + 1.0 * smoothstep(-10.0, 10.0, -temp[i]);
                    let cold_coast = (1.0 + 0.2 * cur[i].min(0.0) * mar[i]).clamp(0.15, 1.0);
                    // Air forced up a slope rains more readily; air coming down
                    // the far side hardly at all.
                    let slope = 1.0 + cp.orographic.max(0.0) * OROGRAPHIC_GAIN * (1.0 - (-up / OROGRAPHIC_LIFT_M).exp());
                    // And part of the vapour it brings condenses on the climb
                    // into cloud, which rains out over the slope and the crest.
                    lift[i] = 1.0 - (-cp.orographic.max(0.0) * up / OROGRAPHIC_CLOUD_M).exp();
                    dissolve[i] = 1.0 - (-down / CLOUD_DRY_M).exp();
                    let shadow = (-down / 900.0).exp();
                    // Summer convection over warm land.
                    let wf = water_frac[i];
                    let convect = 1.0 + 0.8 * (1.0 - wf) * smoothstep(8.0, 28.0, temp[i]);
                    // Over parched ground the air is dry far up: showers
                    // evaporate before they land.
                    let parched = 1.0 - DRY_GROUND_CAP * dry.map_or(0.0, |d| d[m][i] as f64) * (1.0 - wf);
                    rain_rate[i] = (RAIN_RATE * lm * slope * shadow * cold_coast * convect * parched).clamp(0.0005, 0.5);
                    // The air column's capacity follows the air above, not
                    // the heat of dry ground below it.
                    sat[i] = qsat(t_air[m][i] as f64 - drop_c[i]);
                    // Warm air rains in showers at any humidity; cold air only
                    // from layered cloud, once it is nearly saturated.
                    showery[i] = smoothstep(2.0, 20.0, temp[i]);
                    // Open water (ocean and lakes) moistens the air toward
                    // saturation, faster in wind; land gives back part of its
                    // rain, more where it is warm.
                    evap[i] = wf * EVAPORATION_RATE * (0.4 + 0.6 * wind[i].len().min(2.0));
                    // Land gives back part of its rain, more where it is warm
                    // (never all: the rest runs off, so rain inland needs
                    // moisture from the sea). Half of it follows this month's
                    // rain, half the soil's store from the rest of the year,
                    // once that is known (not below freezing).
                    // (0.35 of it in the cold, 0.9 from 16 °C up, less again
                    // in great heat, where little grows to draw on the soil.)
                    let back = (1.0 - wf) * (0.35 + 0.55 * smoothstep(0.0, 16.0, temp[i]) - 0.35 * smoothstep(24.0, 32.0, temp[i]));
                    match soil_water {
                        Some(store) => {
                            recycle[i] = 0.5 * back;
                            soil[i] = 0.5 * back * store[m][i] * smoothstep(-5.0, 10.0, temp[i]);
                        }
                        None => recycle[i] = back,
                    }
                }
                // Iterate transport of vapour `w` and cloud water `cl`.
                // Start fairly humid everywhere, so that a land which can keep
                // its moisture does, and one that cannot dries out.
                let mut w: Vec<f64> = sat.iter().map(|s| START_HUMIDITY * s).collect();
                let mut cl = vec![0.0f64; nc];
                let mut acc = vec![0.0f64; nc];
                let mut last_rain = vec![0.0f64; nc];
                let mut wide_sum = vec![0.0f64; storm_grid.len()];
                let warm = passes / 2;
                for pass in 0..passes {
                    let next: Vec<(f64, f64, f64)> = (0..nc)
                        .map(|i| {
                            let (mut inflow, mut inflow_cl) = (0.0, 0.0);
                            for e in nbr_off[i] as usize..nbr_off[i + 1] as usize {
                                let (j, wt) = (nbr_idx[e] as usize, in_w[e]);
                                inflow += w[j] * wt;
                                inflow_cl += cl[j] * wt;
                            }
                            let mut v = w[i] * stay[i] + inflow + recycle[i] * last_rain[i];
                            // Only humid air that arrives up the slope is
                            // lifted into cloud, so water lingering in a cell
                            // is not wrung out again each pass.
                            let cond = inflow * lift[i] * (v / sat[i]).min(1.0);
                            v -= cond;
                            v += evap[i] * (sat[i] - v).max(0.0) + soil[i].min(LAND_EVAPORATION_RATE * (sat[i] - v).max(0.0));
                            let mut c = cl[i] * stay[i] + inflow_cl + cond;
                            // Cloud that comes down a lee side evaporates into
                            // air that has room for it.
                            let back = dissolve[i] * c.min((sat[i] - v).max(0.0));
                            c -= back;
                            v += back;
                            // Rain grows steeply with relative humidity: hot,
                            // dry air keeps what little vapour it has.
                            let humid = (v / sat[i]).min(1.2);
                            let ready = showery[i] * humid * humid + (1.0 - showery[i]) * smoothstep(0.55, 1.0, humid);
                            let r = (v * rain_rate[i] * ready).min(0.6 * v);
                            v -= r;
                            // Air that cools (climbing, or moving over colder
                            // ground) condenses what it can no longer hold.
                            if v > sat[i] {
                                c += v - sat[i];
                                v = sat[i];
                            }
                            let rc = c * CLOUD_FALLOUT;
                            (v, c - rc, r + rc)
                        })
                        .collect();
                    for (i, (v, c, r)) in next.into_iter().enumerate() {
                        w[i] = v;
                        cl[i] = c;
                        last_rain[i] = r;
                        if pass >= warm {
                            acc[i] += r;
                        }
                    }
                    // Every few passes, storms even out the vapour of the
                    // storm belts over a wide area (the mean wind alone would
                    // leave the land downwind of mountains and far from the
                    // sea with none, summer or winter).
                    if pass % STORM_EVERY == STORM_EVERY - 1 {
                        wide_sum.iter_mut().for_each(|x| *x = 0.0);
                        for i in 0..nc {
                            wide_sum[storm_xfer.nearest[i] as usize] += w[i] * coarse.area[i];
                        }
                        let wide: Vec<f64> = (0..storm_grid.len()).map(|k| if wide_area[k] > 0.0 { wide_sum[k] / wide_area[k] } else { 0.0 }).collect();
                        let mut even = wide.clone();
                        for _ in 0..STORM_REACH {
                            even = smooth(&storm_grid, &even);
                        }
                        for i in 0..nc {
                            let (t, wt) = (storm_xfer.tri[i], storm_xfer.w[i]);
                            let change: f64 = (0..3).map(|k| wt[k] as f64 * (even[t[k] as usize] - wide[t[k] as usize])).sum();
                            w[i] = (w[i] + storms[i] * change).max(0.0);
                        }
                    }
                }
                let k = 1.0 / (passes - warm) as f64;
                acc.iter().map(|a| a * k).collect()
            })
            .collect()
    };
    // Normalise to a plausible global mean, scaled by temperature
    // (Clausius–Clapeyron); months not listed are interpolated.
    let total_area: f64 = coarse.area.iter().sum();
    let calibrate = |raw: &[Vec<f64>], months: &[usize], t_sl: &[Vec<f32>]| -> (Vec<Vec<f32>>, f64, f64) {
        let t_glob: f64 = (0..nc).map(|i| coarse.area[i] * ((0..12).map(|m| t_sl[m][i] as f64).sum::<f64>() / 12.0 - drop_c[i])).sum::<f64>() / total_area;
        let raw_ann: f64 = (0..nc).map(|i| coarse.area[i] * raw.iter().map(|r| r[i]).sum::<f64>()).sum::<f64>() / total_area * 12.0 / months.len() as f64;
        let target = cp.global_precip_mm * (0.035 * (t_glob - 14.0)).exp();
        let scale = if raw_ann > 0.0 { target / raw_ann } else { 0.0 };
        let known: Vec<Vec<f32>> = raw
            .par_iter()
            .map(|v| {
                let s: Vec<f64> = v.iter().map(|x| x * scale).collect();
                smooth(coarse, &smooth(coarse, &s)).into_iter().map(|x| x as f32).collect()
            })
            .collect();
        let all = (0..12)
            .map(|m| match months.iter().position(|&x| x == m) {
                Some(k) => known[k].clone(),
                None => {
                    // Between the two listed months around it.
                    let before = (0..months.len()).max_by_key(|&k| (months[k] + 12 - m) % 12).unwrap();
                    let after = (0..months.len()).min_by_key(|&k| (months[k] + 12 - m) % 12).unwrap();
                    let (gap, done) = ((months[after] + 12 - months[before]) % 12, (m + 12 - months[before]) % 12);
                    let t = if gap > 0 { done as f32 / gap as f32 } else { 0.0 };
                    (0..nc).map(|i| known[before][i] * (1.0 - t) + known[after][i] * t).collect()
                }
            })
            .collect();
        (all, t_glob, target)
    };
    // How dry the ground is each month (0 wet … 1 parched), from rain against
    // what the warmth could evaporate, with a few months' memory.
    let dryness = |precip: &[Vec<f32>], t_sl: &[Vec<f32>]| -> Vec<Vec<f32>> {
        (0..12)
            .map(|m| {
                (0..nc)
                    .map(|i| {
                        let rain = 0.5 * precip[m][i] + 0.3 * precip[(m + 11) % 12][i] + 0.2 * precip[(m + 10) % 12][i];
                        let demand = 4.0 + 5.0 * (t_sl[m][i] - drop_c[i] as f32).max(0.0);
                        1.0 - smoothstep(0.05, 0.4, (rain / demand) as f64) as f32
                    })
                    .collect()
            })
            .collect()
    };

    let passes = cp.moisture_passes.max(10) as usize;
    let (line0, ms0, unit0) = winds_for(&t0);
    ctx.progress(0.38, "Finding the dry lands");
    // A quick rain estimate (four months, fewer passes) tells which lands are
    // dry; their heat then shapes the winds of the full solve.
    let rough_months = [0usize, 3, 6, 9];
    let rough = moisture(&unit0, &ms0, &t0, &t0, &line0, &sea0, None, None, &rough_months, (passes / 2).max(10));
    let dry0 = dryness(&calibrate(&rough, &rough_months, &t0).0, &t0);
    // Water in the soil each month: the rain of that month and the two
    // before it, in the solve's own units.
    let soil_water: Vec<Vec<f64>> = {
        let at = |m: usize| -> Vec<f64> {
            let before = (0..4).max_by_key(|&k| (rough_months[k] + 12 - m) % 12).unwrap();
            let after = (0..4).min_by_key(|&k| (rough_months[k] + 12 - m) % 12).unwrap();
            let (gap, done) = ((rough_months[after] + 12 - rough_months[before]) % 12, (m + 12 - rough_months[before]) % 12);
            let t = if gap > 0 { done as f64 / gap as f64 } else { 0.0 };
            (0..nc).map(|i| rough[before][i] * (1.0 - t) + rough[after][i] * t).collect()
        };
        let monthly: Vec<Vec<f64>> = (0..12).map(at).collect();
        let yearly: Vec<f64> = (0..nc).map(|i| monthly.iter().map(|v| v[i]).sum::<f64>() / 12.0).collect();
        // Half from the last three months, half from the year's store (deep
        // soil, snowmelt, plants that reach it).
        (0..12).map(|m| (0..nc).map(|i| 0.5 * yearly[i] + 0.25 * monthly[m][i] + 0.15 * monthly[(m + 11) % 12][i] + 0.1 * monthly[(m + 10) % 12][i]).collect()).collect()
    };
    ctx.progress(0.44, "Re-solving temperatures with the new winds");
    let sea = sea_air(&unit0);
    let t1 = temperatures(&sea, Some(&unit0), Some(&dry0));
    let t1_air = temperatures(&sea, Some(&unit0), None);
    let (itcz_c, wind_ms, wind_m) = winds_for(&t1);
    ctx.progress(0.5, "Carrying moisture");
    let all_months: Vec<usize> = (0..12).collect();
    let precip_raw = moisture(&wind_m, &wind_ms, &t1, &t1_air, &itcz_c, &sea, Some(&soil_water), Some(&dry0), &all_months, passes);
    ctx.progress(0.8, "Calibrating rainfall");
    let (precip_c, t_glob, target) = calibrate(&precip_raw, &all_months, &t1);
    let t_sl_c = temperatures(&sea, Some(&wind_m), Some(&dryness(&precip_c, &t1)));
    let (mar, cur) = (&sea.mar, &sea.cur);

    // ---- Foehn: air that rains out on the way up comes down the far side warmer
    ctx.progress(0.85, "Warming lee slopes (Foehn)");
    let foehn_c: Vec<Vec<f32>> = if cp.foehn > 0.0 {
        (0..12)
            .into_par_iter()
            .map(|m| foehn_warming(coarse, &wind_m[m], &h_c, &precip_c[m], cp.foehn, spacing_km))
            .collect()
    } else {
        vec![vec![0.0; nc]; 12]
    };

    // ---- onto the world grid
    ctx.progress(0.88, "Interpolating to world grid");
    let n = fine.len();
    let up = |v: &[f32]| -> Vec<f32> {
        match transfer {
            Some(t) => t.up(v),
            None => v.to_vec(),
        }
    };
    let mut temp = vec![0.0f32; 12 * n];
    let mut precip = vec![0.0f32; 12 * n];
    let mut foehn = vec![0.0f32; n];
    let to32 = |v: &[f64]| -> Vec<f32> { v.iter().map(|&x| x as f32).collect() };
    let (broad_f, broad_drop_f, sheet_f) = (up(&to32(&broad_c)), up(&to32(&broad_drop_c)), up(&to32(&sheet_c)));
    for m in 0..12 {
        let t_sl = up(&t_sl_c[m]);
        let pr = up(&precip_c[m]);
        let fw = up(&foehn_c[m]);
        for i in 0..n {
            let e = elev_f[i];
            let mut t = t_sl[i];
            if water_f[i] != 1 {
                t -= broad_drop_f[i] + lapse as f32 * (e.max(0.0) - broad_f[i]);
            }
            if water_f[i] == 0 {
                // Foehn warms the land; seas and lakes keep their own temperature.
                let w = fw[i].max(0.0);
                t += w;
                foehn[i] += w / 12.0;
            }
            temp[m * n + i] = t;
            precip[m * n + i] = pr[i].max(0.0);
        }
    }
    // Ice sheets: melting holds the summer surface near 0 °C.
    for i in 0..n {
        if water_f[i] != 0 || sheet_f[i] <= 0.0 {
            continue;
        }
        for m in 0..12 {
            let t = &mut temp[m * n + i];
            if *t > 0.0 {
                *t *= 1.0 - 0.8 * sheet_f[i].min(1.0);
            }
        }
    }
    let mut wind_u = vec![0.0f32; 12 * n];
    let mut wind_v = vec![0.0f32; 12 * n];
    for m in 0..12 {
        let (eu, ev): (Vec<f32>, Vec<f32>) = (0..nc)
            .map(|i| {
                let (e, nn) = grad.basis(i);
                (wind_ms[m][i].dot(e) as f32, wind_ms[m][i].dot(nn) as f32)
            })
            .unzip();
        wind_u[m * n..(m + 1) * n].copy_from_slice(&up(&eu));
        wind_v[m * n..(m + 1) * n].copy_from_slice(&up(&ev));
    }
    // Orographic rain finer than the climate grid: within each climate cell,
    // rain moves toward the slopes facing the wind and away from lee slopes.
    if let Some(t) = transfer {
        ctx.progress(0.93, "Placing mountain rain");
        orographic_downscale(fine, t, nc, elev_f, &wind_u, &wind_v, &mut precip, cp.orographic, r_km);
    }
    // Rain-rate limit: the wettest places on Earth average ~2,800 mm in their
    // wettest month. A soft limit (−3% at 1,500 mm) keeps monsoon summits below it.
    precip.par_iter_mut().for_each(|p| *p /= (1.0 + (*p / RAIN_MONTH_MAX_MM).powi(4)).powf(0.25));
    let mar_f = up(&mar.iter().map(|&x| x as f32).collect::<Vec<_>>());
    let cur_f = up(&(0..nc).map(|i| (if ocean_c[i] { current_c[i] } else { cur[i] * mar[i] }) as f32).collect::<Vec<_>>());
    let mut f = Fields::default();
    let (t_mean, t_warm, t_cold, p_ann) = summaries(&temp, &precip, n);
    f.put("t_mean", Field::F32(t_mean));
    f.put("t_warm", Field::F32(t_warm));
    f.put("t_cold", Field::F32(t_cold));
    f.put("p_ann", Field::F32(p_ann));
    f.put("temp", Field::F32(temp));
    f.put("precip", Field::F32(precip));
    f.put("wind_u", Field::F32(wind_u));
    f.put("wind_v", Field::F32(wind_v));
    f.put("continentality", Field::F32(mar_f.iter().map(|m| 1.0 - m).collect()));
    f.put("current_offset", Field::F32(cur_f));
    let foehn_max = foehn.iter().cloned().fold(0.0f32, f32::max);
    f.put("foehn", Field::F32(foehn));

    // Zonal profile for charts.
    let zonal: Vec<serde_json::Value> = (0..37)
        .map(|k| {
            let lat_deg = -90.0 + k as f64 * 5.0;
            // Stay just inside the poles: formulas using tan(latitude) become
            // singular at exactly ±π/2.
            let polar_limit = std::f64::consts::FRAC_PI_2 - 1e-4;
            let lat = lat_deg.to_radians().clamp(-polar_limit, polar_limit);
            let ins: f64 = (0..48).map(|d| insolation(p, lat, (d as f64 + 0.5) / 48.0)).sum::<f64>() / 48.0;
            serde_json::json!({
                "lat": lat_deg,
                "insolation": ins,
                "t_annual": band_interp(&mix_ann, lat),
                "t_ocean_jan": band_interp(&ocean_run.month[0], lat),
                "t_ocean_jul": band_interp(&ocean_run.month[6], lat),
                "t_land_jan": band_interp(&land_run.month[0], lat),
                "t_land_jul": band_interp(&land_run.month[6], lat),
            })
        })
        .collect();
    StepOutput {
        fields: f,
        meta: serde_json::json!({
            "global_mean_temp_c": t_glob,
            "global_precip_mm": target,
            "climate_level": cl,
            "hadley_edge_deg": belts.hadley,
            "ferrel_edge_deg": belts.ferrel,
            "foehn_max_c": foehn_max,
            "zonal": zonal,
        }),
    }
}

/// Share of its vapour that saturated air rains out per transport pass,
/// before the local factors (rising or sinking air, slopes, convection).
const RAIN_RATE: f64 = 0.05;
/// Share of a typical cell's water that a wind of `W_REF` carries on per pass.
const TRANSPORT_STEP: f64 = 0.6;
/// Speed at which vapour drifts along a wind too light to measure, as a share of `W_REF`.
const CARRY_FLOOR: f64 = 0.8;
/// Relative humidity the moisture solve starts from.
const START_HUMIDITY: f64 = 0.6;
/// Share of a cell's water mixed into its neighbours per pass everywhere, and
/// the extra share in the middle of the storm belts (travelling storms carry
/// moisture poleward and inland whatever the mean wind).
const EDDY_MIX: f64 = 0.05;
const STORM_MIX: f64 = 0.2;
/// Share of the sinking-air caps that warm land removes.
const LAND_BREAKS_CAP: f64 = 0.5;
/// How much less readily the trade-wind belts rain than the same air would elsewhere.
const TRADE_CAP: f64 = 0.5;
/// How much more readily air rains in the middle of the storm belts.
const STORM_RAIN: f64 = 2.0;
/// Every STORM_EVERY passes, this share of the vapour in the middle of the
/// storm belts is evened out over several hundred km (STORM_REACH smoothing
/// passes on a grid two levels coarser).
const STORM_EVERY: usize = 3;
const STORM_SPREAD: f64 = 1.0;
const STORM_REACH: usize = 8;
/// Share of its readiness to rain that air over parched ground loses.
const DRY_GROUND_CAP: f64 = 0.4;
/// Share of the gap to saturation that open water closes per pass in a wind of
/// `W_REF`, and the most that moist, warm land closes.
const EVAPORATION_RATE: f64 = 0.2;
const LAND_EVAPORATION_RATE: f64 = 0.06;
/// Air forced up a slope rains up to (1 + GAIN) times as readily, at
/// `orographic` = 1; 63% of that within LIFT metres of climb per cell.
const OROGRAPHIC_GAIN: f64 = 1.6;
const OROGRAPHIC_LIFT_M: f64 = 600.0;
/// Climb (m) over which saturated air condenses 63% of the vapour it brings
/// into cloud, at `orographic` = 1.
const OROGRAPHIC_CLOUD_M: f64 = 800.0;
/// Descent (m) over which sinking air evaporates 63% of the cloud it carries.
const CLOUD_DRY_M: f64 = 1000.0;
/// Share of cloud water that rains out per transport pass; the rest drifts on
/// with the wind, so rain from cooling air spreads downwind.
const CLOUD_FALLOUT: f64 = 0.5;

/// Monthly precipitation approaches this (mm) at the very wettest places.
const RAIN_MONTH_MAX_MM: f32 = 2500.0;

/// World-grid orographic rain: a slope facing the wind gets up to (1 + GAIN)
/// times its share, approached at slopes of a few × SLOPE (m of climb per km);
/// a lee slope keeps e^(−descent/LEE) of it. Climate-cell totals are kept.
const DOWNSCALE_GAIN: f64 = 3.0;
const DOWNSCALE_SLOPE: f64 = 25.0;
const DOWNSCALE_LEE: f64 = 40.0;

/// Redistribute each month's rain inside every climate cell by the world-grid
/// terrain: more where the wind blows up a slope, less where it comes down.
/// The climate grid already set how much falls in the region; this only
/// places it, so every climate cell keeps (up to interpolation) its total.
#[allow(clippy::too_many_arguments)]
fn orographic_downscale(fine: &Grid, t: &GridTransfer, nc: usize, elev: &[f32], wind_u: &[f32], wind_v: &[f32], precip: &mut [f32], strength: f64, r_km: f64) {
    if strength <= 0.0 {
        return;
    }
    let n = fine.len();
    // Terrain gradient (m per km, as a tangent vector) of lightly smoothed
    // heights, so single-cell noise and grid directions do not streak the rain.
    let h0: Vec<f64> = elev.iter().map(|&e| e.max(0.0) as f64).collect();
    let hs = smooth(fine, &smooth(fine, &h0));
    let grad: Vec<Vec3> = (0..n)
        .into_par_iter()
        .map(|i| {
            let nb = fine.neighbors(i);
            let mut g = Vec3::ZERO;
            for &j in nb {
                let d = (fine.pos[j as usize] - fine.pos[i]) * r_km;
                g += d * ((hs[j as usize] - hs[i]) / d.dot(d));
            }
            g * (2.0 / nb.len() as f64)
        })
        .collect();
    let basis: Vec<(Vec3, Vec3)> = fine.pos.iter().map(|p| p.east_north()).collect();
    precip.par_chunks_mut(n).enumerate().for_each(|(m, pm)| {
        let (wu, wv) = (&wind_u[m * n..(m + 1) * n], &wind_v[m * n..(m + 1) * n]);
        // Log of the rain factor: + up a slope facing the wind, − down a lee slope.
        let lf: Vec<f64> = (0..n)
            .map(|i| {
                let (e, nn) = basis[i];
                let w = e * wu[i] as f64 + nn * wv[i] as f64;
                let speed = w.len();
                if speed < 1e-6 {
                    return 0.0;
                }
                // Slope along the wind (m/km); calm air barely lifts.
                let s = w.dot(grad[i]) / speed;
                let calm = (speed / 3.0).min(1.0);
                calm * if s > 0.0 { (1.0 + DOWNSCALE_GAIN * strength * (1.0 - (-s / DOWNSCALE_SLOPE).exp())).ln() } else { s / DOWNSCALE_LEE }
            })
            .collect();
        let lf = smooth(fine, &lf);
        let raw: Vec<f32> = (0..n).map(|i| pm[i] * lf[i].exp() as f32).collect();
        let (before, after) = (t.down(fine, nc, pm), t.down(fine, nc, &raw));
        let ratio: Vec<f32> = (0..nc).map(|c| if after[c] > 0.0 { before[c] / after[c] } else { 1.0 }).collect();
        let keep = t.up(&ratio);
        for i in 0..n {
            pm[i] = raw[i] * keep[i];
        }
    });
}

/// Warming per km of climb on which the air rains out (°C/km at `foehn` = 1).
/// Rising saturated air cools at ~6 °C/km, sinking dry air warms at 9.8 °C/km;
/// a month's mean sees only part of that, since the wind does not always blow
/// across the range.
const FOEHN_C_PER_KM: f64 = 1.5;
/// Monthly Foehn warming approaches this (°C at `foehn` = 1) behind the
/// highest, wettest ranges.
const FOEHN_MAX_C: f64 = 4.0;
/// Distance over which Foehn air mixes back into its surroundings (km).
const FOEHN_REACH_KM: f64 = 300.0;
/// Monthly rain (mm) at which about 63% of the climb is counted as wet.
const FOEHN_WET_MM: f64 = 60.0;

/// One month's Foehn warming (°C) on the climate grid. Air carried along the
/// wind banks warmth while it climbs and rains out (`gain` per metre of climb,
/// scaled by how wet the slope is), and remembers the highest ground it has
/// crossed. Below that crest it shows the banked warmth, up to `gain` per metre
/// of descent: nothing on the windward side or the crest, most at the foot of
/// the lee slope. Both fade with distance as the air mixes. The result
/// saturates toward `FOEHN_MAX_C × strength`.
fn foehn_warming(g: &Grid, wind: &[Vec3], h: &[f64], precip: &[f32], strength: f64, spacing_km: f64) -> Vec<f32> {
    let nc = g.len();
    let gain = strength * FOEHN_C_PER_KM / 1000.0;
    let decay = (-spacing_km / FOEHN_REACH_KM).exp();
    let passes = ((FOEHN_REACH_KM * 3.0) / spacing_km).ceil() as usize + 4;
    let wet: Vec<f64> = precip.iter().map(|&p| 1.0 - (-(p.max(0.0) as f64) / FOEHN_WET_MM).exp()).collect();
    let mut bank = vec![0.0f64; nc];
    let mut crest = h.to_vec();
    for _ in 0..passes {
        let next: Vec<(f64, f64)> = (0..nc)
            .map(|i| {
                let (mut aw, mut ab, mut ac, mut ah) = (0.0, 0.0, 0.0, 0.0);
                for &nb in g.neighbors(i) {
                    let j = nb as usize;
                    let a = wind[j].dot((g.pos[i] - g.pos[j]).normalized()).max(0.0);
                    aw += a;
                    ab += a * bank[j];
                    ac += a * crest[j];
                    ah += a * h[j];
                }
                if aw <= 0.0 {
                    return (0.0, h[i]);
                }
                let climb = (h[i] - ah / aw).max(0.0);
                let b = ab / aw * decay + gain * wet[i] * climb;
                let c = h[i] + (ac / aw - h[i]).max(0.0) * decay;
                (b, c)
            })
            .collect();
        for (i, (b, c)) in next.into_iter().enumerate() {
            bank[i] = b;
            crest[i] = c;
        }
    }
    let cap = FOEHN_MAX_C * strength;
    (0..nc)
        .map(|i| {
            let raw = bank[i].min(gain * (crest[i] - h[i])).max(0.0);
            (cap * (1.0 - (-raw / cap).exp())) as f32
        })
        .collect()
}

/// Distance (km) over which air keeps the temperature it came with, at a wind
/// of `W_REF`, over land and over sea (the sea reshapes the air above it faster).
const CARRY_LAND_KM: f64 = 500.0;
const CARRY_SEA_KM: f64 = 300.0;

/// One month's temperatures with the wind's carrying: each cell settles between
/// its own local temperature and that of the air arriving from upwind, the
/// more so the stronger the wind. Cold continental air reaches the coasts it
/// blows off, and mild air the lands it blows onto.
fn advect_temperature(g: &Grid, wind: &[Vec3], dir_in: &[Vec3], local: &[f64], water_frac: &[f64], spacing_km: f64) -> Vec<f64> {
    let n = g.len();
    let (off, idx) = g.neighbor_csr();
    // Inflow weight of each neighbour, normalised per cell.
    let mut wt = vec![0.0f64; idx.len()];
    let mut kappa = vec![0.0f64; n];
    let mut kmax: f64 = 0.0;
    for i in 0..n {
        let range = off[i] as usize..off[i + 1] as usize;
        let mut sum = 0.0;
        for e in range.clone() {
            let j = idx[e] as usize;
            let a = wind[j].dot(dir_in[e]).max(0.0);
            wt[e] = a;
            sum += a;
        }
        if sum > 0.0 {
            for e in range {
                wt[e] /= sum;
            }
            let reach = CARRY_LAND_KM + (CARRY_SEA_KM - CARRY_LAND_KM) * water_frac[i];
            kappa[i] = reach * wind[i].len().min(2.0) / spacing_km;
            kmax = kmax.max(kappa[i]);
        }
    }
    let passes = ((3.0 * kmax).ceil() as usize).clamp(4, 60);
    let mut t = local.to_vec();
    for _ in 0..passes {
        t = (0..n)
            .map(|i| {
                if kappa[i] <= 0.0 {
                    return local[i];
                }
                let up: f64 = (off[i] as usize..off[i + 1] as usize).map(|e| wt[e] * t[idx[e] as usize]).sum();
                (local[i] + kappa[i] * up) / (1.0 + kappa[i])
            })
            .collect();
    }
    t
}

/// Annual mean, warmest month, coldest month, annual precipitation.
pub fn summaries(temp: &[f32], precip: &[f32], n: usize) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut t_mean = vec![0.0f32; n];
    let mut t_warm = vec![f32::MIN; n];
    let mut t_cold = vec![f32::MAX; n];
    let mut p_ann = vec![0.0f32; n];
    for m in 0..12 {
        for i in 0..n {
            let t = temp[m * n + i];
            t_mean[i] += t / 12.0;
            t_warm[i] = t_warm[i].max(t);
            t_cold[i] = t_cold[i].min(t);
            p_ann[i] += precip[m * n + i];
        }
    }
    (t_mean, t_warm, t_cold, p_ann)
}

fn smooth(g: &Grid, v: &[f64]) -> Vec<f64> {
    (0..g.len())
        .into_par_iter()
        .map(|i| {
            let nb = g.neighbors(i);
            let s: f64 = nb.iter().map(|&j| v[j as usize]).sum();
            0.5 * v[i] + 0.5 * s / nb.len() as f64
        })
        .collect()
}
