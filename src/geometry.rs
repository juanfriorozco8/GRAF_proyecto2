use crate::material::MaterialId;
use crate::math::Vec3;
use crate::scene::{Intersection, Ray};

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}

impl Bounds {
    fn union(self, other: Self) -> Self {
        Self {
            min: Vec3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Vec3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }

    fn contains(self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    fn entry(self, ray: Ray, limit: f32) -> Option<f32> {
        let mut near = 0.0_f32;
        let mut far = limit;
        for (origin, direction, inverse, min, max) in [
            (
                ray.origin.x,
                ray.direction.x,
                ray.inverse_direction.x,
                self.min.x,
                self.max.x,
            ),
            (
                ray.origin.y,
                ray.direction.y,
                ray.inverse_direction.y,
                self.min.y,
                self.max.y,
            ),
            (
                ray.origin.z,
                ray.direction.z,
                ray.inverse_direction.z,
                self.min.z,
                self.max.z,
            ),
        ] {
            if direction.abs() < 1.0e-8 {
                if origin < min || origin > max {
                    return None;
                }
            } else {
                let a = (min - origin) * inverse;
                let b = (max - origin) * inverse;
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
        }
        Some(near)
    }

    fn center_component(self, axis: usize) -> f32 {
        match axis {
            0 => self.min.x + self.max.x,
            1 => self.min.y + self.max.y,
            _ => self.min.z + self.max.z,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cuboid {
    center: Vec3,
    half: Vec3,
    axes: [Vec3; 3],
    material: MaterialId,
}

impl Cuboid {
    pub fn new(center: Vec3, size: Vec3, material: MaterialId) -> Self {
        Self::rotated(center, size, 0.0, 0.0, material)
    }

    pub fn rotated(center: Vec3, size: Vec3, yaw: f32, roll: f32, material: MaterialId) -> Self {
        let (sy, cy) = yaw.sin_cos();
        let (sr, cr) = roll.sin_cos();
        Self {
            center,
            half: size * 0.5,
            axes: [
                Vec3::new(cy * cr, sr, -sy * cr),
                Vec3::new(-cy * sr, cr, sy * sr),
                Vec3::new(sy, 0.0, cy),
            ],
            material,
        }
    }

    fn bounds(self) -> Bounds {
        let extent = Vec3::new(
            self.axes[0].x.abs() * self.half.x
                + self.axes[1].x.abs() * self.half.y
                + self.axes[2].x.abs() * self.half.z,
            self.axes[0].y.abs() * self.half.x
                + self.axes[1].y.abs() * self.half.y
                + self.axes[2].y.abs() * self.half.z,
            self.axes[0].z.abs() * self.half.x
                + self.axes[1].z.abs() * self.half.y
                + self.axes[2].z.abs() * self.half.z,
        );
        Bounds {
            min: self.center - extent,
            max: self.center + extent,
        }
    }

    fn local(self, point: Vec3) -> Vec3 {
        let relative = point - self.center;
        Vec3::new(
            relative.dot(self.axes[0]),
            relative.dot(self.axes[1]),
            relative.dot(self.axes[2]),
        )
    }

    fn contains(self, point: Vec3) -> bool {
        let point = self.local(point);
        point.x.abs() < self.half.x && point.y.abs() < self.half.y && point.z.abs() < self.half.z
    }

    fn intersect(self, ray: Ray, limit: f32) -> Option<Intersection> {
        let origin = self.local(ray.origin);
        let direction = Vec3::new(
            ray.direction.dot(self.axes[0]),
            ray.direction.dot(self.axes[1]),
            ray.direction.dot(self.axes[2]),
        );
        let mut near = f32::NEG_INFINITY;
        let mut far = f32::INFINITY;
        let mut near_normal = Vec3::ZERO;
        let mut far_normal = Vec3::ZERO;
        for (o, d, half, axis) in [
            (origin.x, direction.x, self.half.x, Vec3::new(1.0, 0.0, 0.0)),
            (origin.y, direction.y, self.half.y, Vec3::new(0.0, 1.0, 0.0)),
            (origin.z, direction.z, self.half.z, Vec3::new(0.0, 0.0, 1.0)),
        ] {
            if d.abs() < 1.0e-8 {
                if o.abs() > half {
                    return None;
                }
                continue;
            }
            let a = (-half - o) / d;
            let b = (half - o) / d;
            let (entry, exit, normal) = if a < b { (a, b, -axis) } else { (b, a, axis) };
            if entry > near {
                near = entry;
                near_normal = normal;
            }
            if exit < far {
                far = exit;
                far_normal = -normal;
            }
            if near > far {
                return None;
            }
        }
        let entering = near > 1.0e-5;
        let (distance, local_normal) = if entering {
            (near, near_normal)
        } else {
            (far, far_normal)
        };
        if distance <= 1.0e-5 || distance > limit {
            return None;
        }
        let normal = self.axes[0] * local_normal.x
            + self.axes[1] * local_normal.y
            + self.axes[2] * local_normal.z;
        Some(Intersection {
            point: ray.at(distance),
            normal,
            distance,
            material: self.material,
            entering,
            texture_point: origin + direction * distance,
            texture_normal: local_normal,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    vertices: [Vec3; 3],
    material: MaterialId,
}

impl Triangle {
    pub fn new(a: Vec3, b: Vec3, c: Vec3, material: MaterialId) -> Self {
        Self {
            vertices: [a, b, c],
            material,
        }
    }

    fn bounds(self) -> Bounds {
        self.vertices
            .iter()
            .fold(
                Bounds {
                    min: self.vertices[0],
                    max: self.vertices[0],
                },
                |bounds, &point| {
                    bounds.union(Bounds {
                        min: point,
                        max: point,
                    })
                },
            )
            .union(Bounds {
                min: self.vertices[0] - Vec3::ONE * 0.001,
                max: self.vertices[0] + Vec3::ONE * 0.001,
            })
    }

    fn intersect(self, ray: Ray, limit: f32) -> Option<Intersection> {
        let [a, b, c] = self.vertices;
        let edge_a = b - a;
        let edge_b = c - a;
        let p = ray.direction.cross(edge_b);
        let determinant = edge_a.dot(p);
        if determinant.abs() < 1.0e-8 {
            return None;
        }
        let inverse = determinant.recip();
        let offset = ray.origin - a;
        let u = offset.dot(p) * inverse;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = offset.cross(edge_a);
        let v = ray.direction.dot(q) * inverse;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let distance = edge_b.dot(q) * inverse;
        if distance <= 1.0e-5 || distance > limit {
            return None;
        }
        let normal = edge_a.cross(edge_b).normalized();
        Some(Intersection {
            point: ray.at(distance),
            normal,
            distance,
            material: self.material,
            entering: true,
            texture_point: Vec3::new(u * edge_a.length(), 0.0, v * edge_b.length()),
            texture_normal: Vec3::new(0.0, 1.0, 0.0),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Primitive {
    Box(Cuboid),
    Triangle(Triangle),
}

impl Primitive {
    fn bounds(self) -> Bounds {
        match self {
            Self::Box(shape) => shape.bounds(),
            Self::Triangle(shape) => shape.bounds(),
        }
    }

    fn intersect(self, ray: Ray, limit: f32) -> Option<Intersection> {
        match self {
            Self::Box(shape) => shape.intersect(ray, limit),
            Self::Triangle(shape) => shape.intersect(ray, limit),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum NodeKind {
    Leaf { start: usize, count: usize },
    Branch { left: usize, right: usize },
}

#[derive(Clone, Copy, Debug)]
struct Node {
    bounds: Bounds,
    kind: NodeKind,
}

#[derive(Debug, Default)]
pub struct DetailGeometry {
    primitives: Vec<Primitive>,
    nodes: Vec<Node>,
}

impl DetailGeometry {
    pub fn add_box(&mut self, shape: Cuboid) {
        self.primitives.push(Primitive::Box(shape));
    }
    pub fn add_triangle(&mut self, shape: Triangle) {
        self.primitives.push(Primitive::Triangle(shape));
    }

    pub fn build(&mut self) {
        self.nodes.clear();
        if !self.primitives.is_empty() {
            self.build_node(0, self.primitives.len());
        }
    }

    fn build_node(&mut self, start: usize, end: usize) -> usize {
        let bounds = self.primitives[start + 1..end]
            .iter()
            .fold(self.primitives[start].bounds(), |bounds, shape| {
                bounds.union(shape.bounds())
            });
        let node = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            kind: NodeKind::Leaf {
                start,
                count: end - start,
            },
        });
        if end - start > 4 {
            let extent = bounds.max - bounds.min;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            self.primitives[start..end].sort_unstable_by(|a, b| {
                a.bounds()
                    .center_component(axis)
                    .total_cmp(&b.bounds().center_component(axis))
            });
            let middle = start + (end - start) / 2;
            let left = self.build_node(start, middle);
            let right = self.build_node(middle, end);
            self.nodes[node].kind = NodeKind::Branch { left, right };
        }
        node
    }

    pub fn intersect(&self, ray: Ray, limit: f32) -> Option<Intersection> {
        if self.nodes.is_empty() || self.nodes[0].bounds.entry(ray, limit).is_none() {
            return None;
        }
        let mut hit = None;
        self.visit(0, ray, limit, &mut hit);
        hit
    }

    // The caller has already checked this node's bounds. Do not repeat the slab test.
    fn visit(&self, index: usize, ray: Ray, limit: f32, nearest: &mut Option<Intersection>) {
        let limit = nearest.as_ref().map_or(limit, |hit| hit.distance);
        let node = self.nodes[index];
        match node.kind {
            NodeKind::Leaf { start, count } => {
                for shape in &self.primitives[start..start + count] {
                    let limit = nearest.as_ref().map_or(limit, |hit| hit.distance);
                    if let Some(hit) = shape.intersect(ray, limit) {
                        *nearest = Some(hit);
                    }
                }
            }
            NodeKind::Branch { left, right } => {
                let a = self.nodes[left].bounds.entry(ray, limit);
                let b = self.nodes[right].bounds.entry(ray, limit);
                match (a, b) {
                    (None, None) => {}
                    (Some(_), None) => self.visit(left, ray, limit, nearest),
                    (None, Some(_)) => self.visit(right, ray, limit, nearest),
                    (Some(a), Some(b)) => {
                        let (first, second, second_entry) = if a <= b {
                            (left, right, b)
                        } else {
                            (right, left, a)
                        };
                        self.visit(first, ray, limit, nearest);
                        if second_entry <= nearest.as_ref().map_or(limit, |hit| hit.distance) {
                            self.visit(second, ray, limit, nearest);
                        }
                    }
                }
            }
        }
    }

    pub fn medium_at(&self, point: Vec3) -> MaterialId {
        if self.nodes.is_empty() {
            return MaterialId::Air;
        }
        let mut smallest = f32::INFINITY;
        let mut material = MaterialId::Air;
        self.containing(0, point, &mut smallest, &mut material);
        material
    }

    fn containing(&self, index: usize, point: Vec3, smallest: &mut f32, material: &mut MaterialId) {
        let node = self.nodes[index];
        if !node.bounds.contains(point) {
            return;
        }
        match node.kind {
            NodeKind::Leaf { start, count } => {
                for primitive in &self.primitives[start..start + count] {
                    if let Primitive::Box(shape) = primitive {
                        let volume = shape.half.x * shape.half.y * shape.half.z;
                        if volume < *smallest && shape.contains(point) {
                            *smallest = volume;
                            *material = shape.material;
                        }
                    }
                }
            }
            NodeKind::Branch { left, right } => {
                self.containing(left, point, smallest, material);
                self.containing(right, point, smallest, material);
            }
        }
    }
}
