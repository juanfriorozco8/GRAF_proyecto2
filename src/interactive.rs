use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::time::Duration;

use crate::image::Image;
use crate::renderer::{Camera, RenderConfig, render_sample_batch};
use crate::scene::World;

const ADDRESS: &str = "127.0.0.1:7878";
const VIEWER_SCRIPT: &[u8] = include_bytes!("viewer.js");
const PAGE: &str = r#"<!doctype html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>The End</title>
  <style>
    :root { color-scheme: dark; font-family: system-ui, -apple-system, sans-serif; }
    * { box-sizing: border-box; }
    body { margin: 0; height: 100vh; background: #08050d; overflow: hidden; }
    #view { display: block; width: 100vw; height: 100vh; object-fit: contain; image-rendering: pixelated; }
    .controls { position: fixed; right: 12px; bottom: 12px; padding: 7px 10px;
      background: #0b0714a6; border-left: 1px solid #9661b54d; color: #fff;
      font-size: 11px; font-weight: 400; line-height: 1.65; pointer-events: none; }
    #status { color: #d5c5df; font-size: 10px; }
    #status:empty { display: none; }
    @media (max-width: 480px) { .controls { right: 8px; bottom: 8px; font-size: 10px; } }
  </style>
  <script src="/viewer.js" defer></script>
</head>
<body>
  <img id="view" alt="Diorama de The End" decoding="async">
  <aside class="controls" aria-label="Controles de cámara">
    <div>A / D girar · W / S acercar y alejar</div>
    <div>R / F altura · 0 restablecer</div>
    <div id="status" role="status">Preparando…</div>
  </aside>
</body>
</html>
"#;

#[derive(Clone, Copy, Debug)]
pub struct InteractiveConfig {
    pub width: usize,
    pub height: usize,
    pub samples_per_pixel: usize,
    pub max_depth: u32,
    pub threads: usize,
    pub seed: u32,
}

pub fn serve(world: &World, config: InteractiveConfig) -> std::io::Result<()> {
    let listener = TcpListener::bind(ADDRESS)?;
    let url = format!("http://{ADDRESS}");
    println!("Visor interactivo disponible en {url}");
    println!("Controles: A/D girar, W/S zoom, R/F altura, 0 restablecer, Ctrl+C salir.");
    if std::env::var_os("THE_END_NO_OPEN").is_none() {
        let _ = Command::new("open").arg(&url).spawn();
    }

    let mut cache = FrameCache::default();
    for connection in listener.incoming() {
        match connection {
            Ok(mut stream) => {
                if let Err(error) = handle_request(&mut stream, world, config, &mut cache) {
                    eprintln!("Error atendiendo el visor: {error}");
                }
            }
            Err(error) => eprintln!("Error de conexión: {error}"),
        }
    }
    Ok(())
}

fn handle_request(
    stream: &mut TcpStream,
    world: &World,
    config: InteractiveConfig,
    cache: &mut FrameCache,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut request = [0_u8; 4096];
    let bytes_read = stream.read(&mut request)?;
    let request = String::from_utf8_lossy(&request[..bytes_read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    if path == "/" {
        return respond(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
        );
    }
    if path == "/viewer.js" {
        return respond(
            stream,
            "200 OK",
            "text/javascript; charset=utf-8",
            VIEWER_SCRIPT,
        );
    }
    if path.starts_with("/render?") {
        let frame = cache.frame(world, config, CameraState::from_path(path));
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/bmp\r\nContent-Length: {}\r\nX-Render-Samples: {}\r\nX-Target-Samples: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            frame.bytes.len(),
            frame.samples,
            frame.target_samples,
        );
        stream.write_all(headers.as_bytes())?;
        stream.write_all(&frame.bytes)?;
        return stream.flush();
    }
    respond(
        stream,
        "404 Not Found",
        "text/plain; charset=utf-8",
        b"No encontrado",
    )
}

fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

#[derive(Debug, Default)]
struct FrameCache {
    pose: Option<CameraState>,
    accumulated: Option<Image>,
    samples: usize,
}

#[derive(Debug)]
struct FrameResponse {
    bytes: Vec<u8>,
    samples: usize,
    target_samples: usize,
}

impl FrameCache {
    fn frame(
        &mut self,
        world: &World,
        config: InteractiveConfig,
        pose: CameraState,
    ) -> FrameResponse {
        let target_samples = config.samples_per_pixel.max(1);
        let same_pose = self.pose.is_some_and(|previous| previous.same_pose(pose));
        if !same_pose {
            self.samples = 0;
            self.accumulated = None;
        }
        let requested_samples = if pose.fast { 1 } else { target_samples };
        if self.samples < requested_samples {
            let camera = Camera::orbit_manual(
                pose.angle,
                pose.radius,
                pose.elevation,
                config.width,
                config.height,
            );
            let render_config = RenderConfig {
                width: config.width,
                height: config.height,
                samples_per_pixel: 1,
                max_depth: config.max_depth.min(5),
                row_band_size: 1,
                threads: config.threads,
            };
            // Each HTTP request traces at most one sample. Refinement cannot hold
            // up camera motion for the duration of a four-sample batch.
            let (sample, _) =
                render_sample_batch(world, camera, render_config, config.seed, self.samples);
            if let Some(accumulated) = &mut self.accumulated {
                for (sum, next) in accumulated.pixels.iter_mut().zip(sample.pixels) {
                    *sum += next;
                }
            } else {
                self.accumulated = Some(sample);
            }
            self.samples += 1;
            self.pose = Some(pose);
        }
        FrameResponse {
            bytes: self
                .accumulated
                .as_ref()
                .expect("frame has at least one sample")
                .bmp_bytes_scaled(1.0 / self.samples as f32),
            samples: self.samples,
            target_samples,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CameraState {
    angle: f32,
    radius: f32,
    elevation: f32,
    fast: bool,
}

impl CameraState {
    fn same_pose(self, other: Self) -> bool {
        self.angle == other.angle
            && self.radius == other.radius
            && self.elevation == other.elevation
    }

    fn from_path(path: &str) -> Self {
        let mut state = Self {
            angle: -1.42,
            radius: 130.0,
            elevation: 100.0,
            fast: false,
        };
        let Some((_, query)) = path.split_once('?') else {
            return state;
        };
        for field in query.split('&') {
            let Some((name, value)) = field.split_once('=') else {
                continue;
            };
            if name == "quality" {
                state.fast = value == "fast";
                continue;
            }
            let Ok(number) = value.parse::<f32>() else {
                continue;
            };
            match name {
                "angle" if number.is_finite() => state.angle = number,
                "radius" if number.is_finite() => state.radius = number.clamp(85.0, 210.0),
                "elevation" if number.is_finite() => state.elevation = number.clamp(55.0, 155.0),
                _ => {}
            }
        }
        state
    }
}
