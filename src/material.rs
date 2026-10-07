use crate::math::{Vec3, hash_3d};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MaterialId {
    #[default]
    Air = 0,
    EndStone = 1,
    Obsidian = 2,
    Glass = 3,
    EndCrystal = 4,
    DragonHide = 5,
    DragonWing = 6,
    DragonEye = 7,
    EndermanHide = 8,
    EndermanEye = 9,
    Iron = 10,
    Bedrock = 11,
    Portal = 12,
    Flame = 13,
    CrystalFrame = 14,
}

#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub albedo: Vec3,
    pub diffuse: f32,
    pub specular: f32,
    pub shininess: f32,
    pub transparency: f32,
    pub reflectivity: f32,
    pub refractive_index: f32,
    pub emission: Vec3,
    pub absorption: Vec3,
    pub normal_strength: f32,
}

impl Material {
    fn opaque(
        albedo: Vec3,
        specular: f32,
        reflectivity: f32,
        shininess: f32,
        normal_strength: f32,
    ) -> Self {
        Self {
            albedo,
            diffuse: 0.9,
            specular,
            shininess,
            transparency: 0.0,
            reflectivity,
            refractive_index: 1.0,
            emission: Vec3::ZERO,
            absorption: Vec3::ZERO,
            normal_strength,
        }
    }
}

impl MaterialId {
    pub fn properties(self) -> Material {
        match self {
            Self::Air => Material {
                transparency: 1.0,
                ..Material::opaque(Vec3::ZERO, 0.0, 0.0, 1.0, 0.0)
            },
            Self::EndStone => {
                Material::opaque(Vec3::new(0.62, 0.65, 0.39), 0.035, 0.008, 9.0, 0.14)
            }
            Self::Obsidian => {
                Material::opaque(Vec3::new(0.020, 0.014, 0.029), 0.12, 0.004, 70.0, 0.045)
            }
            Self::Glass => Material {
                albedo: Vec3::new(0.60, 0.76, 0.82),
                diffuse: 0.08,
                specular: 0.90,
                shininess: 128.0,
                transparency: 0.92,
                reflectivity: 0.05,
                refractive_index: 1.50,
                emission: Vec3::ZERO,
                absorption: Vec3::new(0.035, 0.018, 0.010),
                normal_strength: 0.012,
            },
            Self::EndCrystal => Material {
                albedo: Vec3::new(0.84, 0.16, 0.67),
                diffuse: 0.30,
                specular: 0.65,
                shininess: 84.0,
                transparency: 0.16,
                reflectivity: 0.20,
                refractive_index: 1.34,
                emission: Vec3::new(0.95, 0.12, 1.35),
                absorption: Vec3::new(0.015, 0.08, 0.005),
                normal_strength: 0.045,
            },
            Self::DragonHide => {
                Material::opaque(Vec3::new(0.027, 0.025, 0.032), 0.14, 0.02, 28.0, 0.065)
            }
            Self::DragonWing => {
                Material::opaque(Vec3::new(0.048, 0.046, 0.056), 0.07, 0.01, 18.0, 0.035)
            }
            Self::DragonEye | Self::EndermanEye => Material {
                emission: Vec3::new(0.88, 0.08, 1.65),
                ..Material::opaque(Vec3::new(0.54, 0.07, 0.76), 0.12, 0.01, 32.0, 0.0)
            },
            Self::EndermanHide => {
                Material::opaque(Vec3::new(0.021, 0.020, 0.027), 0.09, 0.008, 22.0, 0.045)
            }
            Self::Iron => Material::opaque(Vec3::new(0.36, 0.37, 0.34), 0.62, 0.26, 96.0, 0.025),
            Self::Bedrock => Material::opaque(Vec3::new(0.20, 0.21, 0.18), 0.055, 0.01, 8.0, 0.12),
            Self::Portal => Material {
                emission: Vec3::new(0.007, 0.002, 0.017),
                ..Material::opaque(Vec3::new(0.004, 0.003, 0.011), 0.65, 0.30, 110.0, 0.0)
            },
            Self::Flame => Material {
                emission: Vec3::new(3.1, 1.25, 0.12),
                ..Material::opaque(Vec3::new(1.0, 0.48, 0.04), 0.0, 0.0, 1.0, 0.0)
            },
            Self::CrystalFrame => Material {
                emission: Vec3::new(0.10, 0.19, 0.23),
                ..Material::opaque(Vec3::new(0.60, 0.82, 0.88), 0.80, 0.28, 120.0, 0.01)
            },
        }
    }
}

pub fn texture_color(id: MaterialId, point: Vec3, normal: Vec3) -> Vec3 {
    let material = id.properties();
    let (u, v) = face_uv(point, normal);
    let px = (u * 16.0).floor() as i32;
    let py = (v * 16.0).floor() as i32;
    let grain = hash_3d(px, py, 0, id as u32 * 31 + 17);
    let cluster = hash_3d(px.div_euclid(3), py.div_euclid(2), 0, id as u32 * 47 + 71);
    match id {
        MaterialId::Air => Vec3::ZERO,
        MaterialId::EndStone => {
            let pit = if cluster > 0.79 && grain > 0.33 {
                0.64
            } else {
                1.0
            };
            material.albedo * (0.82 + grain * 0.22) * pit
        }
        MaterialId::Obsidian => {
            // Small broken mineral flecks, rather than continuous purple waves.
            let fleck = if cluster > 0.68 {
                0.25 + grain * 0.55
            } else {
                grain * 0.10
            };
            material.albedo.lerp(Vec3::new(0.082, 0.044, 0.12), fleck) * (0.78 + grain * 0.3)
        }
        MaterialId::Glass => {
            let seam = ((u * 8.0).rem_euclid(1.0) < 0.025 || (v * 8.0).rem_euclid(1.0) < 0.025)
                as u8 as f32;
            material.albedo * (0.92 + grain * 0.06 + seam * 0.09)
        }
        MaterialId::EndCrystal => {
            let rune = ((px.rem_euclid(7) == py.rem_euclid(7)) || (px + py).rem_euclid(9) == 0)
                as u8 as f32;
            material
                .albedo
                .lerp(Vec3::new(0.43, 0.80, 0.88), rune * 0.60 + grain * 0.12)
        }
        MaterialId::DragonHide | MaterialId::EndermanHide => {
            material.albedo * (0.80 + cluster * 0.40 + grain * 0.13)
        }
        MaterialId::DragonWing => {
            let rib = ((u * 1.7 + v * 0.6).sin().abs()).powf(20.0);
            material.albedo * (0.78 + grain * 0.15 + rib * 0.42)
        }
        MaterialId::Iron => material.albedo * (0.76 + grain * 0.35),
        MaterialId::Bedrock => material.albedo * (0.35 + cluster * 0.90 + grain * 0.28),
        MaterialId::Portal => {
            let star = if grain > 0.996 { 1.0 } else { 0.0 };
            material.albedo.lerp(Vec3::new(0.35, 0.27, 0.54), star)
        }
        MaterialId::Flame => material
            .albedo
            .lerp(Vec3::new(1.0, 0.92, 0.38), grain * 0.65),
        MaterialId::CrystalFrame => material.albedo * (0.85 + grain * 0.15),
        MaterialId::DragonEye | MaterialId::EndermanEye => material.albedo,
    }
}

pub fn mapped_normal(id: MaterialId, point: Vec3, geometric_normal: Vec3) -> Vec3 {
    let strength = id.properties().normal_strength;
    if strength <= 0.0 {
        return geometric_normal;
    }
    let tangent = if geometric_normal.y.abs() > 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let bitangent = geometric_normal.cross(tangent).normalized();
    let tangent = bitangent.cross(geometric_normal).normalized();
    let frequency = match id {
        MaterialId::EndStone | MaterialId::Bedrock => 16.0,
        MaterialId::Obsidian => 24.0,
        MaterialId::Glass => 8.0,
        MaterialId::EndCrystal => 15.0,
        MaterialId::DragonHide | MaterialId::EndermanHide => 20.0,
        MaterialId::DragonWing => 9.0,
        _ => 12.0,
    };
    let du = (point.dot(tangent) * frequency).sin() * 0.55
        + (point.dot(bitangent) * frequency * 0.73).cos() * 0.25;
    let dv = (point.dot(bitangent) * frequency * 1.13).sin() * 0.55
        - (point.dot(tangent) * frequency * 0.61).cos() * 0.25;
    (geometric_normal + tangent * (du * strength) + bitangent * (dv * strength)).normalized()
}

fn face_uv(point: Vec3, normal: Vec3) -> (f32, f32) {
    if normal.x.abs() > 0.5 {
        (point.z, point.y)
    } else if normal.y.abs() > 0.5 {
        (point.x, point.z)
    } else {
        (point.x, point.y)
    }
}
