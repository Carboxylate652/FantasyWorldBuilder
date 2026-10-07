//! Re-import of an edited provinces.png (+ definition.csv).
//!
//! Before anything changes, the files are checked:
//! - errors (block the import): duplicate colours or ids in the CSV, colours in
//!   the image that are missing from the CSV, and blended edge colours left by
//!   anti-aliasing brushes;
//! - warnings: CSV rows with no pixels, provinces split into disconnected
//!   pieces, provinces below a minimum pixel count, and "X-crossings" where four
//!   provinces meet at one pixel corner.
//!
//! Then every cell takes its province by majority pixel vote.

use crate::edits::ProvinceImport;
use crate::grid::Grid;
use crate::hash;
use crate::vec3::Vec3;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize)]
pub struct Def {
    pub id: u32,
    pub rgb: [u8; 3],
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub ok: bool,
    pub width: usize,
    pub height: usize,
    pub provinces: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Decoded image as one palette index per pixel (into `colors`).
pub struct Indexed {
    pub width: usize,
    pub height: usize,
    pub colors: Vec<[u8; 3]>,
    pub px: Vec<u32>,
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

pub fn read_png(path: &str) -> Result<Indexed, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| format!("{path}: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{path}: {e}"))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = info.color_type.samples();
    if ch < 3 {
        return Err(format!("{path}: provinces.png must be an RGB image (found {ch} channel(s))"));
    }
    let mut index: HashMap<u32, u32> = HashMap::new();
    let mut colors = Vec::new();
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = &buf[y * info.line_size..];
        for x in 0..w {
            let o = x * ch;
            let c = [row[o], row[o + 1], row[o + 2]];
            let key = (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32;
            let k = *index.entry(key).or_insert_with(|| {
                colors.push(c);
                colors.len() as u32 - 1
            });
            px.push(k);
        }
    }
    Ok(Indexed { width: w, height: h, colors, px })
}

/// definition.csv: `id;r;g;b;name;...` (CK3 / Victoria 3). The `0;0;0;0;x;x;`
/// header row and comment lines are skipped.
pub fn read_definition(path: &str) -> Result<Vec<Def>, String> {
    let text = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let text = String::from_utf8_lossy(&text);
    let mut defs = Vec::new();
    for (ln, line) in text.lines().enumerate() {
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split(';').collect();
        if cols.len() < 4 {
            return Err(format!("{path} line {}: expected id;r;g;b;name", ln + 1));
        }
        let Ok(id) = cols[0].trim().parse::<u32>() else {
            if ln == 0 {
                continue; // a text header
            }
            return Err(format!("{path} line {}: bad id `{}`", ln + 1, cols[0]));
        };
        if id == 0 {
            continue;
        }
        let ch = |k: usize| cols[k].trim().parse::<u8>().map_err(|_| format!("{path} line {}: bad colour value `{}`", ln + 1, cols[k]));
        defs.push(Def { id, rgb: [ch(1)?, ch(2)?, ch(3)?], name: cols.get(4).map(|s| s.trim().to_string()).unwrap_or_default() });
    }
    Ok(defs)
}

/// Check an image and its definitions. Returns the report and the definition
/// for each image colour (`None` when it has no CSV row).
pub fn validate(img: &Indexed, defs: Option<&[Def]>, min_pixels: usize) -> (Report, Vec<Option<Def>>) {
    let (w, h) = (img.width, img.height);
    let mut rep = Report { width: w, height: h, ..Default::default() };
    let nc = img.colors.len();
    let mut count = vec![0usize; nc];
    for &k in &img.px {
        count[k as usize] += 1;
    }

    // Definitions: duplicates, and the definition for each image colour.
    let mut by_color: Vec<Option<Def>> = vec![None; nc];
    match defs {
        Some(defs) => {
            let mut seen_c: HashMap<[u8; 3], u32> = HashMap::new();
            let mut seen_id: HashMap<u32, ()> = HashMap::new();
            for d in defs {
                if let Some(other) = seen_c.insert(d.rgb, d.id) {
                    rep.errors.push(format!("Colour {} is used by provinces {other} and {}", hex(d.rgb), d.id));
                }
                if seen_id.insert(d.id, ()).is_some() {
                    rep.errors.push(format!("Province id {} appears twice in definition.csv", d.id));
                }
            }
            let lookup: HashMap<[u8; 3], &Def> = defs.iter().map(|d| (d.rgb, d)).collect();
            for (k, c) in img.colors.iter().enumerate() {
                by_color[k] = lookup.get(c).map(|d| (*d).clone());
            }
            let present: std::collections::HashSet<[u8; 3]> = img.colors.iter().copied().collect();
            let unused: Vec<String> = defs.iter().filter(|d| !present.contains(&d.rgb)).take(12).map(|d| d.id.to_string()).collect();
            let n_unused = defs.iter().filter(|d| !present.contains(&d.rgb)).count();
            if n_unused > 0 {
                rep.warnings.push(format!("{n_unused} definition.csv rows have no pixels (ids {}{})", unused.join(", "), if n_unused > 12 { ", …" } else { "" }));
            }
        }
        None => {
            // No CSV: every colour is a province, numbered by colour value.
            let mut order: Vec<usize> = (0..nc).collect();
            order.sort_by_key(|&k| img.colors[k]);
            for (id, &k) in order.iter().enumerate() {
                by_color[k] = Some(Def { id: id as u32 + 1, rgb: img.colors[k], name: String::new() });
            }
        }
    }

    // Unknown colours: anti-aliasing blends between neighbours, or missing rows.
    let at = |x: usize, y: usize| img.px[y * w + x] as usize;
    let mut blended = 0usize;
    let mut blend_at = None;
    let mut missing: HashMap<usize, (usize, (usize, usize))> = HashMap::new();
    for y in 0..h {
        for x in 0..w {
            let k = at(x, y);
            if by_color[k].is_some() {
                continue;
            }
            let c = img.colors[k];
            let nbs = [((x + w - 1) % w, y), ((x + 1) % w, y), (x, y.saturating_sub(1)), (x, (y + 1).min(h - 1))];
            let known: Vec<[u8; 3]> = nbs.iter().map(|&(a, b)| at(a, b)).filter(|&j| by_color[j].is_some()).map(|j| img.colors[j]).collect();
            let mut is_blend = false;
            'pairs: for i in 0..known.len() {
                for j in i + 1..known.len() {
                    let (a, b) = (known[i], known[j]);
                    if a != b && between(c, a, b) {
                        is_blend = true;
                        break 'pairs;
                    }
                }
            }
            if is_blend {
                blended += 1;
                blend_at.get_or_insert((x, y));
            } else {
                missing.entry(k).or_insert((0, (x, y))).0 += 1;
            }
        }
    }
    if blended > 0 {
        let (x, y) = blend_at.unwrap();
        rep.errors.push(format!("{blended} pixels look like anti-aliased blends between two provinces (first at x={x}, y={y}); paint with a hard-edged pencil"));
    }
    if !missing.is_empty() {
        let mut v: Vec<(usize, (usize, (usize, usize)))> = missing.into_iter().collect();
        v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
        let total = v.len();
        let list: Vec<String> = v.iter().take(8).map(|(k, (n, (x, y)))| format!("{} ({n} px, first at x={x}, y={y})", hex(img.colors[*k]))).collect();
        rep.errors.push(format!("{total} colours are not in definition.csv: {}{}", list.join("; "), if total > 8 { "; …" } else { "" }));
    }

    // Pieces per province (4-connected, wrapping east–west) and tiny provinces.
    let mut seen = vec![false; w * h];
    let mut pieces = vec![0u32; nc];
    let mut stack = Vec::new();
    for s in 0..w * h {
        if seen[s] {
            continue;
        }
        let k = img.px[s];
        pieces[k as usize] += 1;
        seen[s] = true;
        stack.push(s);
        while let Some(p) = stack.pop() {
            let (x, y) = (p % w, p / w);
            let mut visit = |q: usize| {
                if !seen[q] && img.px[q] == k {
                    seen[q] = true;
                    stack.push(q);
                }
            };
            visit(y * w + (x + w - 1) % w);
            visit(y * w + (x + 1) % w);
            if y > 0 {
                visit(p - w);
            }
            if y + 1 < h {
                visit(p + w);
            }
        }
    }
    let split: Vec<String> = (0..nc)
        .filter(|&k| pieces[k] > 1 && by_color[k].is_some())
        .map(|k| format!("{} ({} pieces)", by_color[k].as_ref().unwrap().id, pieces[k]))
        .collect();
    if !split.is_empty() {
        rep.warnings.push(format!(
            "{} provinces are split into disconnected pieces: {}{}",
            split.len(),
            split.iter().take(10).cloned().collect::<Vec<_>>().join(", "),
            if split.len() > 10 { ", …" } else { "" }
        ));
    }
    let tiny: Vec<String> = (0..nc)
        .filter(|&k| count[k] < min_pixels && by_color[k].is_some())
        .map(|k| format!("{} ({} px)", by_color[k].as_ref().unwrap().id, count[k]))
        .collect();
    if !tiny.is_empty() {
        rep.warnings.push(format!(
            "{} provinces have fewer than {min_pixels} pixels: {}{}",
            tiny.len(),
            tiny.iter().take(10).cloned().collect::<Vec<_>>().join(", "),
            if tiny.len() > 10 { ", …" } else { "" }
        ));
    }

    // X-crossings: four different provinces around one pixel corner, or a
    // checkerboard of two.
    let mut xs = 0usize;
    let mut x_at = None;
    for y in 0..h.saturating_sub(1) {
        for x in 0..w {
            let x1 = (x + 1) % w;
            let (a, b, c, d) = (at(x, y), at(x1, y), at(x, y + 1), at(x1, y + 1));
            let four = a != b && a != c && a != d && b != c && b != d && c != d;
            let checker = a == d && b == c && a != b;
            if four || checker {
                xs += 1;
                x_at.get_or_insert((x, y));
            }
        }
    }
    if xs > 0 {
        let (x, y) = x_at.unwrap();
        rep.warnings.push(format!("{xs} X-crossings where four provinces meet at a pixel corner (first at x={x}, y={y})"));
    }

    rep.provinces = by_color.iter().filter(|d| d.is_some()).count();
    rep.ok = rep.errors.is_empty();
    (rep, by_color)
}

/// Is colour `c` (nearly) a mix of `a` and `b`?
fn between(c: [u8; 3], a: [u8; 3], b: [u8; 3]) -> bool {
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for k in 0..3 {
        let d = a[k] as f64 - b[k] as f64;
        num += (c[k] as f64 - b[k] as f64) * d;
        den += d * d;
    }
    if den < 1.0 {
        return false;
    }
    let t = num / den;
    if !(0.03..=0.97).contains(&t) {
        return false;
    }
    (0..3).all(|k| (c[k] as f64 - (b[k] as f64 + t * (a[k] as f64 - b[k] as f64))).abs() <= 6.0)
}

/// Content hash of the image and CSV (the import's cache key).
pub fn content_hash(png: &str, csv: Option<&str>) -> Result<String, String> {
    let a = std::fs::read(png).map_err(|e| format!("{png}: {e}"))?;
    let mut h = hash::bytes(&a);
    if let Some(c) = csv {
        let b = std::fs::read(c).map_err(|e| format!("{c}: {e}"))?;
        h = hash::combine(h, hash::bytes(&b));
    }
    Ok(hash::hex(h))
}

/// Latitude band of an exported package (package.json next to the image), or the whole globe.
pub fn package_lat_range(png: &str) -> (f64, f64) {
    let dir = std::path::Path::new(png).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let v: Option<serde_json::Value> = std::fs::read_to_string(dir.join("package.json")).ok().and_then(|s| serde_json::from_str(&s).ok());
    match v {
        Some(v) => (v["lat_min"].as_f64().unwrap_or(-90.0), v["lat_max"].as_f64().unwrap_or(90.0)),
        None => (-90.0, 90.0),
    }
}

/// Validate files and build an import record (the `import_provinces` command).
pub fn describe(png: &str, csv: Option<&str>, lat_min: Option<f64>, lat_max: Option<f64>) -> Result<(ProvinceImport, Report), String> {
    let img = read_png(png)?;
    let defs = match csv {
        Some(c) => Some(read_definition(c)?),
        None => None,
    };
    let (rep, _) = validate(&img, defs.as_deref(), 8);
    let (a, b) = package_lat_range(png);
    let imp = ProvinceImport {
        png: png.to_string(),
        csv: csv.map(|s| s.to_string()),
        lat_min: lat_min.unwrap_or(a),
        lat_max: lat_max.unwrap_or(b),
        content_hash: content_hash(png, csv)?,
    };
    Ok((imp, rep))
}

/// Result of rasterising an import onto the grid.
pub struct Votes {
    /// Index into `defs` per cell (u32::MAX = no pixels and no neighbour to copy).
    pub cell: Vec<u32>,
    pub defs: Vec<Def>,
    pub report: Report,
}

/// Each cell takes the province with the most pixels inside it; cells that got
/// no pixels copy a neighbour.
pub fn rasterize(imp: &ProvinceImport, g: &Grid) -> Result<Votes, String> {
    let img = read_png(&imp.png)?;
    let defs = match &imp.csv {
        Some(c) => Some(read_definition(c)?),
        None => None,
    };
    let (report, by_color) = validate(&img, defs.as_deref(), 8);
    if !report.ok {
        return Err(format!("province import has errors: {}", report.errors.join(" | ")));
    }
    // Compact the definitions that appear in the image.
    let mut out_defs: Vec<Def> = Vec::new();
    let mut color_to_def = vec![u32::MAX; img.colors.len()];
    for (k, d) in by_color.iter().enumerate() {
        if let Some(d) = d {
            color_to_def[k] = out_defs.len() as u32;
            out_defs.push(d.clone());
        }
    }
    let n = g.len();
    // Up to four candidate provinces per cell, with pixel counts.
    let mut votes = vec![[(u32::MAX, 0u32); 4]; n];
    let (w, h) = (img.width, img.height);
    let span = imp.lat_max - imp.lat_min;
    let mut hint = None;
    for y in 0..h {
        let lat = (imp.lat_max - (y as f64 + 0.5) / h as f64 * span).to_radians();
        for x in 0..w {
            let lon = (-180.0 + (x as f64 + 0.5) / w as f64 * 360.0).to_radians();
            let c = g.nearest(Vec3::from_lat_lon(lat, lon), hint);
            hint = Some(c);
            let d = color_to_def[img.px[y * w + x] as usize];
            if d == u32::MAX {
                continue;
            }
            let slots = &mut votes[c];
            if let Some(s) = slots.iter_mut().find(|s| s.0 == d) {
                s.1 += 1;
            } else if let Some(s) = slots.iter_mut().find(|s| s.0 == u32::MAX) {
                *s = (d, 1);
            }
        }
    }
    let mut cell: Vec<u32> = votes.iter().map(|v| v.iter().filter(|s| s.0 != u32::MAX).max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map_or(u32::MAX, |s| s.0)).collect();
    // Cells without pixels (outside the band, or a coarse image) copy a neighbour.
    for _ in 0..n {
        let mut changed = false;
        for i in 0..n {
            if cell[i] != u32::MAX {
                continue;
            }
            if let Some(&j) = g.neighbors(i).iter().find(|&&j| cell[j as usize] != u32::MAX) {
                cell[i] = cell[j as usize];
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(Votes { cell, defs: out_defs, report })
}
