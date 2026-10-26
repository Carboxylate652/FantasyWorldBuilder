//! Rivers run along cell edges, like Civilization V's: each river segment joins
//! two adjacent cell corners, so it lies on the boundary between the two cells
//! on its banks, where borders are drawn. Hydrology stores a segment per corner
//! (`river_to[t]` is where the segment from corner `t` ends, −1 if none); this
//! turns per-segment values into per-edge and per-cell lookups.

use crate::grid::Grid;

/// How much more a border river's barrier weighs when states and provinces
/// grow across it. A river cell used to be crossed in two costly steps and
/// was costly to travel along, which kept regions on their own bank; an edge
/// is crossed in one step, so it counts double to keep rivers as borders as
/// often as before.
pub const CROSSING_WEIGHT: f32 = 2.0;

/// A value for every cell edge (aligned with the grid's neighbour arrays),
/// taken from the river segment that runs along it (0 off-river).
pub struct RiverEdges {
    value: Vec<f32>,
}

impl RiverEdges {
    /// `value[t]` belongs to the segment from corner `t` to `to[t]`.
    pub fn new(g: &Grid, to: &[i32], value: &[f32]) -> RiverEdges {
        let mut v = vec![0.0f32; g.neighbor_csr().1.len()];
        for t in 0..to.len().min(value.len()) {
            if to[t] < 0 || value[t] == 0.0 {
                continue;
            }
            let (a, b) = g.corner_edge_cells(t, to[t] as usize);
            for (x, y) in [(a, b), (b, a)] {
                if let Some(e) = g.edge_index(x, y) {
                    v[e] = v[e].max(value[t]);
                }
            }
        }
        RiverEdges { value: v }
    }

    /// The value of the river along the edge between neighbouring cells `a` and `b`.
    #[inline]
    pub fn get(&self, g: &Grid, a: usize, b: usize) -> f32 {
        g.edge_index(a, b).map_or(0.0, |e| self.value[e])
    }

    /// Per cell, the largest value of the river edges around it (0 if none):
    /// the cells on a river's banks.
    pub fn per_cell_max(&self, g: &Grid) -> Vec<f32> {
        let (off, _) = g.neighbor_csr();
        (0..g.len()).map(|i| self.value[off[i] as usize..off[i + 1] as usize].iter().cloned().fold(0.0, f32::max)).collect()
    }
}

/// Per cell, the largest of a per-segment value over the river segments on
/// its boundary.
pub fn bank_max<T: Copy + PartialOrd + Default>(g: &Grid, to: &[i32], value: &[T]) -> Vec<T> {
    let mut out = vec![T::default(); g.len()];
    for t in 0..to.len() {
        if to[t] < 0 {
            continue;
        }
        let (a, b) = g.corner_edge_cells(t, to[t] as usize);
        for c in [a, b] {
            if value[t] > out[c] {
                out[c] = value[t];
            }
        }
    }
    out
}
