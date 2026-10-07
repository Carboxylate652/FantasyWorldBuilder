//! Minimal 3D vector type used for positions on the unit sphere.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };

    #[inline]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }
    #[inline]
    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    #[inline]
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    #[inline]
    pub fn len(self) -> f64 {
        self.dot(self).sqrt()
    }
    #[inline]
    pub fn normalized(self) -> Vec3 {
        let l = self.len();
        if l > 0.0 {
            self * (1.0 / l)
        } else {
            self
        }
    }
    /// Great-circle angle between two unit vectors (radians).
    #[inline]
    pub fn angle_to(self, o: Vec3) -> f64 {
        let c = self.cross(o).len();
        let d = self.dot(o);
        c.atan2(d)
    }
    /// Latitude/longitude in radians (y axis is the rotation axis, north = +y).
    #[inline]
    pub fn lat_lon(self) -> (f64, f64) {
        let lat = self.y.clamp(-1.0, 1.0).asin();
        let lon = self.x.atan2(self.z);
        (lat, lon)
    }
    #[inline]
    pub fn from_lat_lon(lat: f64, lon: f64) -> Vec3 {
        let c = lat.cos();
        Vec3::new(c * lon.sin(), lat.sin(), c * lon.cos())
    }
    pub fn from_lat_lon_deg(lat: f64, lon: f64) -> Vec3 {
        Vec3::from_lat_lon(lat.to_radians(), lon.to_radians())
    }
    /// Local east and north unit vectors at this point on the sphere.
    pub fn east_north(self) -> (Vec3, Vec3) {
        let up = Vec3::new(0.0, 1.0, 0.0);
        let mut east = up.cross(self);
        if east.len() < 1e-9 {
            east = Vec3::new(1.0, 0.0, 0.0);
        }
        let east = east.normalized();
        let north = self.cross(east).normalized();
        (east, north)
    }
    /// Rotate around a unit axis by `angle` radians (Rodrigues).
    pub fn rotate(self, axis: Vec3, angle: f64) -> Vec3 {
        let (s, c) = angle.sin_cos();
        self * c + axis.cross(self) * s + axis * (axis.dot(self) * (1.0 - c))
    }
    /// Project onto the tangent plane at unit vector `p`.
    #[inline]
    pub fn tangent_at(self, p: Vec3) -> Vec3 {
        self - p * self.dot(p)
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    #[inline]
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for Vec3 {
    #[inline]
    fn add_assign(&mut self, o: Vec3) {
        self.x += o.x;
        self.y += o.y;
        self.z += o.z;
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    #[inline]
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f64> for Vec3 {
    type Output = Vec3;
    #[inline]
    fn mul(self, s: f64) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    #[inline]
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}
