//! Heightmap import: an equirectangular PNG (for example an exported heightmap
//! edited in GIMP, or real Earth elevation) replaces the tectonic relief.

use crate::edits::{ElevationImport, HeightEncoding};
use crate::hash;

pub struct Heightmap {
    pub width: usize,
    pub height: usize,
    /// Metres, row-major from the north edge.
    pub data: Vec<f32>,
    pub lat_min: f64,
    pub lat_max: f64,
}

/// Build an import record for a file, hashing its contents as the cache key.
pub fn describe(path: &str, encoding: HeightEncoding, lat_min: f64, lat_max: f64, blend: f64) -> Result<ElevationImport, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let imp = ElevationImport { path: path.to_string(), encoding, lat_min, lat_max, blend: blend.clamp(0.0, 1.0), content_hash: hash::hex(hash::bytes(&bytes)) };
    load(&imp)?; // validate now rather than at the next run
    Ok(imp)
}

pub fn load(imp: &ElevationImport) -> Result<Heightmap, String> {
    let file = std::fs::File::open(&imp.path).map_err(|e| format!("{}: {e}", imp.path))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().map_err(|e| format!("{}: {e}", imp.path))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{}: {e}", imp.path))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = info.color_type.samples();
    let sixteen = info.bit_depth == png::BitDepth::Sixteen;
    let bpp = channels * if sixteen { 2 } else { 1 };
    let max = if sixteen { 65535.0 } else { 255.0 };
    let mut data = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = &buf[y * info.line_size..];
        for x in 0..w {
            let o = x * bpp;
            let raw = if sixteen { u16::from_be_bytes([row[o], row[o + 1]]) as f64 } else { row[o] as f64 };
            let v = raw / max;
            let m = match imp.encoding {
                HeightEncoding::Heightmap16 => v * 20000.0 - 11000.0,
                HeightEncoding::Linear { min_m, max_m } => min_m + v * (max_m - min_m),
                HeightEncoding::Paradox8 { sea_level_value, max_elevation_m, max_depth_m } => {
                    let r = v * 255.0;
                    if r <= sea_level_value {
                        (r / sea_level_value.max(1.0) - 1.0) * max_depth_m
                    } else {
                        (r - sea_level_value - 1.0).max(0.0) / (254.0 - sea_level_value).max(1.0) * max_elevation_m
                    }
                }
            };
            data.push(m as f32);
        }
    }
    Ok(Heightmap { width: w, height: h, data, lat_min: imp.lat_min, lat_max: imp.lat_max })
}

impl Heightmap {
    /// Bilinear sample at a latitude/longitude in degrees; None outside the latitude band.
    pub fn sample(&self, lat: f64, lon: f64) -> Option<f32> {
        if lat < self.lat_min || lat > self.lat_max {
            return None;
        }
        let fx = (lon + 180.0) / 360.0 * self.width as f64 - 0.5;
        let fy = (self.lat_max - lat) / (self.lat_max - self.lat_min) * self.height as f64 - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor().clamp(0.0, (self.height - 1) as f64);
        let tx = (fx - x0) as f32;
        let ty = ((fy - y0) as f32).clamp(0.0, 1.0);
        let w = self.width as i64;
        let xi = |x: f64| (((x as i64) % w + w) % w) as usize;
        let (xa, xb) = (xi(x0), xi(x0 + 1.0));
        let ya = y0 as usize;
        let yb = (ya + 1).min(self.height - 1);
        let at = |x: usize, y: usize| self.data[y * self.width + x];
        let top = at(xa, ya) * (1.0 - tx) + at(xb, ya) * tx;
        let bot = at(xa, yb) * (1.0 - tx) + at(xb, yb) * tx;
        Some(top * (1.0 - ty) + bot * ty)
    }
}
