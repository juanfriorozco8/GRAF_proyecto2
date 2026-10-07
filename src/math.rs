use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, rhs: Self) -> f32 {
        self.x.mul_add(rhs.x, self.y.mul_add(rhs.y, self.z * rhs.z))
    }

    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y.mul_add(rhs.z, -self.z * rhs.y),
            self.z.mul_add(rhs.x, -self.x * rhs.z),
            self.x.mul_add(rhs.y, -self.y * rhs.x),
        )
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn normalized(self) -> Self {
        let length = self.length();
        if length > 1.0e-8 {
            self / length
        } else {
            Self::ZERO
        }
    }

    pub fn reflect(self, normal: Self) -> Self {
        self - normal * (2.0 * self.dot(normal))
    }

    pub fn refract(self, normal: Self, eta: f32) -> Option<Self> {
        let cos_i = (-self).dot(normal).clamp(-1.0, 1.0);
        let perpendicular = (self + normal * cos_i) * eta;
        let parallel_squared = 1.0 - perpendicular.length_squared();
        (parallel_squared >= 0.0)
            .then(|| (perpendicular - normal * parallel_squared.sqrt()).normalized())
    }

    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self * (1.0 - t) + rhs * t
    }

    pub fn clamp(self, min: f32, max: f32) -> Self {
        Self::new(
            self.x.clamp(min, max),
            self.y.clamp(min, max),
            self.z.clamp(min, max),
        )
    }

    pub fn exp(self) -> Self {
        Self::new(self.x.exp(), self.y.exp(), self.z.exp())
    }

    pub fn max_component(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }

    pub fn hadamard(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

impl Mul<Vec3> for f32 {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Self::Output {
        rhs * self
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y, -self.z)
    }
}

pub fn hash_u32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

pub fn hash_3d(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mixed = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f)
        ^ seed;
    hash_u32(mixed) as f32 / u32::MAX as f32
}

pub fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub fn value_noise_2d(x: f32, z: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let tx = smoothstep(x - x.floor());
    let tz = smoothstep(z - z.floor());
    let a = hash_3d(x0, 0, z0, seed);
    let b = hash_3d(x0 + 1, 0, z0, seed);
    let c = hash_3d(x0, 0, z0 + 1, seed);
    let d = hash_3d(x0 + 1, 0, z0 + 1, seed);
    let ab = a + (b - a) * tx;
    let cd = c + (d - c) * tx;
    ab + (cd - ab) * tz
}

pub fn fbm_2d(x: f32, z: f32, seed: u32) -> f32 {
    let mut frequency = 0.12;
    let mut amplitude = 0.58;
    let mut total = 0.0;
    for octave in 0..4 {
        total += value_noise_2d(
            x * frequency,
            z * frequency,
            seed.wrapping_add(octave * 7919),
        ) * amplitude;
        frequency *= 2.03;
        amplitude *= 0.5;
    }
    total
}
