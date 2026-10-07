use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::image::Image;
use crate::material::{Material, MaterialId, mapped_normal, texture_color};
use crate::math::{Vec3, hash_3d, hash_u32};
use crate::scene::{PointLight, Ray, World};

const RAY_EPSILON: f32 = 1.0e-3;
const MAX_DISTANCE: f32 = 600.0;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub origin: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    tan_half_fov: f32,
    aspect_ratio: f32,
}

impl Camera {
    pub fn look_at(
        origin: Vec3,
        target: Vec3,
        vertical_fov_degrees: f32,
        aspect_ratio: f32,
    ) -> Self {
        let forward = (target - origin).normalized();
        let world_up = Vec3::new(0.0, 1.0, 0.0);
        let right = forward.cross(world_up).normalized();
        let up = right.cross(forward).normalized();
        Self {
            origin,
            forward,
            right,
            up,
            tan_half_fov: (vertical_fov_degrees.to_radians() * 0.5).tan(),
            aspect_ratio,
        }
    }

    pub fn ray(self, normalized_x: f32, normalized_y: f32) -> Ray {
        // Same screen-space normalization taught in class, extended with a camera basis.
        let screen_x = (2.0 * normalized_x - 1.0) * self.aspect_ratio * self.tan_half_fov;
        let screen_y = (1.0 - 2.0 * normalized_y) * self.tan_half_fov;
        Ray::new(
            self.origin,
            self.forward + self.right * screen_x + self.up * screen_y,
        )
    }

    pub fn orbit(progress: f32, width: usize, height: usize) -> Self {
        let eased_zoom = 0.5 - 0.5 * (TAU * progress).cos();
        let radius = 130.0 - eased_zoom * 15.0;
        let angle = -1.42 + progress * TAU;
        let elevation = 100.0 + (progress * PI).sin() * 5.0;
        Self::orbit_manual(angle, radius, elevation, width, height)
    }

    pub fn orbit_manual(
        angle: f32,
        radius: f32,
        elevation: f32,
        width: usize,
        height: usize,
    ) -> Self {
        let origin = Vec3::new(angle.cos() * radius, elevation, angle.sin() * radius);
        Self::look_at(
            origin,
            Vec3::new(0.0, 30.0, 0.0),
            48.0,
            width as f32 / height as f32,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderConfig {
    pub width: usize,
    pub height: usize,
    pub samples_per_pixel: usize,
    pub max_depth: u32,
    pub row_band_size: usize,
    pub threads: usize,
}

#[derive(Debug)]
pub struct RenderStats {
    pub elapsed: Duration,
    pub primary_rays: usize,
    pub threads: usize,
}

#[derive(Clone, Copy, Debug)]
struct OpticalResponse {
    local_weight: f32,
    reflection_weight: f32,
    transmission_weight: f32,
    transmitted_direction: Option<Vec3>,
}

fn optical_response(
    material: Material,
    direction: Vec3,
    face_normal: Vec3,
    source_ior: f32,
    target_ior: f32,
) -> OpticalResponse {
    let transmitted_direction = if material.transparency > 0.0 {
        direction.refract(face_normal, source_ior / target_ior)
    } else {
        None
    };
    let incident_cosine = (-direction).dot(face_normal).clamp(0.0, 1.0);
    // Por encima del ángulo crítico no existe rayo transmitido: toda la fracción
    // transparente debe reflejarse. Los tres pesos conservan una suma de uno.
    let fresnel = if material.transparency > 0.0 && transmitted_direction.is_none() {
        1.0
    } else {
        schlick(incident_cosine, source_ior, target_ior)
    };
    OpticalResponse {
        local_weight: (1.0 - material.transparency) * (1.0 - material.reflectivity),
        reflection_weight: ((1.0 - material.transparency) * material.reflectivity
            + material.transparency * fresnel)
            .clamp(0.0, 1.0),
        transmission_weight: (material.transparency * (1.0 - fresnel)).clamp(0.0, 1.0),
        transmitted_direction,
    }
}

pub fn render(
    world: &World,
    camera: Camera,
    config: RenderConfig,
    frame_seed: u32,
) -> (Image, RenderStats) {
    render_sample_batch(world, camera, config, frame_seed, 0)
}

/// Uses absolute sample indices so progressive refinement matches a batch render.
pub fn render_sample_batch(
    world: &World,
    camera: Camera,
    config: RenderConfig,
    frame_seed: u32,
    first_sample: usize,
) -> (Image, RenderStats) {
    let started = Instant::now();
    let mut image = Image::new(config.width, config.height);
    // Each task owns disjoint rows. Workers write directly into the final image;
    // there is no per-pixel index buffer or serial copy after rendering.
    let band_height = config.row_band_size.max(1);
    let bands: VecDeque<_> = image
        .pixels
        .chunks_mut(config.width * band_height)
        .enumerate()
        .map(|(band, pixels)| (band * band_height, pixels))
        .collect();
    let worker_count = config.threads.max(1).min(bands.len().max(1));
    let queue = Mutex::new(bands);
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = &queue;
            scope.spawn(move || {
                loop {
                    // Drop the queue lock before tracing any pixels.
                    let task = { queue.lock().expect("render queue poisoned").pop_front() };
                    let Some((y_start, pixels)) = task else {
                        break;
                    };
                    for (offset, pixel) in pixels.iter_mut().enumerate() {
                        let x = offset % config.width;
                        let y = y_start + offset / config.width;
                        let mut color = Vec3::ZERO;
                        for sample in first_sample..first_sample + config.samples_per_pixel {
                            let seed = hash_u32(
                                (x as u32).wrapping_mul(73_856_093)
                                    ^ (y as u32).wrapping_mul(19_349_663)
                                    ^ (sample as u32).wrapping_mul(83_492_791)
                                    ^ frame_seed,
                            );
                            let (jitter_x, jitter_y) = if sample == 0 {
                                (0.5, 0.5)
                            } else {
                                (random01(seed), random01(seed ^ 0xa511_e9b3))
                            };
                            let ray = camera.ray(
                                (x as f32 + jitter_x) / config.width as f32,
                                (y as f32 + jitter_y) / config.height as f32,
                            );
                            color += trace(world, ray, config.max_depth, MaterialId::Air);
                        }
                        *pixel = color / config.samples_per_pixel as f32;
                    }
                }
            });
        }
    });
    let stats = RenderStats {
        elapsed: started.elapsed(),
        primary_rays: config.width * config.height * config.samples_per_pixel,
        threads: worker_count,
    };
    (image, stats)
}

fn trace(world: &World, ray: Ray, depth: u32, current_medium: MaterialId) -> Vec3 {
    if depth == 0 {
        return skybox(ray.direction) * 0.12;
    }
    let Some(hit) = world.intersect(ray, MAX_DISTANCE) else {
        return skybox(ray.direction);
    };
    let material = hit.material.properties();
    let outward_normal = hit.normal;
    let face_normal = if ray.direction.dot(outward_normal) < 0.0 {
        outward_normal
    } else {
        -outward_normal
    };
    let shading_normal = mapped_normal(hit.material, hit.point, face_normal);
    let albedo = texture_color(hit.material, hit.texture_point, hit.texture_normal);
    let view_direction = -ray.direction;

    let mut direct = albedo.hadamard(Vec3::new(0.12, 0.12, 0.15));
    let sun_direction = Vec3::new(-0.52, 0.78, -0.34).normalized();
    if shading_normal.dot(sun_direction) > 0.0 {
        let sun_visibility = shadow_visibility(
            world,
            hit.point + face_normal * RAY_EPSILON,
            sun_direction,
            220.0,
        );
        direct += shade_light(
            albedo,
            shading_normal,
            view_direction,
            sun_direction,
            Vec3::new(0.88, 0.91, 0.78),
            0.78 * sun_visibility,
            material.diffuse,
            material.specular,
            material.shininess,
        );
    }

    for light in &world.point_lights {
        direct += shade_point_light(
            world,
            *light,
            hit.point,
            face_normal,
            shading_normal,
            view_direction,
            albedo,
            material.diffuse,
            material.specular,
            material.shininess,
        );
    }

    let transmitted_medium = if material.transparency > 0.0 {
        world.medium_at(hit.point - face_normal * RAY_EPSILON)
    } else {
        MaterialId::Air
    };
    let source_ior = current_medium.properties().refractive_index;
    let target_ior = transmitted_medium.properties().refractive_index;
    let optics = optical_response(material, ray.direction, face_normal, source_ior, target_ior);

    let mut color = material.emission + direct * optics.local_weight;
    if optics.reflection_weight > 0.001 {
        let reflected_ray = Ray::new(
            hit.point + face_normal * RAY_EPSILON,
            ray.direction.reflect(face_normal),
        );
        color += trace(world, reflected_ray, depth - 1, current_medium) * optics.reflection_weight;
    }
    if let Some(refracted_direction) = optics
        .transmitted_direction
        .filter(|_| optics.transmission_weight > 0.001)
    {
        let refracted_ray = Ray::new(hit.point - face_normal * RAY_EPSILON, refracted_direction);
        color +=
            trace(world, refracted_ray, depth - 1, transmitted_medium) * optics.transmission_weight;
    }

    if current_medium != MaterialId::Air {
        let absorption = current_medium.properties().absorption;
        color = color.hadamard(Vec3::new(
            (-absorption.x * hit.distance).exp(),
            (-absorption.y * hit.distance).exp(),
            (-absorption.z * hit.distance).exp(),
        ));
    }
    color
}

#[allow(clippy::too_many_arguments)]
fn shade_point_light(
    world: &World,
    light: PointLight,
    point: Vec3,
    face_normal: Vec3,
    shading_normal: Vec3,
    view_direction: Vec3,
    albedo: Vec3,
    diffuse: f32,
    specular: f32,
    shininess: f32,
) -> Vec3 {
    let to_light = light.position - point;
    let distance_squared = to_light.length_squared().max(0.25);
    let distance = distance_squared.sqrt();
    let direction = to_light / distance;
    // No lanzar rayos de sombra para luces detrás de la superficie ni para aportes
    // imperceptibles. Esta salida temprana mantiene fluido el visor con luces locales de alcance limitado.
    if shading_normal.dot(direction) <= 0.0 {
        return Vec3::ZERO;
    }
    let attenuation = light.intensity / (1.0 + distance_squared);
    if attenuation < 0.012 {
        return Vec3::ZERO;
    }
    let visibility = shadow_visibility(
        world,
        point + face_normal * RAY_EPSILON,
        direction,
        distance - 0.02,
    );
    shade_light(
        albedo,
        shading_normal,
        view_direction,
        direction,
        light.color,
        attenuation * visibility,
        diffuse,
        specular,
        shininess,
    )
}

#[allow(clippy::too_many_arguments)]
fn shade_light(
    albedo: Vec3,
    normal: Vec3,
    view_direction: Vec3,
    light_direction: Vec3,
    light_color: Vec3,
    intensity: f32,
    diffuse_factor: f32,
    specular_factor: f32,
    shininess: f32,
) -> Vec3 {
    let lambert = normal.dot(light_direction).max(0.0);
    if lambert <= 0.0 || intensity <= 0.0 {
        return Vec3::ZERO;
    }
    let half_vector = (light_direction + view_direction).normalized();
    let specular = normal.dot(half_vector).max(0.0).powf(shininess) * specular_factor;
    (albedo * (lambert * diffuse_factor) + Vec3::ONE * specular).hadamard(light_color) * intensity
}

fn shadow_visibility(world: &World, origin: Vec3, direction: Vec3, max_distance: f32) -> f32 {
    let mut ray = Ray::new(origin, direction);
    let mut remaining = max_distance;
    let mut visibility = 1.0;
    while remaining > RAY_EPSILON {
        let Some(hit) = world.intersect(ray, remaining) else {
            return visibility;
        };
        let transmission = match hit.material {
            MaterialId::Glass => 0.62,
            MaterialId::EndCrystal => 0.48,
            _ => return 0.0,
        };
        if hit.entering {
            visibility *= transmission;
            if visibility < 0.001 {
                return 0.0;
            }
        }
        let advance = hit.distance + RAY_EPSILON;
        remaining -= advance;
        ray.origin = ray.at(advance);
    }
    visibility
}

fn schlick(cosine: f32, source_ior: f32, target_ior: f32) -> f32 {
    let base = ((source_ior - target_ior) / (source_ior + target_ior)).powi(2);
    base + (1.0 - base) * (1.0 - cosine).powi(5)
}

fn skybox(direction: Vec3) -> Vec3 {
    let vertical = (direction.y * 0.5 + 0.5).clamp(0.0, 1.0);
    let color = Vec3::new(0.006, 0.004, 0.011).lerp(Vec3::new(0.010, 0.007, 0.017), vertical);
    let longitude = direction.z.atan2(direction.x) / TAU + 0.5;
    let latitude = direction.y.clamp(-1.0, 1.0).asin() / PI + 0.5;
    let noise = hash_3d(
        (longitude * 4096.0) as i32,
        (latitude * 2048.0) as i32,
        0,
        0xe11d,
    );
    color + Vec3::new(0.0015, 0.0010, 0.0020) * noise
}

fn random01(seed: u32) -> f32 {
    hash_u32(seed) as f32 / u32::MAX as f32
}
