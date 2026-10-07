use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::math::Vec3;

#[derive(Debug)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Vec3>,
}

impl Image {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![Vec3::ZERO; width * height],
        }
    }

    pub fn save_bmp(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut writer = BufWriter::new(File::create(path)?);
        writer.write_all(&self.bmp_bytes())?;
        writer.flush()
    }

    pub fn bmp_bytes(&self) -> Vec<u8> {
        self.bmp_bytes_scaled(1.0)
    }

    pub fn bmp_bytes_scaled(&self, scale: f32) -> Vec<u8> {
        let row_bytes = self.width * 3;
        let padding = (4 - row_bytes % 4) % 4;
        let pixel_bytes = (row_bytes + padding) * self.height;
        let file_size = 54 + pixel_bytes;
        let mut output = Vec::with_capacity(file_size);

        output.extend_from_slice(b"BM");
        output.extend_from_slice(&(file_size as u32).to_le_bytes());
        output.extend_from_slice(&[0_u8; 4]);
        output.extend_from_slice(&54_u32.to_le_bytes());
        output.extend_from_slice(&40_u32.to_le_bytes());
        output.extend_from_slice(&(self.width as i32).to_le_bytes());
        output.extend_from_slice(&(self.height as i32).to_le_bytes());
        output.extend_from_slice(&1_u16.to_le_bytes());
        output.extend_from_slice(&24_u16.to_le_bytes());
        output.extend_from_slice(&0_u32.to_le_bytes());
        output.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
        output.extend_from_slice(&2835_i32.to_le_bytes());
        output.extend_from_slice(&2835_i32.to_le_bytes());
        output.extend_from_slice(&0_u32.to_le_bytes());
        output.extend_from_slice(&0_u32.to_le_bytes());

        output.resize(file_size, 0);
        if pixel_bytes == 0 {
            return output;
        }
        let row_stride = row_bytes + padding;
        let workers = if self.pixels.len() >= 65_536 {
            std::thread::available_parallelism()
                .map_or(1, usize::from)
                .min(8)
        } else {
            1
        };
        let rows_per_worker = self.height.div_ceil(workers).max(1);
        std::thread::scope(|scope| {
            for (band, bytes) in output[54..]
                .chunks_mut(row_stride * rows_per_worker)
                .enumerate()
            {
                scope.spawn(move || {
                    for (offset, row) in bytes.chunks_mut(row_stride).enumerate() {
                        let y = self.height - 1 - (band * rows_per_worker + offset);
                        let pixels = &self.pixels[y * self.width..(y + 1) * self.width];
                        for (&pixel, encoded) in
                            pixels.iter().zip(row[..row_bytes].chunks_exact_mut(3))
                        {
                            let rgb = to_srgb8(pixel * scale);
                            encoded.copy_from_slice(&[rgb[2], rgb[1], rgb[0]]);
                        }
                    }
                });
            }
        });
        output
    }
}

fn to_srgb8(linear: Vec3) -> [u8; 3] {
    let mapped = aces_tonemap(linear);
    [channel(mapped.x), channel(mapped.y), channel(mapped.z)]
}

fn aces_tonemap(color: Vec3) -> Vec3 {
    const A: f32 = 2.51;
    const B: f32 = 0.03;
    const C: f32 = 2.43;
    const D: f32 = 0.59;
    const E: f32 = 0.14;
    Vec3::new(
        (color.x * (A * color.x + B)) / (color.x * (C * color.x + D) + E),
        (color.y * (A * color.y + B)) / (color.y * (C * color.y + D) + E),
        (color.z * (A * color.z + B)) / (color.z * (C * color.z + D) + E),
    )
    .clamp(0.0, 1.0)
}

fn channel(value: f32) -> u8 {
    (value.powf(1.0 / 2.2) * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}
