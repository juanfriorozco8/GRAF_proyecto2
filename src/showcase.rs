use crate::math::Vec3;
use crate::renderer::Camera;
use crate::scene::{Ray, World};

pub const DURATION_SECONDS: f32 = 36.0;

/// Every shot uses the same world, materials, lights and optical paths as the viewer.
pub fn camera_at(world: &World, seconds: f32, width: usize, height: usize) -> Camera {
    let seconds = seconds.clamp(0.0, DURATION_SECONDS);
    let aspect = width as f32 / height as f32;
    match seconds {
        t if t < 12.0 => Camera::orbit(t / 12.0, width, height),
        t if t < 20.0 => {
            // The tallest uncaged crystal: view both oblique glass faces and its core.
            let progress = ease((t - 12.0) / 8.0);
            let center = Vec3::new(42.5, 67.9, 6.5);
            let angle = -2.35 + progress * 1.65;
            let radius = 5.5 - (progress * std::f32::consts::PI).sin() * 0.6;
            let origin = center + Vec3::new(angle.cos() * radius, 0.65, angle.sin() * radius);
            Camera::look_at(origin, center - Vec3::new(0.0, 0.45, 0.0), 37.0, aspect)
        }
        t if t < 24.0 => {
            // The lower caged pillar: metallic reflection, open bars and crystal light.
            let progress = ease((t - 20.0) / 4.0);
            let center = Vec3::new(30.5, 52.0, 29.5);
            let angle = -0.9 + progress * 0.65;
            let origin = center + Vec3::new(angle.cos() * 11.5, 2.0, angle.sin() * 11.5);
            Camera::look_at(origin, center, 38.0, aspect)
        }
        t if t < 28.0 => {
            // Low grazing view resolves the normal-map shading and voxel terraces.
            let progress = ease((t - 24.0) / 4.0);
            let target = terrain_point(world, 13.5, -9.5);
            let origin = target + Vec3::new(-5.5 + progress * 4.0, 2.3, -5.0);
            Camera::look_at(origin, target, 43.0, aspect)
        }
        t if t < 32.0 => {
            // Portal torches visibly light the bedrock and nearby End Stone.
            let progress = ease((t - 28.0) / 4.0);
            let floor = terrain_point(world, 0.5, 5.5);
            let target = Vec3::new(0.5, floor.y + 1.5, 0.5);
            let angle = -2.0 + progress * 0.65;
            let origin = target + Vec3::new(angle.cos() * 12.0, 7.0, angle.sin() * 12.0);
            Camera::look_at(origin, target, 44.0, aspect)
        }
        t => {
            // Dragon scales, eyes, wings and the island behind it close the film.
            let progress = ease((t - 32.0) / 4.0);
            let target = Vec3::new(-3.0, 44.5, -2.0);
            let origin = target + Vec3::new(-17.0 + progress * 7.0, 9.0, -25.0);
            Camera::look_at(origin, target, 43.0, aspect)
        }
    }
}

fn ease(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

fn terrain_point(world: &World, x: f32, z: f32) -> Vec3 {
    let ray = Ray::new(Vec3::new(x, 90.0, z), Vec3::new(0.0, -1.0, 0.0));
    world
        .intersect(ray, 100.0)
        .expect("showcase target must lie on the central island")
        .point
}
