//! Community detection on weighted graphs: the Louvain method with a
//! resolution parameter (Blondel et al. 2008; Reichardt & Bornholdt 2006).
//!
//! Each level moves nodes one at a time to the neighbouring community with the
//! largest modularity gain, until no move helps; then every community becomes
//! one node and the next level starts. Higher resolution gives more, smaller
//! communities. Node order is fixed, so results are deterministic.

/// Communities of an undirected weighted graph with `n` nodes. `edges` are
/// (a, b, weight) with weight > 0; pairs may repeat (weights add) and a == b
/// is a self-loop. Returns a community per node, numbered 0.. in order of
/// first appearance.
pub fn louvain(n: usize, edges: &[(u32, u32, f64)], resolution: f64) -> Vec<u32> {
    let mut node_comm: Vec<u32> = (0..n as u32).collect();
    let mut g = Graph::new(n, edges);
    loop {
        let (mut local, moved) = g.local_moving(resolution);
        if !moved {
            break;
        }
        let k = renumber(&mut local);
        for c in node_comm.iter_mut() {
            *c = local[*c as usize];
        }
        if k == g.n {
            break;
        }
        g = g.aggregate(&local, k);
    }
    renumber(&mut node_comm);
    node_comm
}

/// Renumber labels 0.. in order of first appearance; returns the count.
pub fn renumber(labels: &mut [u32]) -> usize {
    let max = labels.iter().copied().max().map_or(0, |m| m as usize + 1);
    let mut map = vec![u32::MAX; max];
    let mut next = 0u32;
    for l in labels.iter_mut() {
        let m = &mut map[*l as usize];
        if *m == u32::MAX {
            *m = next;
            next += 1;
        }
        *l = *m;
    }
    next as usize
}

/// Compressed adjacency with separate self-loop weights.
struct Graph {
    n: usize,
    start: Vec<usize>,
    nbr: Vec<(u32, f64)>,
    selfw: Vec<f64>,
    /// Weighted degree (self-loops count twice).
    deg: Vec<f64>,
    m2: f64,
}

impl Graph {
    fn new(n: usize, edges: &[(u32, u32, f64)]) -> Graph {
        let mut lists: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
        let mut selfw = vec![0.0; n];
        for &(a, b, w) in edges {
            if w <= 0.0 {
                continue;
            }
            if a == b {
                selfw[a as usize] += w;
            } else {
                lists[a as usize].push((b, w));
                lists[b as usize].push((a, w));
            }
        }
        // Merge repeated pairs.
        for l in lists.iter_mut() {
            l.sort_by_key(|x| x.0);
            l.dedup_by(|x, y| {
                if x.0 == y.0 {
                    y.1 += x.1;
                    true
                } else {
                    false
                }
            });
        }
        let mut start = Vec::with_capacity(n + 1);
        let mut nbr = Vec::new();
        let mut deg = vec![0.0; n];
        for (i, l) in lists.into_iter().enumerate() {
            start.push(nbr.len());
            deg[i] = 2.0 * selfw[i] + l.iter().map(|x| x.1).sum::<f64>();
            nbr.extend(l);
        }
        start.push(nbr.len());
        let m2 = deg.iter().sum();
        Graph { n, start, nbr, selfw, deg, m2 }
    }

    fn neighbours(&self, i: usize) -> &[(u32, f64)] {
        &self.nbr[self.start[i]..self.start[i + 1]]
    }

    /// One level of local moving. Returns the community of each node and
    /// whether any node moved.
    fn local_moving(&self, resolution: f64) -> (Vec<u32>, bool) {
        let n = self.n;
        let mut comm: Vec<u32> = (0..n as u32).collect();
        let mut tot: Vec<f64> = self.deg.clone();
        if self.m2 <= 0.0 {
            return (comm, false);
        }
        let mut link = vec![0.0f64; n];
        let mut touched: Vec<u32> = Vec::new();
        let mut moved_any = false;
        for _pass in 0..32 {
            let mut moved = false;
            for i in 0..n {
                let ci = comm[i];
                let ki = self.deg[i];
                // Weights from i to each neighbouring community.
                for &(j, w) in self.neighbours(i) {
                    let cj = comm[j as usize];
                    if link[cj as usize] == 0.0 {
                        touched.push(cj);
                    }
                    link[cj as usize] += w;
                }
                tot[ci as usize] -= ki;
                // Gain of joining community c (relative to staying alone).
                let gain = |c: u32, link: &[f64], tot: &[f64]| link[c as usize] - resolution * tot[c as usize] * ki / self.m2;
                let mut best = ci;
                let mut best_gain = gain(ci, &link, &tot);
                for &c in &touched {
                    let g = gain(c, &link, &tot);
                    // Ties go to the lower community id.
                    if g > best_gain + 1e-12 || ((g - best_gain).abs() <= 1e-12 && c < best) {
                        best = c;
                        best_gain = g;
                    }
                }
                tot[best as usize] += ki;
                if best != ci {
                    comm[i] = best;
                    moved = true;
                    moved_any = true;
                }
                for &c in &touched {
                    link[c as usize] = 0.0;
                }
                touched.clear();
            }
            if !moved {
                break;
            }
        }
        (comm, moved_any)
    }

    /// Graph of communities: internal weight becomes a self-loop.
    fn aggregate(&self, comm: &[u32], k: usize) -> Graph {
        let mut edges: Vec<(u32, u32, f64)> = Vec::new();
        for i in 0..self.n {
            let ci = comm[i];
            if self.selfw[i] > 0.0 {
                edges.push((ci, ci, self.selfw[i]));
            }
            for &(j, w) in self.neighbours(i) {
                if (j as usize) > i {
                    let cj = comm[j as usize];
                    edges.push((ci, cj, w));
                }
            }
        }
        Graph::new(k, &edges)
    }
}

/// Modularity of a partition at the given resolution (for tests and reports).
pub fn modularity(n: usize, edges: &[(u32, u32, f64)], comm: &[u32], resolution: f64) -> f64 {
    let g = Graph::new(n, edges);
    if g.m2 <= 0.0 {
        return 0.0;
    }
    let k = comm.iter().copied().max().map_or(0, |m| m as usize + 1);
    let mut inside = vec![0.0; k];
    let mut tot = vec![0.0; k];
    for i in 0..n {
        let c = comm[i] as usize;
        tot[c] += g.deg[i];
        inside[c] += 2.0 * g.selfw[i];
        for &(j, w) in g.neighbours(i) {
            if comm[j as usize] as usize == c {
                inside[c] += w;
            }
        }
    }
    (0..k).map(|c| inside[c] / g.m2 - resolution * (tot[c] / g.m2).powi(2)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two 5-cliques joined by one weak edge split into two communities.
    #[test]
    fn two_cliques() {
        let mut e = Vec::new();
        for base in [0u32, 5] {
            for a in 0..5 {
                for b in a + 1..5 {
                    e.push((base + a, base + b, 1.0));
                }
            }
        }
        e.push((4, 5, 0.1));
        let c = louvain(10, &e, 1.0);
        assert!((0..5).all(|i| c[i] == c[0]));
        assert!((5..10).all(|i| c[i] == c[5]));
        assert_ne!(c[0], c[5]);
        assert!(modularity(10, &e, &c, 1.0) > 0.4);
    }

    /// A ring of eight 4-cliques: high resolution keeps them apart, low merges them.
    #[test]
    fn resolution_controls_size() {
        let mut e = Vec::new();
        for k in 0..8u32 {
            let b = 4 * k;
            for x in 0..4 {
                for y in x + 1..4 {
                    e.push((b + x, b + y, 1.0));
                }
            }
            e.push((b + 3, (b + 4) % 32, 1.0));
        }
        let fine = louvain(32, &e, 1.0);
        let coarse = louvain(32, &e, 0.05);
        let count = |c: &[u32]| c.iter().copied().max().unwrap() as usize + 1;
        assert_eq!(count(&fine), 8);
        assert!(count(&coarse) < 8);
    }

    #[test]
    fn isolated_nodes_stay_alone() {
        let c = louvain(3, &[(0, 1, 1.0)], 1.0);
        assert_eq!(c[0], c[1]);
        assert_ne!(c[0], c[2]);
    }
}
