//! Transport for Stage 4: roads by quality, railway lines with transfers,
//! sea links and air routes, and the cost of travelling over them.
//!
//! Travel cost is measured in "km of open land on foot": a road divides a
//! border's cost by its speed, a train ride costs a fraction of its length,
//! and every boarding, alighting and change of line costs a fixed penalty
//! (half on boarding, half on alighting), so a trip with one change pays one
//! extra penalty, as passengers and freight lose time at junctions.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Road qualities: 0 none, 1 track, 2 paved road, 3 highway (motor age).
pub const ROAD_NAMES: [&str; 4] = ["none", "track", "paved road", "highway"];
/// Speed over a border with a road of each quality (travel cost is divided by it).
pub const ROAD_SPEED: [f64; 4] = [1.0, 1.6, 2.6, 6.0];
/// Building cost per km (times 1 + barrier), and upkeep per km and year.
pub const ROAD_BUILD: [f64; 4] = [0.0, 25.0, 180.0, 1500.0];
pub const ROAD_UPKEEP: [f64; 4] = [0.0, 0.4, 5.0, 20.0];
/// Railways: track per km (times 1 + barrier), a station, and their upkeep per year.
pub const RAIL_BUILD_KM: f64 = 1000.0;
pub const STATION_BUILD: f64 = 12_000.0;
pub const RAIL_UPKEEP_KM: f64 = 25.0;
pub const STATION_UPKEEP: f64 = 250.0;
/// Airports: building and upkeep per year.
pub const AIRPORT_BUILD: f64 = 300_000.0;
pub const AIRPORT_UPKEEP: f64 = 8_000.0;
/// Travel cost per km by train, by air and by sea, and the fixed costs of a
/// flight and a sea passage (in km of land).
pub const RAIL_FACTOR: f64 = 0.12;
pub const AIR_FACTOR: f64 = 0.05;
pub const AIR_PENALTY: f64 = 300.0;
pub const SEA_FACTOR: f64 = 0.6;
pub const SEA_PENALTY: f64 = 200.0;

/// A railway line: the provinces it runs through, in order, and which of them
/// have a station on it.
#[derive(Clone, Debug)]
pub struct Line {
    pub id: u32,
    /// Nation that built it.
    pub owner: u32,
    pub path: Vec<usize>,
    pub station: Vec<bool>,
    pub opened: i32,
    pub closed: Option<i32>,
    pub km: f64,
}

impl Line {
    pub fn stations(&self) -> impl Iterator<Item = usize> + '_ {
        self.path.iter().zip(&self.station).filter(|x| *x.1).map(|x| *x.0)
    }
    pub fn is_open(&self) -> bool {
        self.closed.is_none()
    }
}

/// Cheapest travel cost from `src` to every reachable province.
///
/// - `land(p, emit)` calls `emit(q, cost)` for each border p can cross;
/// - `lines`: open lines, with `usable(p)` telling which stations may be used
///   (e.g. those on the traveller's own land) and `seg_km(a, b)` the length
///   between two neighbouring provinces on a line;
/// - `links`: extra two-way links (sea passages, flights) as (a, b, cost).
#[allow(clippy::too_many_arguments)]
pub fn travel_costs(
    src: usize,
    land: &dyn Fn(usize, &mut dyn FnMut(usize, f64)),
    lines: &[&Line],
    usable: &dyn Fn(usize) -> bool,
    seg_km: &dyn Fn(usize, usize) -> f64,
    transfer_km: f64,
    links: &[(usize, usize, f64)],
) -> HashMap<usize, f64> {
    // Nodes: provinces (Prov), and a line's station (OnLine).
    #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Node {
        Prov(usize),
        OnLine(usize, usize),
    }
    // Stations per province, and the next station along each line.
    let mut at: HashMap<usize, Vec<(usize, usize)>> = HashMap::new(); // province -> (line, station index)
    let mut stops: Vec<Vec<(usize, f64)>> = Vec::with_capacity(lines.len()); // per line: (province, km from start)
    for (li, l) in lines.iter().enumerate() {
        let mut acc = 0.0;
        let mut v = Vec::new();
        for (k, &p) in l.path.iter().enumerate() {
            if k > 0 {
                acc += seg_km(l.path[k - 1], p);
            }
            if l.station[k] && usable(p) {
                at.entry(p).or_default().push((li, v.len()));
                v.push((p, acc));
            }
        }
        stops.push(v);
    }
    let mut extra: HashMap<usize, Vec<(usize, f64)>> = HashMap::new();
    for &(a, b, c) in links {
        extra.entry(a).or_default().push((b, c));
        extra.entry(b).or_default().push((a, c));
    }
    let mut dist: HashMap<Node, f64> = HashMap::new();
    let mut heap = BinaryHeap::new();
    dist.insert(Node::Prov(src), 0.0);
    heap.push(Reverse((0u64, Node::Prov(src))));
    let half = 0.5 * transfer_km.max(0.0);
    while let Some(Reverse((bits, node))) = heap.pop() {
        let d = f64::from_bits(bits);
        if d > dist[&node] {
            continue;
        }
        let mut next: Vec<(Node, f64)> = Vec::new();
        match node {
            Node::Prov(p) => {
                land(p, &mut |q, c| next.push((Node::Prov(q), c)));
                for &(li, k) in at.get(&p).map(|v| v.as_slice()).unwrap_or(&[]) {
                    next.push((Node::OnLine(li, k), half));
                }
                for &(q, c) in extra.get(&p).map(|v| v.as_slice()).unwrap_or(&[]) {
                    next.push((Node::Prov(q), c));
                }
            }
            Node::OnLine(li, k) => {
                let s = &stops[li];
                next.push((Node::Prov(s[k].0), half));
                if k + 1 < s.len() {
                    next.push((Node::OnLine(li, k + 1), (s[k + 1].1 - s[k].1) * RAIL_FACTOR));
                }
                if k > 0 {
                    next.push((Node::OnLine(li, k - 1), (s[k].1 - s[k - 1].1) * RAIL_FACTOR));
                }
            }
        }
        for (m, c) in next {
            let nd = d + c;
            if dist.get(&m).map_or(true, |&x| nd < x) {
                dist.insert(m, nd);
                heap.push(Reverse((nd.to_bits(), m)));
            }
        }
    }
    dist.into_iter().filter_map(|(k, v)| if let Node::Prov(p) = k { Some((p, v)) } else { None }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: u32, path: Vec<usize>) -> Line {
        let n = path.len();
        Line { id, owner: 1, station: vec![true; n], path, opened: 1850, closed: None, km: 0.0 }
    }

    #[test]
    fn transfers_cost_a_penalty() {
        // Provinces 0..5 in a row, 100 km apart on foot (cost 100 per border).
        let land = |p: usize, emit: &mut dyn FnMut(usize, f64)| {
            if p > 0 {
                emit(p - 1, 100.0);
            }
            if p < 5 {
                emit(p + 1, 100.0);
            }
        };
        let seg = |_: usize, _: usize| 100.0;
        // One line 0..5: 500 km by train = 60, plus boarding and alighting (one penalty).
        let a = line(1, vec![0, 1, 2, 3, 4, 5]);
        let d1 = travel_costs(0, &land, &[&a], &|_| true, &seg, 150.0, &[]);
        assert!((d1[&5] - (500.0 * RAIL_FACTOR + 150.0)).abs() < 1e-9, "{}", d1[&5]);
        // Two lines meeting at 2: one change adds one penalty.
        let (b, c) = (line(2, vec![0, 1, 2]), line(3, vec![2, 3, 4, 5]));
        let d2 = travel_costs(0, &land, &[&b, &c], &|_| true, &seg, 150.0, &[]);
        assert!((d2[&5] - (500.0 * RAIL_FACTOR + 300.0)).abs() < 1e-9, "{}", d2[&5]);
        // A short hop is cheaper on foot than paying the penalty.
        assert_eq!(d2[&1], 100.0);
        // Stations that can't be used (another nation's) break the line.
        let d3 = travel_costs(0, &land, &[&a], &|p| p != 5, &seg, 150.0, &[]);
        assert!((d3[&5] - (400.0 * RAIL_FACTOR + 150.0 + 100.0)).abs() < 1e-9, "{}", d3[&5]);
        // A flight link.
        let d4 = travel_costs(0, &land, &[], &|_| true, &seg, 150.0, &[(0, 5, 80.0)]);
        assert_eq!(d4[&5], 80.0);
        assert_eq!(d4[&4], 180.0);
    }
}
