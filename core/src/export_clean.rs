//! Clean-up of the rasterised province map before it is written.
//!
//! Detail noise makes the exported coastline sharper than the grid, but it also
//! makes islets and ponds that the grid does not have. Each one becomes a
//! stray piece of the nearest province. Province borders are noise-warped too,
//! which leaves a few pixels cut off from their province. This pass:
//!
//! Every grid cell has an anchor: the plus of five pixels at its centre.
//!
//! 1. Islets, ponds and lake specks are turned into the surface around them (the heightmap
//!    follows, so the package stays consistent) unless they hold an anchor of
//!    their own surface and are the largest such piece of a province, or of
//!    their grid landmass or sea (if at least 8 pixels), or are at least the
//!    size of a cell.
//!    Then every land pixel gets a land or wasteland province and every sea
//!    pixel a sea province, taken from a neighbour that already matches.
//! 2. Every stray piece of a province (not its largest) that holds none of its
//!    anchors, or is smaller than two cells, goes to the neighbouring province
//!    of the same surface it touches most. Pieces with no such neighbour are
//!    islands and stay.
//! 3. A province left with fewer than 8 pixels (a one-cell island the noise
//!    sank) gets a disc of about half a cell stamped at its first anchor.
//! 4. "X-crossings", where four provinces (or a checkerboard of two) meet at
//!    one pixel corner, are broken up.
//!
//! Real islands that belong to a province (they hold its anchors) are kept.

use serde::Serialize;
use std::collections::BTreeMap;

/// Surface class of a pixel.
pub const LAND: u8 = 0;
pub const SEA: u8 = 1;
pub const LAKE: u8 = 2;

pub struct ProvRaster {
    pub w: usize,
    pub h: usize,
    /// Province id per pixel.
    pub ids: Vec<u32>,
    /// Surface class per pixel (LAND, SEA, LAKE).
    pub cls: Vec<u8>,
}

/// The centre of a grid cell: its pixel, surface class and province.
#[derive(Clone, Copy)]
pub struct Anchor {
    pub px: usize,
    pub cls: u8,
    pub id: u32,
    /// Connected landmass or water body of the cell on the grid.
    pub body: u32,
}

/// Per row: pixels per grid cell (a cell's area over a pixel's area) and how
/// much a pixel is squeezed east–west (cos latitude, times the x/y pixel ratio).
pub struct RowScale {
    pub cell_px: Vec<f64>,
    pub x_scale: Vec<f64>,
}

pub fn row_scale(w: usize, h: usize, lat_max: f64, lat_min: f64, cells: usize) -> RowScale {
    let cell_sr = 4.0 * std::f64::consts::PI / cells as f64;
    let (dx, dy) = (2.0 * std::f64::consts::PI / w as f64, (lat_max - lat_min).to_radians() / h as f64);
    let cos: Vec<f64> = (0..h).map(|y| (lat_max - (y as f64 + 0.5) / h as f64 * (lat_max - lat_min)).to_radians().cos().max(1e-6)).collect();
    RowScale { cell_px: cos.iter().map(|c| cell_sr / (dx * dy * c)).collect(), x_scale: cos.iter().map(|c| c * dx / dy).collect() }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CleanReport {
    pub islets_removed: usize,
    pub ponds_filled: usize,
    pub surface_pixels_flipped: usize,
    pub specks_merged: usize,
    pub speck_pixels_merged: usize,
    pub kind_pixels_fixed: usize,
    pub x_crossings_fixed: usize,
    pub x_crossings_left: usize,
    pub provinces_stamped: usize,
    pub split_provinces: usize,
}

/// 4-connected neighbours of pixel `p`, wrapping east–west.
#[inline]
fn neighbours(w: usize, h: usize, p: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (p % w, p / w);
    let row = y * w;
    [Some(row + (x + w - 1) % w), Some(row + (x + 1) % w), (y > 0).then(|| p - w), (y + 1 < h).then(|| p + w)].into_iter().flatten()
}

/// Union-find root with path halving.
fn root(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

/// Connected components (4-connected, wrapping east–west) of pixels with equal
/// values. Works on runs of equal pixels along each row, so it costs about one
/// pass over the image. Components are numbered in order of their first pixel.
/// Returns the component of each pixel and the pixel count of each component.
fn components<T: PartialEq + Copy>(w: usize, h: usize, v: &[T]) -> (Vec<u32>, Vec<u32>) {
    // Runs: (row start index into `runs`, x0, x1 exclusive) per row.
    let mut runs: Vec<(u32, u32)> = Vec::new(); // (x0, x1) per run
    let mut row_start = Vec::with_capacity(h + 1);
    for y in 0..h {
        row_start.push(runs.len());
        let row = &v[y * w..(y + 1) * w];
        let mut x0 = 0;
        for x in 1..=w {
            if x == w || row[x] != row[x0] {
                runs.push((x0 as u32, x as u32));
                x0 = x;
            }
        }
    }
    row_start.push(runs.len());
    let mut parent: Vec<u32> = (0..runs.len() as u32).collect();
    let union = |parent: &mut Vec<u32>, a: usize, b: usize| {
        let (ra, rb) = (root(parent, a as u32), root(parent, b as u32));
        if ra != rb {
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            parent[hi as usize] = lo;
        }
    };
    for y in 0..h {
        let (a0, a1) = (row_start[y], row_start[y + 1]);
        // East–west wrap.
        if a1 - a0 > 1 && v[y * w] == v[y * w + w - 1] {
            union(&mut parent, a0, a1 - 1);
        }
        if y == 0 {
            continue;
        }
        // Overlapping runs of the row above with the same value.
        let (b0, b1) = (row_start[y - 1], row_start[y]);
        let mut j = b0;
        for i in a0..a1 {
            let (x0, x1) = runs[i];
            while j < b1 && runs[j].1 <= x0 {
                j += 1;
            }
            let mut k = j;
            while k < b1 && runs[k].0 < x1 {
                if v[(y - 1) * w + runs[k].0 as usize] == v[y * w + x0 as usize] {
                    union(&mut parent, i, k);
                }
                k += 1;
            }
        }
    }
    // Number components in order of their first run (= first pixel), fill pixels.
    let mut id_of_root = vec![u32::MAX; runs.len()];
    let mut sizes: Vec<u32> = Vec::new();
    let mut comp = vec![0u32; w * h];
    for y in 0..h {
        for i in row_start[y]..row_start[y + 1] {
            let r = root(&mut parent, i as u32) as usize;
            if id_of_root[r] == u32::MAX {
                id_of_root[r] = sizes.len() as u32;
                sizes.push(0);
            }
            let id = id_of_root[r];
            let (x0, x1) = runs[i];
            sizes[id as usize] += x1 - x0;
            comp[y * w + x0 as usize..y * w + x1 as usize].fill(id);
        }
    }
    (comp, sizes)
}

/// Marks a province id with no known kind in `kind_of`.
pub const UNKNOWN: u8 = u8::MAX;

/// `kind_of[id]`: surface class of each province (LAND for land and
/// wasteland, UNKNOWN for ids not in the table); every id in the raster must be
/// a valid index.
pub fn clean(r: &mut ProvRaster, anchors: &[Anchor], scale: &RowScale, kind_of: &[u8]) -> CleanReport {
    let cell_px = &scale.cell_px;
    let (w, h) = (r.w, r.h);
    let mut rep = CleanReport::default();

    let plus = |p: usize| std::iter::once(p).chain(neighbours(w, h, p));

    // ---- 1. islets and ponds that hold no anchor. Outside in: a piece is
    // flipped only once it touches a piece that stays, so a pond inside a
    // noise islet joins the sea with it instead of swapping with it.
    for _ in 0..8 {
        let cls = &r.cls;
        let (comp, sizes) = components(w, h, cls);
        let mut backed = vec![false; sizes.len()];
        let mut row = vec![0usize; sizes.len()];
        for p in 0..w * h {
            row[comp[p] as usize] = p / w;
        }
        // Largest raster piece holding an anchor of each grid body, and of each province.
        let mut best: BTreeMap<(u8, u32), u32> = BTreeMap::new();
        let mut best_prov: BTreeMap<u32, u32> = BTreeMap::new();
        for a in anchors {
            for q in plus(a.px) {
                if r.cls[q] == a.cls {
                    let k = comp[q];
                    let bigger = |e: u32| (sizes[k as usize], u32::MAX - k) > (sizes[e as usize], u32::MAX - e);
                    let e = best.entry((a.cls, a.body)).or_insert(k);
                    if bigger(*e) {
                        *e = k;
                    }
                    let e = best_prov.entry(a.id).or_insert(k);
                    if bigger(*e) {
                        *e = k;
                    }
                    if sizes[k as usize] as f64 >= cell_px[row[k as usize]] {
                        backed[k as usize] = true;
                    }
                }
            }
        }
        for &k in best.values().filter(|&&k| sizes[k as usize] >= 8).chain(best_prov.values()) {
            backed[k as usize] = true;
        }
        // Flip each unbacked component to the surface around it, taking the
        // province it touches most on that surface.
        let mut touch: Vec<BTreeMap<(u8, u32), u32>> = vec![BTreeMap::new(); sizes.len()];
        let mut outer = vec![false; sizes.len()];
        for p in 0..w * h {
            let k = comp[p] as usize;
            if backed[k] {
                continue;
            }
            for q in neighbours(w, h, p) {
                if r.cls[q] != r.cls[p] {
                    *touch[k].entry((r.cls[q], r.ids[q])).or_insert(0) += 1;
                    outer[k] |= backed[comp[q] as usize];
                }
            }
        }
        if !outer.iter().zip(&backed).any(|(&o, &b)| o && !b) {
            break;
        }
        let target: Vec<Option<(u8, u32)>> = touch.iter().map(|t| t.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(&k, _)| k)).collect();
        let mut counted = vec![false; sizes.len()];
        for p in 0..w * h {
            let k = comp[p] as usize;
            if backed[k] || !outer[k] {
                continue;
            }
            let Some((c, id)) = target[k] else { continue };
            if !counted[k] {
                counted[k] = true;
                if r.cls[p] == LAND {
                    rep.islets_removed += 1;
                } else {
                    rep.ponds_filled += 1;
                }
            }
            r.cls[p] = c;
            r.ids[p] = id;
            rep.surface_pixels_flipped += 1;
        }
    }

    // ---- 1b. province kind matches the surface (lakes may sit in land provinces)
    let fits = |r: &ProvRaster, p: usize| -> bool {
        match (r.cls[p], kind_of[r.ids[p] as usize]) {
            (_, UNKNOWN) => true,
            (LAKE, k) => k != SEA,
            (c, k) => c == k,
        }
    };
    let mut bad: Vec<usize> = (0..w * h).filter(|&p| !fits(r, p)).collect();
    for _ in 0..64 {
        let before = bad.len();
        let mut still = Vec::new();
        for &p in &bad {
            let pick = neighbours(w, h, p).find(|&q| r.cls[q] == r.cls[p] && fits(r, q)).map(|q| r.ids[q]);
            match pick {
                Some(id) => {
                    r.ids[p] = id;
                    rep.kind_pixels_fixed += 1;
                }
                None => still.push(p),
            }
        }
        bad = still;
        if bad.is_empty() || bad.len() == before {
            break;
        }
    }

    // ---- 2. stray pieces of a province
    for _ in 0..3 {
        let ids = &r.ids;
        let (comp, sizes) = components(w, h, ids);
        let mut main: BTreeMap<u32, (u32, u32)> = BTreeMap::new(); // id -> (size, comp)
        let mut first = vec![usize::MAX; sizes.len()];
        for p in 0..w * h {
            let k = comp[p] as usize;
            if first[k] == usize::MAX {
                first[k] = p;
                let e = main.entry(r.ids[p]).or_insert((0, u32::MAX));
                if sizes[k] > e.0 {
                    *e = (sizes[k], k as u32);
                }
            }
        }
        // A piece is stray if it is not the main piece and holds none of its
        // province's anchors or is smaller than two cells.
        let mut has_anchor = vec![false; sizes.len()];
        for a in anchors {
            for q in plus(a.px) {
                if r.ids[q] == a.id {
                    has_anchor[comp[q] as usize] = true;
                }
            }
        }
        let small: Vec<bool> = (0..sizes.len())
            .map(|k| main[&r.ids[first[k]]].1 != k as u32 && (!has_anchor[k] || (sizes[k] as f64) < 2.0 * cell_px[first[k] / w]))
            .collect();
        let mut touch: BTreeMap<u32, BTreeMap<u32, u32>> = BTreeMap::new();
        for p in 0..w * h {
            let k = comp[p] as usize;
            if !small[k] {
                continue;
            }
            for q in neighbours(w, h, p) {
                if r.ids[q] != r.ids[p] && r.cls[q] == r.cls[p] {
                    *touch.entry(k as u32).or_default().entry(r.ids[q]).or_insert(0) += 1;
                }
            }
        }
        if touch.is_empty() {
            break;
        }
        let mut target = vec![u32::MAX; sizes.len()];
        for (&k, t) in &touch {
            if let Some((&id, _)) = t.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))) {
                target[k as usize] = id;
                rep.specks_merged += 1;
            }
        }
        for p in 0..w * h {
            let id = target[comp[p] as usize];
            if id != u32::MAX {
                r.ids[p] = id;
                rep.speck_pixels_merged += 1;
            }
        }
    }

    // ---- 3. provinces with fewer than 8 pixels get a disc at their first anchor
    let mut count = vec![0u32; kind_of.len()];
    for &id in &r.ids {
        count[id as usize] += 1;
    }
    let low = |id: u32| count[id as usize] < 8;
    // Their few leftover pixels go to a neighbour first, so the disc is their only piece.
    let mut stamped = vec![false; kind_of.len()];
    for a in anchors {
        stamped[a.id as usize] |= low(a.id);
    }
    for p in 0..w * h {
        if stamped[r.ids[p] as usize] {
            if let Some(q) = neighbours(w, h, p).find(|&q| !stamped[r.ids[q] as usize]) {
                r.ids[p] = r.ids[q];
                r.cls[p] = r.cls[q];
            }
        }
    }
    let mut done = std::collections::HashSet::new();
    for a in anchors {
        if !stamped[a.id as usize] || !done.insert(a.id) {
            continue;
        }
        let (ax, ay) = ((a.px % w) as i64, (a.px / w) as i64);
        // Radius in rows for an area of about half a cell (at least 2 px).
        let ry = (0.5 * cell_px[ay as usize] * scale.x_scale[ay as usize] / std::f64::consts::PI).sqrt().max(2.0);
        let rx = (ry / scale.x_scale[ay as usize]).min(w as f64 / 4.0);
        for dy in -(ry.ceil() as i64)..=ry.ceil() as i64 {
            let y = ay + dy;
            if y < 0 || y >= h as i64 {
                continue;
            }
            for dx in -(rx.ceil() as i64)..=rx.ceil() as i64 {
                if (dx as f64 / rx).powi(2) + (dy as f64 / ry).powi(2) <= 1.0 {
                    let q = y as usize * w + (ax + dx).rem_euclid(w as i64) as usize;
                    r.cls[q] = a.cls;
                    r.ids[q] = a.id;
                }
            }
        }
        rep.provinces_stamped += 1;
    }

    // ---- 4. X-crossings: repaint one pixel of the 2×2 block with a
    // neighbour of the same surface, so only three provinces meet there.
    let x_at = |r: &ProvRaster, x: usize, y: usize| -> Option<[usize; 4]> {
        let x1 = (x + 1) % w;
        let q = [y * w + x, y * w + x1, (y + 1) * w + x, (y + 1) * w + x1];
        let [a, b, c, d] = q.map(|p| r.ids[p]);
        let four = a != b && a != c && a != d && b != c && b != d && c != d;
        let checker = a == d && b == c && a != b;
        (four || checker).then_some(q)
    };
    for y in 0..h.saturating_sub(1) {
        for x in 0..w {
            let Some(q) = x_at(r, x, y) else { continue };
            // Try each pixel of the block in turn, copying a 4-neighbour inside the block.
            // Same surface first; failing that (land and sea meeting
            // diagonally), the pixel takes the neighbour's surface too.
            let pairs = [(3, [1, 2]), (0, [1, 2]), (1, [0, 3]), (2, [0, 3])];
            let mut fixed = false;
            'try_: for same_surface in [true, false] {
                for (t, srcs) in pairs {
                    for s in srcs {
                        if (r.cls[q[s]] == r.cls[q[t]]) == same_surface && r.ids[q[s]] != r.ids[q[t]] {
                            let (old, old_cls) = (r.ids[q[t]], r.cls[q[t]]);
                            r.ids[q[t]] = r.ids[q[s]];
                            r.cls[q[t]] = r.cls[q[s]];
                            // Keep the move only if it clears the crossing and the old
                            // province still has a pixel next to this one (never erase a province).
                            let still = neighbours(w, h, q[t]).any(|n| r.ids[n] == old);
                            if still && x_at(r, x, y).is_none() {
                                fixed = true;
                                break 'try_;
                            }
                            r.ids[q[t]] = old;
                            r.cls[q[t]] = old_cls;
                        }
                    }
                }
            }
            if fixed {
                rep.x_crossings_fixed += 1;
            }
        }
    }
    for y in 0..h.saturating_sub(1) {
        for x in 0..w {
            if x_at(r, x, y).is_some() {
                rep.x_crossings_left += 1;
            }
        }
    }

    // ---- what is left
    let ids = &r.ids;
    let (comp, sizes) = components(w, h, ids);
    let mut pieces: BTreeMap<u32, u32> = BTreeMap::new();
    let mut seen = vec![false; sizes.len()];
    for p in 0..w * h {
        let k = comp[p] as usize;
        if !seen[k] {
            seen[k] = true;
            *pieces.entry(r.ids[p]).or_insert(0) += 1;
        }
    }
    rep.split_provinces = pieces.values().filter(|&&c| c > 1).count();
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 24×12 raster: sea province 9 everywhere, land province 1 on the left.
    fn base() -> ProvRaster {
        let (w, h) = (24, 12);
        let mut r = ProvRaster { w, h, ids: vec![9; w * h], cls: vec![SEA; w * h] };
        for y in 0..h {
            for x in 0..8 {
                r.ids[y * w + x] = 1;
                r.cls[y * w + x] = LAND;
            }
        }
        r
    }
    fn anchors_for(r: &ProvRaster) -> Vec<Anchor> {
        // One anchor in the middle of the land and one in the sea.
        vec![Anchor { px: 6 * r.w + 3, cls: LAND, id: 1, body: 0 }, Anchor { px: 6 * r.w + 16, cls: SEA, id: 9, body: 0 }]
    }
    fn scale(r: &ProvRaster) -> RowScale {
        // Large cells, so nothing below counts as "two cells in size".
        RowScale { cell_px: vec![1000.0; r.h], x_scale: vec![1.0; r.h] }
    }
    fn kinds() -> Vec<u8> {
        let mut k = vec![UNKNOWN; 10];
        k[1] = LAND;
        k[2] = LAND;
        k[3] = LAND;
        k[9] = SEA;
        k
    }

    #[test]
    fn noise_islet_and_its_pond_become_sea() {
        let mut r = base();
        let w = r.w;
        // 3×3 islet of province 1 out at sea, with a one-pixel pond in its middle.
        for y in 4..7 {
            for x in 14..17 {
                r.ids[y * w + x] = 1;
                r.cls[y * w + x] = LAND;
            }
        }
        r.cls[5 * w + 15] = SEA;
        r.ids[5 * w + 15] = 9;
        let a = vec![Anchor { px: 6 * w + 3, cls: LAND, id: 1, body: 0 }, Anchor { px: 10 * w + 20, cls: SEA, id: 9, body: 0 }];
        let sc = scale(&r);
        let rep = clean(&mut r, &a, &sc, &kinds());
        assert!((14..17).all(|x| (4..7).all(|y| r.cls[y * w + x] == SEA && r.ids[y * w + x] == 9)));
        assert_eq!(rep.split_provinces, 0);
    }

    #[test]
    fn island_with_an_anchor_stays() {
        let mut r = base();
        let w = r.w;
        for y in 4..7 {
            for x in 14..17 {
                r.ids[y * w + x] = 2;
                r.cls[y * w + x] = LAND;
            }
        }
        let mut a = anchors_for(&r);
        a[1].px = 10 * w + 20;
        a.push(Anchor { px: 5 * w + 15, cls: LAND, id: 2, body: 1 });
        let sc = scale(&r);
        clean(&mut r, &a, &sc, &kinds());
        assert!((14..17).all(|x| (4..7).all(|y| r.ids[y * w + x] == 2 && r.cls[y * w + x] == LAND)));
    }

    #[test]
    fn stray_speck_joins_the_province_around_it() {
        let mut r = base();
        let w = r.w;
        // Province 2 owns the top half of the land; a speck of 2 sits inside 1.
        for y in 0..6 {
            for x in 0..8 {
                r.ids[y * w + x] = 2;
            }
        }
        r.ids[9 * w + 4] = 2;
        let mut a = anchors_for(&r);
        a.push(Anchor { px: 2 * w + 3, cls: LAND, id: 2, body: 0 });
        let sc = scale(&r);
        let rep = clean(&mut r, &a, &sc, &kinds());
        assert_eq!(r.ids[9 * w + 4], 1);
        assert_eq!(rep.split_provinces, 0);
    }

    #[test]
    fn x_crossing_is_broken_up() {
        let mut r = base();
        let w = r.w;
        // Four land provinces meet at the corner between (3,5) and (4,6).
        for y in 0..12 {
            for x in 0..8 {
                r.ids[y * w + x] = match (x < 4, y < 6) {
                    (true, true) => 1,
                    (false, true) => 2,
                    (true, false) => 3,
                    (false, false) => 4,
                };
            }
        }
        let mut k = kinds();
        k.resize(10, UNKNOWN);
        k[4] = LAND;
        let a = vec![
            Anchor { px: 2 * w + 1, cls: LAND, id: 1, body: 0 },
            Anchor { px: 2 * w + 6, cls: LAND, id: 2, body: 0 },
            Anchor { px: 9 * w + 1, cls: LAND, id: 3, body: 0 },
            Anchor { px: 9 * w + 6, cls: LAND, id: 4, body: 0 },
            Anchor { px: 6 * w + 16, cls: SEA, id: 9, body: 0 },
        ];
        let sc = scale(&r);
        let rep = clean(&mut r, &a, &sc, &k);
        assert_eq!(rep.x_crossings_left, 0);
        assert_eq!(rep.x_crossings_fixed, 1);
    }

    #[test]
    fn province_with_no_pixels_is_stamped() {
        let mut r = base();
        let w = r.w;
        let mut a = anchors_for(&r);
        a.push(Anchor { px: 6 * w + 20, cls: LAND, id: 3, body: 2 });
        let sc = RowScale { cell_px: vec![40.0; r.h], x_scale: vec![1.0; r.h] };
        let rep = clean(&mut r, &a, &sc, &kinds());
        assert_eq!(rep.provinces_stamped, 1);
        assert!(r.ids.iter().filter(|&&id| id == 3).count() >= 8);
        assert_eq!(r.cls[6 * w + 20], LAND);
    }

    #[test]
    fn components_wrap_east_west() {
        // Value 1 at both ends of each row, 0 in the middle: one component of 1s.
        let (w, h) = (6, 2);
        let v = [1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1];
        let (comp, sizes) = components(w, h, &v);
        assert_eq!(sizes.len(), 2);
        assert_eq!(comp[0], comp[5]);
        assert_eq!(comp[0], comp[11]);
        assert_eq!(sizes[comp[0] as usize], 4);
    }
}
