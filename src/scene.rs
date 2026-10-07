use crate::geometry::{Cuboid, DetailGeometry, Triangle};
use crate::material::MaterialId;
use crate::math::{Vec3, fbm_2d, value_noise_2d};

pub const WORLD_X: usize = 160;
pub const WORLD_Y: usize = 96;
pub const WORLD_Z: usize = 160;
const HALF_X: i32 = WORLD_X as i32 / 2;
const HALF_Z: i32 = WORLD_Z as i32 / 2;
const CHUNK_SIZE: usize = 8;
const CHUNKS_X: usize = WORLD_X.div_ceil(CHUNK_SIZE);
const CHUNKS_Y: usize = WORLD_Y.div_ceil(CHUNK_SIZE);
const CHUNKS_Z: usize = WORLD_Z.div_ceil(CHUNK_SIZE);

#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    pub inverse_direction: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        let direction = direction.normalized();
        Self {
            origin,
            direction,
            inverse_direction: Vec3::new(
                direction.x.recip(),
                direction.y.recip(),
                direction.z.recip(),
            ),
        }
    }

    pub fn at(self, distance: f32) -> Vec3 {
        self.origin + self.direction * distance
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Intersection {
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
    pub material: MaterialId,
    pub entering: bool,
    pub texture_point: Vec3,
    pub texture_normal: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f32,
}

#[derive(Debug)]
pub struct World {
    voxels: Vec<MaterialId>,
    occupied_chunks: Vec<bool>,
    voxel_bounds: Option<(Vec3, Vec3)>,
    pub point_lights: Vec<PointLight>,
    details: DetailGeometry,
}

impl World {
    pub fn the_end(seed: u32) -> Self {
        let mut world = Self {
            voxels: vec![MaterialId::Air; WORLD_X * WORLD_Y * WORLD_Z],
            occupied_chunks: vec![false; CHUNKS_X * CHUNKS_Y * CHUNKS_Z],
            voxel_bounds: None,
            point_lights: Vec::new(),
            details: DetailGeometry::default(),
        };
        world.generate_terrain(seed);
        world.build_exit_portal();
        world.build_obsidian_towers();
        world.build_endermen();
        world.build_ender_dragon();
        world.details.build();
        world
    }

    pub fn voxel_count(&self) -> usize {
        self.voxels
            .iter()
            .filter(|&&material| material != MaterialId::Air)
            .count()
    }

    pub fn medium_at(&self, point: Vec3) -> MaterialId {
        let voxel = self.get_world(
            point.x.floor() as i32,
            point.y.floor() as i32,
            point.z.floor() as i32,
        );
        if voxel != MaterialId::Air {
            voxel
        } else {
            self.details.medium_at(point)
        }
    }

    pub fn intersect(&self, ray: Ray, max_distance: f32) -> Option<Intersection> {
        let voxel = self.intersect_voxels(ray, max_distance);
        self.details
            .intersect(ray, voxel.as_ref().map_or(max_distance, |hit| hit.distance))
            .or(voxel)
    }

    fn intersect_voxels(&self, ray: Ray, max_distance: f32) -> Option<Intersection> {
        let (bounds_min, bounds_max) = self.voxel_bounds?;
        let (entry, exit, entry_normal) = intersect_bounds(ray, bounds_min, bounds_max)?;
        let start = entry.max(0.0) + 1.0e-4;
        let end = exit.min(max_distance);
        if start >= end {
            return None;
        }

        let position = ray.at(start);
        let mut x = position.x.floor() as i32;
        let mut y = position.y.floor() as i32;
        let mut z = position.z.floor() as i32;
        let step_x = direction_step(ray.direction.x);
        let step_y = direction_step(ray.direction.y);
        let step_z = direction_step(ray.direction.z);
        let delta_x = direction_delta(ray.direction.x);
        let delta_y = direction_delta(ray.direction.y);
        let delta_z = direction_delta(ray.direction.z);
        let mut next_x = next_boundary(ray.origin.x, ray.direction.x, x, step_x);
        let mut next_y = next_boundary(ray.origin.y, ray.direction.y, y, step_y);
        let mut next_z = next_boundary(ray.origin.z, ray.direction.z, z, step_z);
        let mut current = self.get_world(x, y, z);
        if entry > 0.0 && current != MaterialId::Air {
            return Some(Intersection {
                point: ray.at(entry),
                normal: entry_normal,
                distance: entry,
                material: current,
                entering: true,
                texture_point: ray.at(entry),
                texture_normal: entry_normal,
            });
        }

        for _ in 0..(WORLD_X + WORLD_Y + WORLD_Z + 8) * 3 {
            if current == MaterialId::Air && !self.chunk_occupied(x, y, z) {
                let chunk_x =
                    (x + HALF_X).div_euclid(CHUNK_SIZE as i32) * CHUNK_SIZE as i32 - HALF_X;
                let chunk_y = y.div_euclid(CHUNK_SIZE as i32) * CHUNK_SIZE as i32;
                let chunk_z =
                    (z + HALF_Z).div_euclid(CHUNK_SIZE as i32) * CHUNK_SIZE as i32 - HALF_Z;
                let chunk_boundary = |origin, direction, lower, step| {
                    let cell = if step > 0 {
                        lower + CHUNK_SIZE as i32 - 1
                    } else {
                        lower
                    };
                    next_boundary(origin, direction, cell, step)
                };
                let boundary_x = chunk_boundary(ray.origin.x, ray.direction.x, chunk_x, step_x);
                let boundary_y = chunk_boundary(ray.origin.y, ray.direction.y, chunk_y, step_y);
                let boundary_z = chunk_boundary(ray.origin.z, ray.direction.z, chunk_z, step_z);
                let (distance, normal) = if boundary_x <= boundary_y && boundary_x <= boundary_z {
                    (boundary_x, Vec3::new(-(step_x as f32), 0.0, 0.0))
                } else if boundary_y <= boundary_z {
                    (boundary_y, Vec3::new(0.0, -(step_y as f32), 0.0))
                } else {
                    (boundary_z, Vec3::new(0.0, 0.0, -(step_z as f32)))
                };
                if distance > end {
                    return None;
                }
                let position = ray.at(distance + 1.0e-4);
                x = position.x.floor() as i32;
                y = position.y.floor() as i32;
                z = position.z.floor() as i32;
                current = self.get_world(x, y, z);
                if current != MaterialId::Air {
                    return Some(Intersection {
                        point: ray.at(distance),
                        normal,
                        distance,
                        material: current,
                        entering: true,
                        texture_point: ray.at(distance),
                        texture_normal: normal,
                    });
                }
                next_x = next_boundary(ray.origin.x, ray.direction.x, x, step_x);
                next_y = next_boundary(ray.origin.y, ray.direction.y, y, step_y);
                next_z = next_boundary(ray.origin.z, ray.direction.z, z, step_z);
                continue;
            }
            let (distance, axis, step) = if next_x <= next_y && next_x <= next_z {
                let distance = next_x;
                x += step_x;
                next_x += delta_x;
                (distance, Vec3::new(1.0, 0.0, 0.0), step_x)
            } else if next_y <= next_z {
                let distance = next_y;
                y += step_y;
                next_y += delta_y;
                (distance, Vec3::new(0.0, 1.0, 0.0), step_y)
            } else {
                let distance = next_z;
                z += step_z;
                next_z += delta_z;
                (distance, Vec3::new(0.0, 0.0, 1.0), step_z)
            };

            if distance > end {
                return None;
            }
            let next = self.get_world(x, y, z);
            if current != next && (current != MaterialId::Air || next != MaterialId::Air) {
                let entering = current == MaterialId::Air;
                let material = if entering { next } else { current };
                let normal = if entering {
                    axis * -(step as f32)
                } else {
                    axis * step as f32
                };
                return Some(Intersection {
                    point: ray.at(distance),
                    normal,
                    distance,
                    material,
                    entering,
                    texture_point: ray.at(distance),
                    texture_normal: normal,
                });
            }
            current = next;
        }
        None
    }

    fn generate_terrain(&mut self, seed: u32) {
        // Broad terraces match the central End island; the underside tapers into the void.
        for x in -HALF_X..HALF_X {
            for z in -HALF_Z..HALF_Z {
                let xf = x as f32;
                let zf = z as f32;
                let radius = (xf * xf + zf * zf).sqrt();
                let edge = fbm_2d(xf * 0.34, zf * 0.34, seed.wrapping_add(97));
                let island_radius = 70.0 + (edge - 0.5) * 11.0;
                if radius > island_radius {
                    continue;
                }
                let broad = value_noise_2d(xf * 0.035, zf * 0.035, seed);
                let detail = fbm_2d(xf * 0.32, zf * 0.32, seed);
                let mound = 5.2 * (-radius * radius / 1250.0).exp();
                let top =
                    (24.0 + mound + (broad - 0.5) * 4.0 + (detail - 0.5) * 1.4).floor() as i32;
                let taper = (1.0 - (radius / island_radius).powi(2)).max(0.0);
                let thickness = 4.0 + 24.0 * taper.powf(0.65) + (edge - 0.5) * 3.0;
                let bottom = (top - thickness as i32).max(1);
                for y in bottom..=top {
                    self.set_world(x, y, z, MaterialId::EndStone);
                }
            }
        }
    }

    fn add_box(&mut self, center: Vec3, size: Vec3, material: MaterialId) {
        self.details.add_box(Cuboid::new(center, size, material));
    }

    fn add_rotated_box(
        &mut self,
        center: Vec3,
        size: Vec3,
        yaw: f32,
        roll: f32,
        material: MaterialId,
    ) {
        self.details
            .add_box(Cuboid::rotated(center, size, yaw, roll, material));
    }

    fn build_exit_portal(&mut self) {
        let y = self.surface_height(0, 0) + 1;
        for x in -4_i32..=4 {
            for z in -4_i32..=4 {
                let radius = x * x + z * z;
                if (9..=17).contains(&radius) {
                    self.set_world(x, y, z, MaterialId::Bedrock);
                } else if radius < 9 {
                    self.add_box(
                        Vec3::new(x as f32 + 0.5, y as f32 + 0.12, z as f32 + 0.5),
                        Vec3::new(1.0, 0.16, 1.0),
                        MaterialId::Portal,
                    );
                }
            }
        }
        for height in y..=y + 3 {
            self.set_world(0, height, 0, MaterialId::Bedrock);
        }
        for &(x, z) in &[(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let center = Vec3::new(x as f32 + 0.5, y as f32 + 2.0, z as f32 + 0.5);
            self.add_box(center, Vec3::new(0.13, 0.75, 0.13), MaterialId::DragonHide);
            self.add_box(
                center + Vec3::new(0.0, 0.42, 0.0),
                Vec3::new(0.19, 0.32, 0.19),
                MaterialId::Flame,
            );
        }
        self.point_lights.push(PointLight {
            position: Vec3::new(0.5, y as f32 + 3.7, 0.5),
            color: Vec3::new(1.0, 0.66, 0.28),
            intensity: 22.0,
        });
    }

    fn build_obsidian_towers(&mut self) {
        // Ten pillars, 42-block ring. The shortest two have iron cages.
        // Heights are expressed relative to the island's central surface, not sea level.
        let heights = [37, 22, 40, 28, 43, 19, 34, 25, 31, 16];
        let radii = [4, 2, 4, 3, 5, 2, 4, 3, 3, 2];
        for i in 0..10 {
            let angle = i as f32 * std::f32::consts::TAU / 10.0 + 0.15;
            let cx = (angle.cos() * 42.0).round() as i32;
            let cz = (angle.sin() * 42.0).round() as i32;
            let base = self.surface_height(cx, cz) + 1;
            let top = 28 + heights[i];
            let radius = radii[i];
            for x in cx - radius..=cx + radius {
                for z in cz - radius..=cz + radius {
                    let dx = x - cx;
                    let dz = z - cz;
                    if dx * dx + dz * dz <= radius * radius + 1 {
                        for y in base..=top {
                            self.set_world(x, y, z, MaterialId::Obsidian);
                        }
                    }
                }
            }
            self.build_end_crystal(cx, top + 1, cz, i as f32 * 0.37);
            if heights[i] == 19 || heights[i] == 22 {
                self.build_iron_cage(Vec3::new(
                    cx as f32 + 0.5,
                    top as f32 + 1.0,
                    cz as f32 + 0.5,
                ));
            }
        }
    }

    fn build_iron_cage(&mut self, base: Vec3) {
        let width = 5.4;
        let height = 4.8;
        for i in 0..=9 {
            let offset = -width * 0.5 + i as f32 * width / 9.0;
            for side in [-1.0_f32, 1.0] {
                self.add_box(
                    base + Vec3::new(offset, height * 0.5, side * width * 0.5),
                    Vec3::new(0.075, height, 0.075),
                    MaterialId::Iron,
                );
                self.add_box(
                    base + Vec3::new(side * width * 0.5, height * 0.5, offset),
                    Vec3::new(0.075, height, 0.075),
                    MaterialId::Iron,
                );
            }
            self.add_box(
                base + Vec3::new(offset, height, 0.0),
                Vec3::new(0.075, 0.075, width),
                MaterialId::Iron,
            );
            self.add_box(
                base + Vec3::new(0.0, height, offset),
                Vec3::new(width, 0.075, 0.075),
                MaterialId::Iron,
            );
        }
        for level in [0.0, 1.6, 3.2, height] {
            for side in [-1.0_f32, 1.0] {
                self.add_box(
                    base + Vec3::new(0.0, level, side * width * 0.5),
                    Vec3::new(width, 0.075, 0.075),
                    MaterialId::Iron,
                );
                self.add_box(
                    base + Vec3::new(side * width * 0.5, level, 0.0),
                    Vec3::new(0.075, 0.075, width),
                    MaterialId::Iron,
                );
            }
        }
    }

    fn build_end_crystal(&mut self, x: i32, y: i32, z: i32, yaw: f32) {
        let base = Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5);
        self.add_box(
            base + Vec3::new(0.0, 0.16, 0.0),
            Vec3::new(1.8, 0.32, 1.8),
            MaterialId::Bedrock,
        );
        self.add_box(
            base + Vec3::new(0.0, 0.65, 0.0),
            Vec3::new(0.65, 0.70, 0.65),
            MaterialId::Flame,
        );
        let center = base + Vec3::new(0.0, 1.9, 0.0);
        let roll = std::f32::consts::FRAC_PI_4;
        self.add_rotated_box(center, Vec3::ONE * 1.45, yaw, roll, MaterialId::Glass);
        self.add_rotated_box(
            center,
            Vec3::ONE * 0.64,
            yaw + 0.45,
            roll,
            MaterialId::EndCrystal,
        );
        // Thin cube edges give the crystal the familiar wireframe silhouette.
        let (sy, cy) = yaw.sin_cos();
        let (sr, cr) = roll.sin_cos();
        let axes = [
            Vec3::new(cy * cr, sr, -sy * cr),
            Vec3::new(-cy * sr, cr, sy * sr),
            Vec3::new(sy, 0.0, cy),
        ];
        for axis in 0..3 {
            for a in [-1.0_f32, 1.0] {
                for b in [-1.0_f32, 1.0] {
                    let offset =
                        axes[(axis + 1) % 3] * (a * 0.725) + axes[(axis + 2) % 3] * (b * 0.725);
                    let size = match axis {
                        0 => Vec3::new(1.48, 0.035, 0.035),
                        1 => Vec3::new(0.035, 1.48, 0.035),
                        _ => Vec3::new(0.035, 0.035, 1.48),
                    };
                    self.add_rotated_box(
                        center + offset,
                        size,
                        yaw,
                        roll,
                        MaterialId::CrystalFrame,
                    );
                }
            }
        }
        self.point_lights.push(PointLight {
            position: base + Vec3::new(0.0, 2.0, 0.0),
            color: Vec3::new(1.0, 0.63, 0.32),
            intensity: 12.0,
        });
    }

    fn build_endermen(&mut self) {
        for &(x, z, yaw) in &[
            (16, -13, 0.1_f32),
            (-18, -7, 0.7),
            (8, 21, -0.6),
            (-30, 14, 0.3),
            (29, 4, -0.9),
            (-8, 34, 0.8),
            (35, -18, -0.4),
            (-34, -27, 0.2),
            (4, -30, -0.5),
        ] {
            let base = Vec3::new(
                x as f32 + 0.5,
                self.surface_height(x, z) as f32 + 1.0,
                z as f32 + 0.5,
            );
            let (sy, cy) = yaw.sin_cos();
            let transform = |local: Vec3| {
                base + Vec3::new(
                    local.x * cy + local.z * sy,
                    local.y,
                    -local.x * sy + local.z * cy,
                )
            };
            for (center, size) in [
                (Vec3::new(-0.15, 0.85, 0.0), Vec3::new(0.17, 1.7, 0.17)),
                (Vec3::new(0.15, 0.85, 0.0), Vec3::new(0.17, 1.7, 0.17)),
                (Vec3::new(0.0, 2.02, 0.0), Vec3::new(0.50, 0.68, 0.27)),
                (Vec3::new(-0.38, 1.57, 0.0), Vec3::new(0.15, 1.65, 0.15)),
                (Vec3::new(0.38, 1.57, 0.0), Vec3::new(0.15, 1.65, 0.15)),
                (Vec3::new(0.0, 2.65, 0.0), Vec3::new(0.64, 0.59, 0.64)),
            ] {
                self.add_rotated_box(transform(center), size, yaw, 0.0, MaterialId::EndermanHide);
            }
            for side in [-1.0_f32, 1.0] {
                self.add_rotated_box(
                    transform(Vec3::new(side * 0.16, 2.67, -0.324)),
                    Vec3::new(0.19, 0.055, 0.012),
                    yaw,
                    0.0,
                    MaterialId::EndermanEye,
                );
            }
        }
    }

    fn build_ender_dragon(&mut self) {
        let origin = Vec3::new(-3.0, 44.0, -3.0);
        for (center, size) in [
            (Vec3::new(0.0, 0.0, 1.0), Vec3::new(2.2, 2.1, 5.4)),
            (Vec3::new(0.0, 0.4, -2.9), Vec3::new(1.25, 1.25, 2.4)),
            (Vec3::new(0.0, 0.7, -5.0), Vec3::new(2.35, 1.5, 2.2)),
            (Vec3::new(0.0, 0.05, -6.65), Vec3::new(1.35, 0.75, 1.7)),
            (Vec3::new(-0.80, 1.65, -4.8), Vec3::new(0.30, 0.85, 0.30)),
            (Vec3::new(0.80, 1.65, -4.8), Vec3::new(0.30, 0.85, 0.30)),
        ] {
            self.add_box(origin + center, size, MaterialId::DragonHide);
        }
        for side in [-1.0_f32, 1.0] {
            self.add_box(
                origin + Vec3::new(side * 1.182, 0.96, -5.55),
                Vec3::new(0.035, 0.18, 0.65),
                MaterialId::DragonEye,
            );
            for z in [0.0, 2.6] {
                self.add_rotated_box(
                    origin + Vec3::new(side * 1.05, -1.5, z),
                    Vec3::new(0.42, 1.8, 0.42),
                    0.0,
                    side * 0.22,
                    MaterialId::DragonHide,
                );
                self.add_box(
                    origin + Vec3::new(side * 1.3, -2.45, z - 0.3),
                    Vec3::new(0.62, 0.28, 0.9),
                    MaterialId::DragonHide,
                );
            }
            let root = origin + Vec3::new(side * 1.0, 0.7, -0.8);
            let elbow = origin + Vec3::new(side * 5.2, 2.4, -1.4);
            let tip = origin + Vec3::new(side * 11.0, 3.1, 0.3);
            let rear = origin + Vec3::new(side * 6.7, 0.4, 5.3);
            let inner = origin + Vec3::new(side * 1.1, -0.2, 3.5);
            for (a, b, c) in [(root, elbow, rear), (elbow, tip, rear), (root, rear, inner)] {
                self.details
                    .add_triangle(Triangle::new(a, b, c, MaterialId::DragonWing));
            }
            for (a, b) in [
                (root, elbow),
                (elbow, tip),
                (tip, rear),
                (rear, inner),
                (elbow, rear),
            ] {
                self.add_bone(a, b, 0.16, MaterialId::Bedrock);
            }
        }
        let mut previous = origin + Vec3::new(0.0, -0.1, 3.7);
        for i in 0..9 {
            let t = i as f32;
            let next = origin + Vec3::new((t * 0.35).sin() * 1.6, -0.15 - t * 0.20, 4.8 + t * 1.08);
            self.add_bone(
                previous,
                next,
                (0.75 - t * 0.07).max(0.16),
                MaterialId::DragonHide,
            );
            previous = next;
        }
        for z in [-2.8, -1.0, 1.0, 3.0, 5.0, 7.0] {
            self.add_box(
                origin + Vec3::new(0.0, if z < 4.0 { 1.24 } else { -0.1 }, z),
                Vec3::new(0.28, 0.55, 0.40),
                MaterialId::Bedrock,
            );
        }
    }

    fn add_bone(&mut self, start: Vec3, end: Vec3, width: f32, material: MaterialId) {
        let delta = end - start;
        let length = delta.length();
        let yaw = (-delta.z).atan2(delta.x);
        let roll = (delta.y / length).clamp(-1.0, 1.0).asin();
        self.add_rotated_box(
            (start + end) * 0.5,
            Vec3::new(length, width, width),
            yaw,
            roll,
            material,
        );
    }

    fn surface_height(&self, x: i32, z: i32) -> i32 {
        (0..WORLD_Y as i32)
            .rev()
            .find(|&y| self.get_world(x, y, z) != MaterialId::Air)
            .unwrap_or(0)
    }

    fn get_world(&self, x: i32, y: i32, z: i32) -> MaterialId {
        let ix = x + HALF_X;
        let iz = z + HALF_Z;
        if ix < 0
            || y < 0
            || iz < 0
            || ix >= WORLD_X as i32
            || y >= WORLD_Y as i32
            || iz >= WORLD_Z as i32
        {
            return MaterialId::Air;
        }
        self.voxels[index(ix as usize, y as usize, iz as usize)]
    }

    fn chunk_occupied(&self, x: i32, y: i32, z: i32) -> bool {
        let ix = x + HALF_X;
        let iz = z + HALF_Z;
        if ix < 0
            || iz < 0
            || y < 0
            || ix >= WORLD_X as i32
            || iz >= WORLD_Z as i32
            || y >= WORLD_Y as i32
        {
            return false;
        }
        self.occupied_chunks[chunk_index(ix as usize, y as usize, iz as usize)]
    }

    fn set_world(&mut self, x: i32, y: i32, z: i32, material: MaterialId) {
        let ix = x + HALF_X;
        let iz = z + HALF_Z;
        if ix >= 0
            && y >= 0
            && iz >= 0
            && ix < WORLD_X as i32
            && y < WORLD_Y as i32
            && iz < WORLD_Z as i32
        {
            let voxel_index = index(ix as usize, y as usize, iz as usize);
            self.voxels[voxel_index] = material;
            if material != MaterialId::Air {
                self.occupied_chunks[chunk_index(ix as usize, y as usize, iz as usize)] = true;
                let lower = Vec3::new(x as f32, y as f32, z as f32);
                let upper = lower + Vec3::ONE;
                self.voxel_bounds = Some(match self.voxel_bounds {
                    None => (lower, upper),
                    Some((min, max)) => (
                        Vec3::new(min.x.min(lower.x), min.y.min(lower.y), min.z.min(lower.z)),
                        Vec3::new(max.x.max(upper.x), max.y.max(upper.y), max.z.max(upper.z)),
                    ),
                });
            }
        }
    }
}

fn chunk_index(x: usize, y: usize, z: usize) -> usize {
    ((y / CHUNK_SIZE) * CHUNKS_Z + z / CHUNK_SIZE) * CHUNKS_X + x / CHUNK_SIZE
}

fn index(x: usize, y: usize, z: usize) -> usize {
    (y * WORLD_Z + z) * WORLD_X + x
}

fn direction_step(value: f32) -> i32 {
    if value >= 0.0 { 1 } else { -1 }
}

fn direction_delta(value: f32) -> f32 {
    if value.abs() < 1.0e-8 {
        f32::INFINITY
    } else {
        value.abs().recip()
    }
}

fn next_boundary(origin: f32, direction: f32, cell: i32, step: i32) -> f32 {
    if direction.abs() < 1.0e-8 {
        f32::INFINITY
    } else {
        let boundary = if step > 0 {
            cell as f32 + 1.0
        } else {
            cell as f32
        };
        (boundary - origin) / direction
    }
}

fn intersect_bounds(ray: Ray, min: Vec3, max: Vec3) -> Option<(f32, f32, Vec3)> {
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    let mut normal = Vec3::ZERO;
    for (origin, direction, lower, upper, axis) in [
        (
            ray.origin.x,
            ray.direction.x,
            min.x,
            max.x,
            Vec3::new(1.0, 0.0, 0.0),
        ),
        (
            ray.origin.y,
            ray.direction.y,
            min.y,
            max.y,
            Vec3::new(0.0, 1.0, 0.0),
        ),
        (
            ray.origin.z,
            ray.direction.z,
            min.z,
            max.z,
            Vec3::new(0.0, 0.0, 1.0),
        ),
    ] {
        if direction.abs() < 1.0e-8 {
            if origin < lower || origin > upper {
                return None;
            }
        } else {
            let first = (lower - origin) / direction;
            let second = (upper - origin) / direction;
            let entry = first.min(second);
            if entry > near {
                near = entry;
                normal = if direction > 0.0 { -axis } else { axis };
            }
            far = far.min(first.max(second));
            if near > far {
                return None;
            }
        }
    }
    (far >= 0.0).then_some((near, far, normal))
}
