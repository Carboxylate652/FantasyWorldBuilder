//! Seeded 3D gradient noise (improved Perlin) plus fBm and ridged variants.
//! Sampled on the unit sphere, so there are no seams or polar pinching.

use crate::rng::Rng;
use crate::vec3::Vec3;

#[derive(Clone)]
pub struct Noise {
    perm: [u8; 512],
}

const GRAD: [[f64; 3]; 12] = [
    [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0], [-1.0, 0.0, 1.0], [1.0, 0.0, -1.0], [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0], [0.0, -1.0, 1.0], [0.0, 1.0, -1.0], [0.0, -1.0, -1.0],
];

#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

impl Noise {
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Rng::new(seed, stream);
        let mut p: [u8; 256] = std::array::from_fn(|i| i as u8);
        for i in (1..256).rev() {
            let j = rng.below(i + 1);
            p.swap(i, j);
        }
        let mut perm = [0u8; 512];
        for i in 0..512 {
            perm[i] = p[i & 255];
        }
        Noise { perm }
    }

    #[inline]
    fn grad(&self, h: u8, x: f64, y: f64, z: f64) -> f64 {
        let g = GRAD[(h % 12) as usize];
        g[0] * x + g[1] * y + g[2] * z
    }

    /// Gradient noise in roughly [-1, 1].
    pub fn sample(&self, p: Vec3) -> f64 {
        let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
        let (xi, yi, zi) = ((fx as i64 & 255) as usize, (fy as i64 & 255) as usize, (fz as i64 & 255) as usize);
        let (x, y, z) = (p.x - fx, p.y - fy, p.z - fz);
        let (u, v, w) = (fade(x), fade(y), fade(z));
        let pm = &self.perm;
        let a = pm[xi] as usize + yi;
        let aa = pm[a] as usize + zi;
        let ab = pm[a + 1] as usize + zi;
        let b = pm[xi + 1] as usize + yi;
        let ba = pm[b] as usize + zi;
        let bb = pm[b + 1] as usize + zi;
        let r = lerp(
            lerp(
                lerp(self.grad(pm[aa], x, y, z), self.grad(pm[ba], x - 1.0, y, z), u),
                lerp(self.grad(pm[ab], x, y - 1.0, z), self.grad(pm[bb], x - 1.0, y - 1.0, z), u),
                v,
            ),
            lerp(
                lerp(self.grad(pm[aa + 1], x, y, z - 1.0), self.grad(pm[ba + 1], x - 1.0, y, z - 1.0), u),
                lerp(
                    self.grad(pm[ab + 1], x, y - 1.0, z - 1.0),
                    self.grad(pm[bb + 1], x - 1.0, y - 1.0, z - 1.0),
                    u,
                ),
                v,
            ),
            w,
        );
        r * 1.1
    }

    /// Fractal Brownian motion, normalised to roughly [-1, 1].
    pub fn fbm(&self, p: Vec3, freq: f64, octaves: u32) -> f64 {
        let mut sum = 0.0;
        let mut amp = 1.0;
        let mut norm = 0.0;
        let mut f = freq;
        for o in 0..octaves {
            // Offset each octave so lattice artefacts do not line up.
            let off = Vec3::new(o as f64 * 17.13, o as f64 * 3.71, o as f64 * 11.37);
            sum += amp * self.sample(p * f + off);
            norm += amp;
            amp *= 0.5;
            f *= 2.03;
        }
        sum / norm
    }

    /// Ridged multifractal in [0, 1]: sharp crests, good for mountain detail.
    pub fn ridged(&self, p: Vec3, freq: f64, octaves: u32) -> f64 {
        let mut sum = 0.0;
        let mut amp = 1.0;
        let mut norm = 0.0;
        let mut f = freq;
        let mut weight = 1.0;
        for o in 0..octaves {
            let off = Vec3::new(o as f64 * 5.31, o as f64 * 13.7, o as f64 * 2.9);
            let mut n = 1.0 - self.sample(p * f + off).abs();
            n *= n;
            n *= weight;
            weight = (n * 1.6).clamp(0.0, 1.0);
            sum += amp * n;
            norm += amp;
            amp *= 0.5;
            f *= 2.1;
        }
        sum / norm
    }

    /// Domain-warped fBm, gives less regular continental shapes.
    pub fn warped(&self, p: Vec3, freq: f64, octaves: u32, warp: f64) -> f64 {
        let q = Vec3::new(
            self.fbm(p + Vec3::new(1.7, 9.2, 3.1), freq, 3),
            self.fbm(p + Vec3::new(8.3, 2.8, 5.5), freq, 3),
            self.fbm(p + Vec3::new(4.1, 6.6, 7.9), freq, 3),
        );
        self.fbm(p + q * warp, freq, octaves)
    }
}
