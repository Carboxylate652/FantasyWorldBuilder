//! Step 5 — climate estimator. Ad-hoc but physically motivated, in the spirit
//! of the geometrian climate simulator, extended with seasons:
//!
//! 1. Daily-mean insolation by latitude for every day of the year.
//! 2. A 1-D Budyko–Sellers energy balance in x = sin(lat), integrated through
//!    the year until periodic, with ice-albedo feedback. Three runs: ocean-like
//!    and land-like heat capacity give the seasonal swing for maritime and
//!    continental places; a land-fraction-weighted run gives the annual mean.
//! 3. Local temperature: continentality from maritime air carried along the
//!    wind, warm/cold coastal currents, and a lapse rate.
//! 4. Three-cell winds with a seasonally migrating ITCZ, blended so they never
//!    break at the cell edges.
//! 5. Moisture: evaporation, transport along the wind, orographic rain and
//!    rain shadows, ITCZ and subtropical-high modulation.
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

const NB: usize = 90; // EBM bands, uniform in x = sin(lat)
const A_OLR: f64 = 203.3;
const B_OLR: f64 = 2.09;
const C_OCEAN: f64 = 1.3e8; // ~30 m mixed layer, J/m²/K
const C_LAND: f64 = 2.5e7; // land + the atmosphere column mixing over it
const DAY_S: f64 = 86400.0;
/// Fraction of the year at which the northern spring equinox falls (Earth: ~20 March).
const EQUINOX_FRAC: f64 = 79.5 / 365.25;

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
/// feedback. Bands whose previous annual mean was below -8 °C hold an ice sheet
/// that does not darken in summer.
fn albedo(t: f64, x: f64, t_annual_prev: f64) -> f64 {
    let p2 = 0.5 * (3.0 * x * x - 1.0);
    let base = 1.0 - (0.697 - 0.0779 * p2);
    let ice = 1.0 - smoothstep(-10.0, -2.0, t);
    let sheet = if t_annual_prev < -8.0 { 0.85 } else { 0.0 };
    base + (0.62 - base) * ice.max(sheet)
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

fn run_ebm(p: &WorldParams, heat_cap: &[f64; NB]) -> Ebm {
    let pl = &p.planet;
    let days = pl.year_length_days.clamp(20.0, 5000.0).round() as usize;
    let dx = 2.0 / NB as f64;
    let rot = (pl.day_length_h / 24.0).clamp(0.05, 50.0);
    let d = p.climate.heat_diffusion * rot.powf(0.5).clamp(0.4, 3.0);
    let forcing = 30.0 * pl.greenhouse.max(0.05).ln() / 2f64.ln();
    let lat: Vec<f64> = (0..NB).map(|k| band_x(k).asin()).collect();
    // Edge diffusivities (1 - x²) at k+1/2.
    let edge: Vec<f64> = (0..=NB).map(|k| {
        let x = -1.0 + k as f64 * dx;
        1.0 - x * x
    }).collect();
    // Precompute insolation for each day and band.
    let q: Vec<[f64; NB]> = (0..days)
        .map(|day| {
            let f = (day as f64 + 0.5) / days as f64;
            std::array::from_fn(|k| insolation(p, lat[k], f))
        })
        .collect();

    let mut t = [12.0f64; NB];
    let mut t_ann_prev = [12.0f64; NB];
    let mut t_ann_acc = [0.0f64; NB];
    let years = 40;
    let mut month_sum = vec![[0.0f64; NB]; 12];
    let mut month_cnt = [0usize; 12];
    let (mut a, mut b, mut c, mut r) = ([0.0; NB], [0.0; NB], [0.0; NB], [0.0; NB]);
    for year in 0..years {
        for day in 0..days {
            for k in 0..NB {
                let cdt = heat_cap[k] / DAY_S;
                let asr = q[day][k] * (1.0 - albedo(t[k], band_x(k), t_ann_prev[k]));
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
                t_ann_acc[k] += t[k] / days as f64;
            }
            if year == years - 1 {
                let m = ((day * 12) / days).min(11);
                for k in 0..NB {
                    month_sum[m][k] += t[k];
                }
                month_cnt[m] += 1;
            }
        }
        t_ann_prev = t_ann_acc;
        t_ann_acc = [0.0; NB];
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

/// Surface wind (east, north), unit-ish magnitude, at a latitude in a month.
pub fn wind_at(p: &WorldParams, lat_rad: f64, month: usize) -> (f64, f64) {
    let belts = wind_belts(p);
    let (h, f) = (belts.hadley, belts.ferrel);
    let phi = lat_rad.to_degrees() - itcz_lat(p, month);
    let s = if phi >= 0.0 { 1.0 } else { -1.0 };
    let a = phi.abs();
    let (u, v) = if a < h {
        let w = (PI * a / h).sin();
        (-w, -s * 0.45 * w)
    } else if a < f {
        let w = (PI * (a - h) / (f - h)).sin();
        (w, s * 0.3 * w)
    } else {
        let span = (90.0 + 10.0 - f).max(5.0);
        let w = (PI * ((a - f) / span).min(1.0)).sin();
        (-0.55 * w, -s * 0.25 * w)
    };
    (u, v)
}

fn wind_vec(p: &WorldParams, pos: Vec3, lat: f64, month: usize) -> Vec3 {
    let (u, v) = wind_at(p, lat, month);
    let (e, n) = pos.east_north();
    e * u + n * v
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
    let coarse_owned;
    let coarse: &Grid = if cl == fine.level {
        fine
    } else {
        coarse_owned = Grid::new(cl);
        &coarse_owned
    };
    let transfer = if cl == fine.level { None } else { Some(GridTransfer::new(fine, coarse)) };
    let nc = coarse.len();
    let elev_c: Vec<f32> = match &transfer {
        Some(t) => t.down(fine, nc, elev_f),
        None => elev_f.to_vec(),
    };
    let frac = |pred: &dyn Fn(u8) -> bool| -> Vec<f64> {
        let v: Vec<f32> = water_f.iter().map(|&w| pred(w) as u8 as f32).collect();
        let c = match &transfer {
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
    let mut land_frac = [0.0f64; NB];
    let mut band_area = [0.0f64; NB];
    for i in 0..nc {
        let k = (((coarse.lat[i].sin() + 1.0) / 2.0 * NB as f64) as usize).min(NB - 1);
        band_area[k] += coarse.area[i];
        land_frac[k] += coarse.area[i] * (1.0 - water_frac[i]);
    }
    let mut cap_mix = [0.0; NB];
    for k in 0..NB {
        let lf = if band_area[k] > 0.0 { land_frac[k] / band_area[k] } else { 0.0 };
        land_frac[k] = lf;
        cap_mix[k] = lf * C_LAND + (1.0 - lf) * C_OCEAN;
    }
    let caps = [cap_mix, [C_OCEAN; NB], [C_LAND; NB]];
    let runs: Vec<Ebm> = caps.par_iter().map(|c| run_ebm(p, c)).collect();
    let (mix, ocean_run, land_run) = (&runs[0], &runs[1], &runs[2]);
    let mix_ann: [f64; NB] = std::array::from_fn(|k| mix.annual(k));
    let anom = |run: &Ebm, m: usize| -> [f64; NB] {
        std::array::from_fn(|k| run.month[m][k] - run.annual(k))
    };
    let anom_o: Vec<[f64; NB]> = (0..12).map(|m| anom(ocean_run, m)).collect();
    let anom_l: Vec<[f64; NB]> = (0..12).map(|m| anom(land_run, m)).collect();

    // ---- base winds: zonal three-cell belts (unitless; 1 ≈ W_REF m/s)
    ctx.progress(0.2, "Blowing winds");
    let wind_base: Vec<Vec<Vec3>> = (0..12)
        .into_par_iter()
        .map(|m| (0..nc).map(|i| wind_vec(p, coarse.pos[i], coarse.lat[i], m)).collect())
        .collect();
    let annual_mean = |w: &[Vec<Vec3>]| -> Vec<Vec3> {
        (0..nc)
            .map(|i| {
                let mut s = Vec3::ZERO;
                for m in 0..12 {
                    s += w[m][i];
                }
                s * (1.0 / 12.0)
            })
            .collect()
    };

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
            let prox = (-de.min(dw) / 1200.0).exp();
            let a = lat.to_degrees().abs();
            let profile = -gauss(a, 25.0, 12.0) + gauss(a, 57.0, 10.0);
            cp.ocean_current_c * prox * s * profile
        })
        .collect();

    // ---- maritime influence along the wind, then sea-level temperatures.
    // Run twice: with the belt winds, then with the thermally driven winds.
    let lapse = cp.lapse_rate_c_per_km / 1000.0;
    let thermo = |wind_ann: &[Vec3]| -> (Vec<f64>, Vec<f64>, Vec<Vec<f32>>) {
    let reach = cp.maritime_reach_km.max(50.0);
    let decay_adv = (-spacing_km / reach).exp();
    let decay_iso = (-spacing_km / (reach * 0.22)).exp();
    // Lakes are weaker maritime sources than the ocean (smaller, freeze, no currents).
    let mut mar: Vec<f64> = (0..nc).map(|i| if ocean_c[i] { 1.0 } else { 0.6 * lake_frac[i] }).collect();
    let mut cur: Vec<f64> = current_c.clone();
    let passes = ((reach * 2.5) / spacing_km).ceil() as usize + 4;
    for _ in 0..passes {
        let next: Vec<(f64, f64)> = (0..nc)
            .into_par_iter()
            .map(|i| {
                if ocean_c[i] {
                    return (1.0, current_c[i]);
                }
                let mut aw = 0.0;
                let mut am = 0.0;
                let mut ac = 0.0;
                let mut iso: f64 = 0.0;
                let mut iso_c = 0.0;
                for &nb in coarse.neighbors(i) {
                    let j = nb as usize;
                    let dir = (coarse.pos[i] - coarse.pos[j]).normalized();
                    let a = wind_ann[j].dot(dir).max(0.0);
                    aw += a;
                    am += a * mar[j];
                    ac += a * mar[j] * cur[j];
                    if mar[j] > iso {
                        iso = mar[j];
                        iso_c = cur[j];
                    }
                }
                let adv = if aw > 0.0 { am / aw } else { 0.0 };
                let adv_c = if am > 0.0 { ac / am } else { 0.0 };
                let va = adv * decay_adv;
                let vi = iso * decay_iso;
                if va >= vi {
                    (va.max(mar[i]), adv_c)
                } else {
                    (vi.max(mar[i]), iso_c)
                }
            })
            .collect();
        for (i, (m, c)) in next.into_iter().enumerate() {
            mar[i] = m;
            cur[i] = c;
        }
    }

    let t_sl_c: Vec<Vec<f32>> = (0..12)
        .into_par_iter()
        .map(|m| {
            (0..nc)
                .map(|i| {
                    let lat = coarse.lat[i];
                    let cont = 1.0 - mar[i];
                    let a = band_interp(&anom_o[m], lat) * (1.0 - cont) + band_interp(&anom_l[m], lat) * cont;
                    let off = if ocean_c[i] { current_c[i] } else { cur[i] * mar[i] };
                    let mut t = band_interp(&mix_ann, lat) + a + off;
                    if ocean_c[i] {
                        t = t.max(-1.8);
                    }
                    t as f32
                })
                .collect()
        })
        .collect();
    (mar, cur, t_sl_c)
    };
    ctx.progress(0.3, "Spreading maritime air");
    let (_, _, t_first) = thermo(&annual_mean(&wind_base));

    // ---- thermally driven winds and mountain blocking
    ctx.progress(0.36, "Building pressure systems");
    let h_c: Vec<f64> = elev_c.iter().map(|&e| e.max(0.0) as f64).collect();
    let grad = super::wind::Gradient::new(coarse, r_km * 1000.0);
    let wind_ms = super::wind::dynamic_winds(p, coarse, &grad, &wind_base, &t_first, &h_c, &water_frac);
    let wind_m: Vec<Vec<Vec3>> = wind_ms.iter().map(|w| w.iter().map(|v| *v * (1.0 / super::wind::W_REF)).collect()).collect();
    ctx.progress(0.4, "Re-solving temperatures with the new winds");
    let (mar, cur, t_sl_c) = thermo(&annual_mean(&wind_m));

    // ---- moisture and precipitation (coarse, months in parallel)
    ctx.progress(0.45, "Carrying moisture");
    let belts = wind_belts(p);
    let passes = cp.moisture_passes.max(10) as usize;
    let precip_raw: Vec<Vec<f64>> = (0..12)
        .into_par_iter()
        .map(|m| {
            let wind = &wind_m[m];
            let itcz = itcz_lat(p, m);
            // Low-level convergence (1/s) lifts air and makes rain; divergence suppresses it.
            let conv: Vec<f64> = (0..nc).map(|i| -grad.divergence(coarse, i, &wind_ms[m])).collect();
            let temp: Vec<f64> = (0..nc).map(|i| t_sl_c[m][i] as f64 - lapse * h_c[i]).collect();
            // Outgoing transport fractions for each cell, aligned with its neighbour list.
            let mut out_w: Vec<Vec<f64>> = Vec::with_capacity(nc);
            let mut stay = vec![0.0; nc];
            const DIFF: f64 = 0.06;
            for j in 0..nc {
                let nbs = coarse.neighbors(j);
                let wj = wind[j];
                let speed = wj.len();
                let mut a: Vec<f64> = nbs
                    .iter()
                    .map(|&nb| wj.dot((coarse.pos[nb as usize] - coarse.pos[j]).normalized()).max(0.0))
                    .collect();
                let s: f64 = a.iter().sum();
                let mu = (0.2 + 0.7 * speed).min(0.85);
                let deg = nbs.len() as f64;
                for x in a.iter_mut() {
                    *x = if s > 0.0 { mu * *x / s } else { 0.0 } + DIFF / deg;
                }
                stay[j] = 1.0 - a.iter().sum::<f64>();
                out_w.push(a);
            }
            // Incoming weights for each cell, aligned with its own neighbour list.
            let in_w: Vec<Vec<f64>> = (0..nc)
                .map(|i| {
                    coarse
                        .neighbors(i)
                        .iter()
                        .map(|&nb| {
                            let j = nb as usize;
                            let k = coarse.neighbors(j).iter().position(|&x| x as usize == i).unwrap();
                            out_w[j][k]
                        })
                        .collect()
                })
                .collect();
            // Per-cell rain fraction and evaporation.
            let mut rain_frac = vec![0.0; nc];
            let mut evap = vec![0.0; nc];
            let mut cap = vec![0.0; nc];
            for i in 0..nc {
                let nbs = coarse.neighbors(i);
                let mut up = 0.0;
                let mut down = 0.0;
                let mut wsum = 0.0;
                for (k, &nb) in nbs.iter().enumerate() {
                    let j = nb as usize;
                    let w = in_w[i][k];
                    up += w * (h_c[i] - h_c[j]).max(0.0);
                    down += w * (h_c[j] - h_c[i]).max(0.0);
                    wsum += w;
                }
                if wsum > 0.0 {
                    up /= wsum;
                    down /= wsum;
                }
                let lat = coarse.lat[i].to_degrees();
                let phi = lat - itcz;
                let a = phi.abs();
                // The ITCZ bump is smaller now that convergence adds rain there too.
                let mut lm = 1.0 + 1.2 * gauss(phi, 0.0, 7.0);
                lm *= 1.0 + 0.6 * (conv[i] / 3.0e-6).clamp(-0.7, 2.0);
                lm *= 1.0 - 0.7 * gauss(a, belts.hadley, 8.0);
                lm *= 1.0 + 0.5 * gauss(a, belts.ferrel - 8.0, 10.0);
                let cold_coast = (1.0 + 0.09 * cur[i].min(0.0) * mar[i]).clamp(0.3, 1.0);
                // Saturating: a steep windward slope wrings out at most ~30% per step,
                // so rain spreads up the slope instead of piling onto one cell.
                let oro = 0.3 * cp.orographic * (1.0 - (-up / 900.0).exp());
                let shadow = (-down / 900.0).exp();
                // Summer convection over warm land.
                let convect = 1.0 + 0.8 * (1.0 - water_frac[i]) * smoothstep(8.0, 28.0, temp[i]);
                rain_frac[i] = ((0.03 * lm + oro) * shadow * cold_coast * convect).clamp(0.002, 0.6);
                let q = qsat(temp[i]);
                let speed = wind[i].len();
                // Open water (ocean and lakes) evaporates freely; land only weakly.
                let wf = water_frac[i];
                evap[i] = wf * 0.12 * q * (0.5 + speed) + (1.0 - wf) * 0.01 * q;
                cap[i] = 9.0 * q;
            }
            // Iterate transport.
            let mut w = vec![0.0f64; nc];
            let mut acc = vec![0.0f64; nc];
            let mut last_rain = vec![0.0f64; nc];
            const RECYCLE: f64 = 0.55;
            let warm = passes / 2;
            for pass in 0..passes {
                let next: Vec<(f64, f64)> = (0..nc)
                    .map(|i| {
                        // Land re-evaporates part of what fell on it last pass (recycling).
                        let recycled = RECYCLE * (1.0 - water_frac[i]) * last_rain[i];
                        let mut v = w[i] * stay[i] + evap[i] + recycled;
                        for (k, &nb) in coarse.neighbors(i).iter().enumerate() {
                            v += w[nb as usize] * in_w[i][k];
                        }
                        let mut r = v * rain_frac[i];
                        if v - r > cap[i] {
                            r = v - cap[i];
                        }
                        (v - r, r)
                    })
                    .collect();
                for (i, (v, r)) in next.into_iter().enumerate() {
                    w[i] = v;
                    last_rain[i] = r;
                    if pass >= warm {
                        acc[i] += r;
                    }
                }
            }
            let k = 1.0 / (passes - warm) as f64;
            acc.iter().map(|a| a * k).collect()
        })
        .collect();

    // Normalise to a plausible global mean, scaled by temperature (Clausius–Clapeyron).
    ctx.progress(0.8, "Calibrating rainfall");
    let total_area: f64 = coarse.area.iter().sum();
    let t_glob: f64 = (0..nc)
        .map(|i| coarse.area[i] * (0..12).map(|m| t_sl_c[m][i] as f64 - lapse * h_c[i]).sum::<f64>() / 12.0)
        .sum::<f64>()
        / total_area;
    let raw_ann: f64 = (0..nc).map(|i| coarse.area[i] * (0..12).map(|m| precip_raw[m][i]).sum::<f64>()).sum::<f64>() / total_area;
    let target = cp.global_precip_mm * (0.035 * (t_glob - 14.0)).exp();
    let scale = if raw_ann > 0.0 { target / raw_ann } else { 0.0 };
    let precip_c: Vec<Vec<f32>> = precip_raw
        .iter()
        .map(|v| {
            let s: Vec<f64> = v.iter().map(|x| x * scale).collect();
            smooth(coarse, &smooth(coarse, &s)).into_iter().map(|x| x as f32).collect()
        })
        .collect();

    // ---- onto the world grid
    ctx.progress(0.88, "Interpolating to world grid");
    let n = fine.len();
    let up = |v: &[f32]| -> Vec<f32> {
        match &transfer {
            Some(t) => t.up(v),
            None => v.to_vec(),
        }
    };
    let mut temp = vec![0.0f32; 12 * n];
    let mut precip = vec![0.0f32; 12 * n];
    for m in 0..12 {
        let t_sl = up(&t_sl_c[m]);
        let pr = up(&precip_c[m]);
        for i in 0..n {
            let e = elev_f[i];
            let mut t = t_sl[i] - (lapse * e.max(0.0) as f64) as f32;
            if water_f[i] == 1 {
                t = t.max(-1.8);
            }
            temp[m * n + i] = t;
            precip[m * n + i] = pr[i].max(0.0);
        }
    }
    // Ice sheets: where the annual mean is far below freezing, melting holds the
    // summer surface near 0 °C.
    for i in 0..n {
        if water_f[i] != 0 {
            continue;
        }
        let mean: f32 = (0..12).map(|m| temp[m * n + i]).sum::<f32>() / 12.0;
        if mean < -9.0 {
            for m in 0..12 {
                let t = &mut temp[m * n + i];
                if *t > 0.0 {
                    *t *= 0.2;
                }
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

    // Zonal profile for charts.
    let zonal: Vec<serde_json::Value> = (0..37)
        .map(|k| {
            let lat_deg = -90.0 + k as f64 * 5.0;
            let lat = lat_deg.to_radians().clamp(-1.5707, 1.5707);
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
            "zonal": zonal,
        }),
    }
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
