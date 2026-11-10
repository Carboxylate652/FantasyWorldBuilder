//! Graph searches over the cell adjacency graph.

use crate::grid::Grid;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Copy, PartialEq)]
struct Item {
    cost: f64,
    cell: u32,
    label: u32,
}
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        // Min-heap on cost; ties broken by cell then label so the search is deterministic.
        o.cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| o.cell.cmp(&self.cell))
            .then_with(|| o.label.cmp(&self.label))
    }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

pub struct DijkstraResult {
    pub cost: Vec<f64>,
    /// Label of the source that reached each cell (u32::MAX = unreached).
    pub label: Vec<u32>,
}

/// Multi-source Dijkstra. `edge(from, to, label)` returns the cost of stepping
/// from `from` to `to` for a front with that label, given the precomputed
/// angular edge length, or None if not allowed.
pub fn multi_source(
    grid: &Grid,
    sources: &[(u32, f64, u32)],
    max_cost: f64,
    mut edge: impl FnMut(usize, usize, u32, f64) -> Option<f64>,
) -> DijkstraResult {
    let n = grid.len();
    let mut cost = vec![f64::INFINITY; n];
    let mut label = vec![u32::MAX; n];
    let mut heap = BinaryHeap::with_capacity(sources.len() * 4);
    for &(c, c0, l) in sources {
        if c0 < cost[c as usize] {
            cost[c as usize] = c0;
            label[c as usize] = l;
            heap.push(Item { cost: c0, cell: c, label: l });
        }
    }
    while let Some(Item { cost: c, cell, label: l }) = heap.pop() {
        let ci = cell as usize;
        if c > cost[ci] || label[ci] != l {
            continue;
        }
        let (off, nbr) = grid.neighbor_csr();
        let (len, _) = grid.neighbor_geometry();
        for e in off[ci] as usize..off[ci + 1] as usize {
            let nb = nbr[e];
            let nj = nb as usize;
            if let Some(w) = edge(ci, nj, l, len[e]) {
                let nc = c + w;
                if nc < cost[nj] && nc <= max_cost {
                    cost[nj] = nc;
                    label[nj] = l;
                    heap.push(Item { cost: nc, cell: nb, label: l });
                }
            }
        }
    }
    DijkstraResult { cost, label }
}

/// Geodesic distance (km) from every cell to the nearest cell where `is_source`
/// holds, travelling only through cells where `passable` holds.
pub fn distance_km(grid: &Grid, radius_km: f64, max_km: f64, is_source: impl Fn(usize) -> bool, passable: impl Fn(usize) -> bool) -> Vec<f64> {
    let sources: Vec<(u32, f64, u32)> = (0..grid.len()).filter(|&i| is_source(i)).map(|i| (i as u32, 0.0, 0)).collect();
    multi_source(grid, &sources, max_km, |_a, b, _, edge_len| {
        if passable(b) {
            Some(edge_len * radius_km)
        } else {
            None
        }
    })
    .cost
}

/// Connected components of cells where `member` holds. Returns (component id
/// per cell, u32::MAX for non-members; list of member cells per component).
pub fn components(grid: &Grid, member: impl Fn(usize) -> bool) -> (Vec<u32>, Vec<Vec<u32>>) {
    let n = grid.len();
    let mut comp = vec![u32::MAX; n];
    let mut list: Vec<Vec<u32>> = Vec::new();
    let mut stack = Vec::new();
    for s in 0..n {
        if comp[s] != u32::MAX || !member(s) {
            continue;
        }
        let id = list.len() as u32;
        let mut cells = Vec::new();
        comp[s] = id;
        stack.push(s);
        while let Some(c) = stack.pop() {
            cells.push(c as u32);
            for &nb in grid.neighbors(c) {
                let nb = nb as usize;
                if comp[nb] == u32::MAX && member(nb) {
                    comp[nb] = id;
                    stack.push(nb);
                }
            }
        }
        list.push(cells);
    }
    (comp, list)
}
