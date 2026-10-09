//! Deterministic random numbers. Everything is derived from (seed, stream) so
//! results never depend on thread scheduling or crate versions.

use crate::vec3::Vec3;

#[inline]
pub fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Stateless hash of (seed, stream, index) to a float in [0, 1).
#[inline]
pub fn hash_unit(seed: u64, stream: u64, index: u64) -> f64 {
    let h = mix64(seed ^ mix64(stream.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ mix64(index)));
    (h >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// Named sub-streams so that adding a new random consumer never shifts others.
pub mod stream {
    pub const SKETCH: u64 = 1;
    pub const PLATES: u64 = 2;
    pub const PLATE_GROWTH: u64 = 3;
    pub const PLATE_MOTION: u64 = 4;
    pub const HOTSPOTS: u64 = 5;
    pub const RELIEF_NOISE: u64 = 6;
    pub const COAST_NOISE: u64 = 7;
    pub const EXPORT_NOISE: u64 = 8;
    pub const SCATTER_BRUSH: u64 = 9;
    pub const ROUTING: u64 = 10;
    pub const STATES: u64 = 11;
    pub const PROVINCES: u64 = 12;
    pub const NAMES: u64 = 13;
    pub const PROVINCE_COLORS: u64 = 14;
    pub const BORDER_NOISE: u64 = 15;
    pub const CULTURES: u64 = 16;
    pub const COMMUNITIES: u64 = 17;
}

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64, stream: u64) -> Self {
        Rng { state: mix64(seed ^ mix64(stream.wrapping_add(0x632B_E59B_D9B4_E019))) }
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix64(self.state)
    }
    #[inline]
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
    #[inline]
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f64()
    }
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        ((self.next_u64() >> 1) % (n.max(1) as u64)) as usize
    }
    pub fn unit_vector(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let t = self.range(0.0, std::f64::consts::TAU);
        let r = (1.0 - z * z).sqrt();
        Vec3::new(r * t.cos(), z, r * t.sin())
    }
    pub fn normal(&mut self) -> f64 {
        let u1 = self.f64().max(1e-12);
        let u2 = self.f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}
