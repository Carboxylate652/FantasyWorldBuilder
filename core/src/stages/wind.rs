//! Longitude-dependent surface winds for the climate step.
//!
//! The zonal three-cell belts (climate::wind_at) are the base flow. On top:
//!
//! * Thermal pressure: a surface-pressure anomaly p′ = −α·(T − zonal mean T),
//!   so cold continents in winter become highs and hot ones in summer lows.
//!   T is the sea-level temperature, with the land anomaly amplified by height:
//!   an elevated surface heats or cools the air above it more than the free
//!   atmosphere at that height, which makes plateaus heat lows in summer.
//!   Raw surface pressure (P ~ −h) is deliberately *not* used: its slope-induced
//!   gradients are ~100× larger than real ones and would blow down every slope.
//!   The anomaly is smoothed to synoptic scale (~1,000 km).
//! * Friction–Coriolis balance: r·v + f·k̂×v = G with G = −∇p′/ρ, solved in
//!   closed form. Near the equator (f → 0) air flows down the gradient into
//!   lows (monsoon inflow); at mid-latitudes it runs along the isobars.
//! * Mountain blocking: part of any upslope component is turned along the
//!   contours, so air flows around ranges and is channelled through gaps.
//!
//! Winds are in m/s internally; the moisture model uses them divided by W_REF.

use crate::grid::{Grid, GridTransfer};
use crate::params::WorldParams;
use crate::vec3::Vec3;
use rayon::prelude::*;

/// m/s represented by one unit of the zonal belt wind.
pub const W_REF: f64 = 8.0;
/// hPa of surface-pressure anomaly per °C of temperature anomaly.
const ALPHA_HPA_PER_C: f64 = 1.0;
/// Boundary-layer friction (1/s), about a one-day spin-down.
const FRICTION: f64 = 2.5e-5;
const RHO_AIR: f64 = 1.2;
const MAX_THERMAL_MS: f64 = 20.0;

/// Least-squares gradients in each cell's local east/north frame.
pub struct Gradient {
    east: Vec<Vec3>,
    north: Vec<Vec3>,
    /// Per neighbour: (dx, dy) in metres, aligned with the grid's neighbour list.
    offsets: Vec<Vec<(f64, f64)>>,
    /// Inverse of the 2×2 normal matrix, per cell.
    inv: Vec<[f64; 4]>,
}

impl Gradient {
    pub fn new(g: &Grid, radius_m: f64) -> Gradient {
        let n = g.len();
        let rows: Vec<(Vec3, Vec3, Vec<(f64, f64)>, [f64; 4])> = (0..n)
            .into_par_iter()
            .map(|i| {
                let p = g.pos[i];
                let (e, nn) = p.east_north();
                let offs: Vec<(f64, f64)> = g
                    .neighbors(i)
                    .iter()
                    .map(|&j| {
                        let d = (g.pos[j as usize] - p).tangent_at(p);
                        (d.dot(e) * radius_m, d.dot(nn) * radius_m)
                    })
                    .collect();
                let (mut a, mut b, mut c) = (0.0, 0.0, 0.0);
                for &(dx, dy) in &offs {
                    a += dx * dx;
                    b += dx * dy;
                    c += dy * dy;
                }
                let det = a * c - b * b;
                let inv = if det.abs() > 0.0 { [c / det, -b / det, -b / det, a / det] } else { [0.0; 4] };
                (e, nn, offs, inv)
            })
            .collect();
        let mut east = Vec::with_capacity(n);
        let mut north = Vec::with_capacity(n);
        let mut offsets = Vec::with_capacity(n);
        let mut inv = Vec::with_capacity(n);
        for (e, nn, o, m) in rows {
            east.push(e);
            north.push(nn);
            offsets.push(o);
            inv.push(m);
        }
        Gradient { east, north, offsets, inv }
    }

    /// (∂/∂east, ∂/∂north) of a scalar at cell i, per metre.
    pub fn at(&self, g: &Grid, i: usize, v: impl Fn(usize) -> f64) -> (f64, f64) {
        let vi = v(i);
        let (mut sx, mut sy) = (0.0, 0.0);
        for (k, &j) in g.neighbors(i).iter().enumerate() {
            let (dx, dy) = self.offsets[i][k];
            let dv = v(j as usize) - vi;
            sx += dx * dv;
            sy += dy * dv;
        }
        let m = &self.inv[i];
        (m[0] * sx + m[1] * sy, m[2] * sx + m[3] * sy)
    }

    pub fn basis(&self, i: usize) -> (Vec3, Vec3) {
        (self.east[i], self.north[i])
    }

    /// Horizontal divergence (1/s) of a tangent vector field (m/s).
    pub fn divergence(&self, g: &Grid, i: usize, w: &[Vec3]) -> f64 {
        let (e, n) = (self.east[i], self.north[i]);
        let (dudx, _) = self.at(g, i, |j| w[j].dot(e));
        let (_, dvdy) = self.at(g, i, |j| w[j].dot(n));
        dudx + dvdy
    }
}

/// Monthly winds (m/s, tangent 3-D vectors) on the climate grid.
#[allow(clippy::too_many_arguments)]
pub fn dynamic_winds(
    p: &WorldParams,
    g: &Grid,
    grad: &Gradient,
    base: &[Vec<Vec3>],
    t_sl: &[Vec<f32>],
    h: &[f64],
    water_frac: &[f64],
) -> Vec<Vec<Vec3>> {
    let cp = &p.climate;
    let n = g.len();
    let omega = std::f64::consts::TAU / (p.planet.day_length_h.max(0.5) * 3600.0);

    // Synoptic smoothing on a coarse sub-grid of the same hierarchy (~450 km cells on Earth).
    let smooth_level = g.level.saturating_sub(3).clamp(2, 4);
    let sg = Grid::new(smooth_level);
    let xfer = GridTransfer::new(g, &sg);
    let sgrad = Gradient::new(&sg, p.planet.radius_km * 1000.0);

    // Terrain slope for blocking (uphill unit vector and steepness in m/km).
    let slope: Vec<(Vec3, f64)> = (0..n)
        .map(|i| {
            let (gx, gy) = grad.at(g, i, |j| h[j]);
            let (e, nn) = grad.basis(i);
            let s = (gx * gx + gy * gy).sqrt();
            let dir = if s > 0.0 { (e * gx + nn * gy) * (1.0 / s) } else { Vec3::ZERO };
            (dir, s * 1000.0)
        })
        .collect();

    // Zonal-mean helper (area-weighted, 60 bands in sin(lat)).
    const ZB: usize = 60;
    let band = |lat: f64| (((lat.sin() + 1.0) / 2.0 * ZB as f64) as usize).min(ZB - 1);

    (0..12)
        .into_par_iter()
        .map(|m| {
            let t = &t_sl[m];
            let mut zs = [0.0f64; ZB];
            let mut zw = [0.0f64; ZB];
            for i in 0..n {
                let b = band(g.lat[i]);
                zs[b] += t[i] as f64 * g.area[i];
                zw[b] += g.area[i];
            }
            let zmean: Vec<f64> = (0..ZB).map(|b| if zw[b] > 0.0 { zs[b] / zw[b] } else { 0.0 }).collect();
            // Thermal pressure anomaly; elevated land amplifies its own anomaly.
            let pres: Vec<f32> = (0..n)
                .map(|i| {
                    let a = t[i] as f64 - zmean[band(g.lat[i])];
                    let lift = 1.0 + 0.35 * (h[i] / 1000.0) * (1.0 - water_frac[i]);
                    (-ALPHA_HPA_PER_C * a * lift) as f32
                })
                .collect();
            // Smooth to synoptic scale.
            let mut ps = xfer.down(g, sg.len(), &pres);
            for _ in 0..3 {
                ps = (0..sg.len())
                    .map(|i| {
                        let nb = sg.neighbors(i);
                        let s: f32 = nb.iter().map(|&j| ps[j as usize]).sum();
                        0.5 * ps[i] + 0.5 * s / nb.len() as f32
                    })
                    .collect();
            }
            // Pressure-gradient acceleration G = −∇p/ρ (hPa → Pa), computed on the
            // smoothing grid and interpolated as a vector: interpolating p and
            // differentiating afterwards would make G piecewise constant (faceted winds).
            let (mut gx3, mut gy3, mut gz3) = (vec![0.0f32; sg.len()], vec![0.0f32; sg.len()], vec![0.0f32; sg.len()]);
            for i in 0..sg.len() {
                let (px, py) = sgrad.at(&sg, i, |j| ps[j] as f64);
                let (e, nn) = sgrad.basis(i);
                let gv = (e * px + nn * py) * (-100.0 / RHO_AIR);
                gx3[i] = gv.x as f32;
                gy3[i] = gv.y as f32;
                gz3[i] = gv.z as f32;
            }
            let (gx3, gy3, gz3) = (xfer.up(&gx3), xfer.up(&gy3), xfer.up(&gz3));

            (0..n)
                .map(|i| {
                    let (e, nn) = grad.basis(i);
                    let gv = Vec3::new(gx3[i] as f64, gy3[i] as f64, gz3[i] as f64);
                    let (gx, gy) = (gv.dot(e), gv.dot(nn));
                    let f = 2.0 * omega * g.lat[i].sin();
                    let den = FRICTION * FRICTION + f * f;
                    let mut ve = (FRICTION * gx + f * gy) / den;
                    let mut vn = (FRICTION * gy - f * gx) / den;
                    let sp = (ve * ve + vn * vn).sqrt();
                    if sp > MAX_THERMAL_MS {
                        ve *= MAX_THERMAL_MS / sp;
                        vn *= MAX_THERMAL_MS / sp;
                    }
                    let mut w = base[m][i] * W_REF + (e * ve + nn * vn) * cp.thermal_wind;

                    // Mountain blocking: turn part of the upslope component along the contours.
                    let (up, s_km) = slope[i];
                    let wn = w.dot(up);
                    if wn > 0.0 && cp.mountain_blocking > 0.0 {
                        let b = cp.mountain_blocking.clamp(0.0, 1.0) * (1.0 - (-s_km / 12.0).exp());
                        let along = w - up * wn;
                        let al = along.len();
                        let dir = if al > 1e-9 { along * (1.0 / al) } else { g.pos[i].cross(up).normalized() };
                        w = along + up * (wn * (1.0 - b)) + dir * (wn * b);
                    }
                    w
                })
                .collect()
        })
        .collect()
}
