//! Geodesic sphere grid: a subdivided icosahedron with N = 10·4^n + 2 cells
//! (hexagons plus 12 pentagons). Cells are the mesh vertices; the triangles
//! are only used for rendering and for barycentric interpolation.
//!
//! Vertex ordering is hierarchical: the first N(n-1) vertices of a level-n grid
//! are exactly the vertices of the level-(n-1) grid, which makes coarse/fine
//! transfers trivial.

use crate::vec3::Vec3;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default)]
pub struct FastHasher(u64);
impl Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = crate::rng::mix64(i);
    }
}
pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

pub fn cell_count(level: u32) -> usize {
    10 * 4usize.pow(level) + 2
}

pub struct Grid {
    pub level: u32,
    pub pos: Vec<Vec3>,
    pub tris: Vec<[u32; 3]>,
    nbr_off: Vec<u32>,
    nbr: Vec<u32>,
    vtri_off: Vec<u32>,
    vtri: Vec<u32>,
    /// Cell area on the unit sphere (steradians); sums to 4π.
    pub area: Vec<f64>,
    pub lat: Vec<f64>,
    pub lon: Vec<f64>,
    /// Mean angular distance between neighbouring cells (radians).
    pub spacing: f64,
    start_table: Vec<u32>,
}

const TABLE_W: usize = 96;
const TABLE_H: usize = 48;

impl Grid {
    pub fn new(level: u32) -> Grid {
        let t = (1.0 + 5f64.sqrt()) / 2.0;
        let mut pos: Vec<Vec3> = [
            (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
            (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
            (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
        ]
        .iter()
        .map(|&(x, y, z)| Vec3::new(x, y, z).normalized())
        .collect();
        let mut tris: Vec<[u32; 3]> = vec![
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
            [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
            [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];
        // Make every triangle counter-clockwise seen from outside.
        for tri in tris.iter_mut() {
            let (a, b, c) = (pos[tri[0] as usize], pos[tri[1] as usize], pos[tri[2] as usize]);
            if (b - a).cross(c - a).dot(a) < 0.0 {
                tri.swap(1, 2);
            }
        }

        for _ in 0..level {
            let mut mid: FastMap<u64, u32> = FastMap::default();
            mid.reserve(tris.len() * 3 / 2);
            let mut next = Vec::with_capacity(tris.len() * 4);
            let mut midpoint = |a: u32, b: u32, pos: &mut Vec<Vec3>| -> u32 {
                let key = if a < b { (a as u64) << 32 | b as u64 } else { (b as u64) << 32 | a as u64 };
                *mid.entry(key).or_insert_with(|| {
                    pos.push((pos[a as usize] + pos[b as usize]).normalized());
                    (pos.len() - 1) as u32
                })
            };
            for &[a, b, c] in &tris {
                let ab = midpoint(a, b, &mut pos);
                let bc = midpoint(b, c, &mut pos);
                let ca = midpoint(c, a, &mut pos);
                next.push([a, ab, ca]);
                next.push([b, bc, ab]);
                next.push([c, ca, bc]);
                next.push([ab, bc, ca]);
            }
            tris = next;
        }

        let n = pos.len();
        debug_assert_eq!(n, cell_count(level));

        // Neighbours and incident triangles (CSR).
        let mut deg = vec![0u32; n];
        for t in &tris {
            for &v in t {
                deg[v as usize] += 1;
            }
        }
        // In a closed triangle mesh each vertex has as many neighbours as incident triangles.
        let mut off = vec![0u32; n + 1];
        for i in 0..n {
            off[i + 1] = off[i] + deg[i];
        }
        let mut vtri = vec![0u32; off[n] as usize];
        let mut fill = off.clone();
        for (ti, t) in tris.iter().enumerate() {
            for &v in t {
                vtri[fill[v as usize] as usize] = ti as u32;
                fill[v as usize] += 1;
            }
        }
        let mut nbr = vec![u32::MAX; off[n] as usize];
        let mut fill = off.clone();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                for (u, v) in [(a, b), (b, a)] {
                    let (s, e) = (off[u as usize] as usize, fill[u as usize] as usize);
                    if !nbr[s..e].contains(&v) {
                        nbr[e] = v;
                        fill[u as usize] += 1;
                    }
                }
            }
        }

        // Cell areas from the chord triangles, normalised to 4π.
        let mut area = vec![0.0f64; n];
        for t in &tris {
            let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            let ar = (b - a).cross(c - a).len() * 0.5 / 3.0;
            for &v in t {
                area[v as usize] += ar;
            }
        }
        let total: f64 = area.iter().sum();
        let k = 4.0 * std::f64::consts::PI / total;
        area.iter_mut().for_each(|a| *a *= k);

        let mut lat = Vec::with_capacity(n);
        let mut lon = Vec::with_capacity(n);
        for p in &pos {
            let (la, lo) = p.lat_lon();
            lat.push(la);
            lon.push(lo);
        }

        let mut sum = 0.0;
        let mut cnt = 0usize;
        for i in (0..n).step_by((n / 5000).max(1)) {
            for j in off[i]..off[i + 1] {
                sum += pos[i].angle_to(pos[nbr[j as usize] as usize]);
                cnt += 1;
            }
        }

        let mut g = Grid {
            level,
            pos,
            tris,
            nbr_off: off,
            nbr,
            vtri_off: fill_offsets_from(&deg),
            vtri,
            area,
            lat,
            lon,
            spacing: sum / cnt as f64,
            start_table: Vec::new(),
        };
        g.build_start_table();
        g
    }

    pub fn len(&self) -> usize {
        self.pos.len()
    }

    #[inline]
    pub fn neighbors(&self, i: usize) -> &[u32] {
        &self.nbr[self.nbr_off[i] as usize..self.nbr_off[i + 1] as usize]
    }

    #[inline]
    pub fn incident_tris(&self, i: usize) -> &[u32] {
        &self.vtri[self.vtri_off[i] as usize..self.vtri_off[i + 1] as usize]
    }

    /// CSR neighbour arrays (offsets, indices) for export to the UI.
    pub fn neighbor_csr(&self) -> (&[u32], &[u32]) {
        (&self.nbr_off, &self.nbr)
    }

    fn build_start_table(&mut self) {
        let mut table = vec![0u32; TABLE_W * TABLE_H];
        let mut cur = 0usize;
        for y in 0..TABLE_H {
            for x in 0..TABLE_W {
                let (la, lo) = table_center(x, y);
                let p = Vec3::from_lat_lon(la, lo);
                cur = self.walk(p, cur);
                table[y * TABLE_W + x] = cur as u32;
            }
        }
        self.start_table = table;
    }

    #[inline]
    fn walk(&self, p: Vec3, start: usize) -> usize {
        let mut cur = start;
        let mut best_d = self.pos[cur].dot(p);
        loop {
            let mut best = cur;
            for &nb in self.neighbors(cur) {
                let d = self.pos[nb as usize].dot(p);
                if d > best_d {
                    best_d = d;
                    best = nb as usize;
                }
            }
            if best == cur {
                return cur;
            }
            cur = best;
        }
    }

    /// Nearest cell to a unit vector. `hint` (a nearby cell) speeds up coherent queries.
    pub fn nearest(&self, p: Vec3, hint: Option<usize>) -> usize {
        let start = match hint {
            Some(h) => h,
            None => {
                let (la, lo) = p.lat_lon();
                let y = (((la + std::f64::consts::FRAC_PI_2) / std::f64::consts::PI) * TABLE_H as f64)
                    .floor()
                    .clamp(0.0, TABLE_H as f64 - 1.0) as usize;
                let x = (((lo + std::f64::consts::PI) / std::f64::consts::TAU) * TABLE_W as f64)
                    .floor()
                    .clamp(0.0, TABLE_W as f64 - 1.0) as usize;
                self.start_table[y * TABLE_W + x] as usize
            }
        };
        self.walk(p, start)
    }

    /// Triangle containing `p`, with barycentric weights. Returns (nearest cell, corners, weights).
    pub fn locate(&self, p: Vec3, hint: Option<usize>) -> (usize, [u32; 3], [f64; 3]) {
        let v = self.nearest(p, hint);
        let mut best: Option<([u32; 3], [f64; 3])> = None;
        let mut best_min = f64::NEG_INFINITY;
        for &t in self.incident_tris(v) {
            let tri = self.tris[t as usize];
            let (a, b, c) = (self.pos[tri[0] as usize], self.pos[tri[1] as usize], self.pos[tri[2] as usize]);
            let wa = p.dot(b.cross(c));
            let wb = p.dot(c.cross(a));
            let wc = p.dot(a.cross(b));
            let m = wa.min(wb).min(wc);
            if m > best_min {
                best_min = m;
                let s = wa.max(0.0) + wb.max(0.0) + wc.max(0.0);
                let w = if s > 0.0 { [wa.max(0.0) / s, wb.max(0.0) / s, wc.max(0.0) / s] } else { [1.0, 0.0, 0.0] };
                best = Some((tri, w));
            }
        }
        let (tri, w) = best.unwrap_or(([v as u32; 3], [1.0, 0.0, 0.0]));
        (v, tri, w)
    }

    /// Breadth-first collection of all cells within `radius` (radians) of `p`.
    /// Calls `f(cell, angular_distance)`. `scratch` must be all-false and is restored.
    pub fn for_cells_within(&self, p: Vec3, radius: f64, hint: Option<usize>, scratch: &mut Vec<bool>, mut f: impl FnMut(usize, f64)) -> usize {
        if scratch.len() != self.len() {
            *scratch = vec![false; self.len()];
        }
        let start = self.nearest(p, hint);
        let cos_r = (radius + self.spacing * 0.5).cos();
        let mut stack = vec![start];
        let mut seen = vec![start];
        scratch[start] = true;
        while let Some(c) = stack.pop() {
            let d = self.pos[c].angle_to(p);
            if d <= radius {
                f(c, d);
            }
            for &nb in self.neighbors(c) {
                let nb = nb as usize;
                if !scratch[nb] && self.pos[nb].dot(p) >= cos_r {
                    scratch[nb] = true;
                    seen.push(nb);
                    stack.push(nb);
                }
            }
        }
        for s in seen {
            scratch[s] = false;
        }
        start
    }
}

fn fill_offsets_from(deg: &[u32]) -> Vec<u32> {
    let mut off = vec![0u32; deg.len() + 1];
    for i in 0..deg.len() {
        off[i + 1] = off[i] + deg[i];
    }
    off
}

fn table_center(x: usize, y: usize) -> (f64, f64) {
    let la = -std::f64::consts::FRAC_PI_2 + (y as f64 + 0.5) / TABLE_H as f64 * std::f64::consts::PI;
    let lo = -std::f64::consts::PI + (x as f64 + 0.5) / TABLE_W as f64 * std::f64::consts::TAU;
    (la, lo)
}

/// Maps every cell of a fine grid onto a coarse grid of the same hierarchy:
/// nearest coarse cell (for averaging down) and containing coarse triangle
/// (for interpolating up).
pub struct GridTransfer {
    pub nearest: Vec<u32>,
    pub tri: Vec<[u32; 3]>,
    pub w: Vec<[f32; 3]>,
}

impl GridTransfer {
    pub fn new(fine: &Grid, coarse: &Grid) -> GridTransfer {
        use rayon::prelude::*;
        let n = fine.len();
        let res: Vec<(u32, [u32; 3], [f32; 3])> = (0..n)
            .into_par_iter()
            .map(|i| {
                if i < coarse.len() {
                    (i as u32, [i as u32; 3], [1.0, 0.0, 0.0])
                } else {
                    let (v, t, w) = coarse.locate(fine.pos[i], None);
                    (v as u32, t, [w[0] as f32, w[1] as f32, w[2] as f32])
                }
            })
            .collect();
        GridTransfer {
            nearest: res.iter().map(|r| r.0).collect(),
            tri: res.iter().map(|r| r.1).collect(),
            w: res.iter().map(|r| r.2).collect(),
        }
    }

    /// Area-weighted average of a fine field onto the coarse grid.
    pub fn down(&self, fine: &Grid, coarse_len: usize, field: &[f32]) -> Vec<f32> {
        let mut sum = vec![0.0f64; coarse_len];
        let mut wsum = vec![0.0f64; coarse_len];
        for i in 0..field.len() {
            let c = self.nearest[i] as usize;
            sum[c] += field[i] as f64 * fine.area[i];
            wsum[c] += fine.area[i];
        }
        sum.iter().zip(&wsum).map(|(s, w)| if *w > 0.0 { (s / w) as f32 } else { 0.0 }).collect()
    }

    /// Barycentric interpolation of a coarse field onto the fine grid.
    pub fn up(&self, coarse: &[f32]) -> Vec<f32> {
        self.tri
            .iter()
            .zip(&self.w)
            .map(|(t, w)| coarse[t[0] as usize] * w[0] + coarse[t[1] as usize] * w[1] + coarse[t[2] as usize] * w[2])
            .collect()
    }
}
