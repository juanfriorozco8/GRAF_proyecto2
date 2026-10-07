mod geometry;
mod image;
mod material;
mod math;
mod renderer;
mod scene;

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

use renderer::{Camera, RenderConfig, render};
use scene::World;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    Preview,
    Render,
    Animate,
}

#[derive(Debug)]
struct Options {
    command: Command,
    width: usize,
    height: usize,
    samples: usize,
    depth: u32,
    frames: usize,
    threads: usize,
    output: PathBuf,
    seed: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = parse_options()?;
    let started = Instant::now();
    let world = World::the_end(options.seed);
    println!(
        "The End: {} voxels, isla procedural 160x160, {} hilos CPU",
        world.voxel_count(),
        options.threads
    );

    match options.command {
        Command::Preview | Command::Render => render_still(&world, &options)?,
        Command::Animate => render_animation(&world, &options)?,
    }
    println!("Proceso completado en {:.2?}", started.elapsed());
    Ok(())
}

fn render_still(world: &World, options: &Options) -> Result<(), Box<dyn Error>> {
    let camera = Camera::orbit_manual(-1.42, 130.0, 100.0, options.width, options.height);
    let config = render_config(options);
    let (image, stats) = render(world, camera, config, options.seed);
    image.save_bmp(&options.output)?;
    println!(
        "Guardado {} ({}x{}, {} spp, {} rayos primarios) en {:.2?}",
        options.output.display(),
        options.width,
        options.height,
        options.samples,
        stats.primary_rays,
        stats.elapsed
    );
    Ok(())
}

fn render_animation(world: &World, options: &Options) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(&options.output)?;
    let config = render_config(options);
    for frame in 0..options.frames {
        let progress = frame as f32 / options.frames as f32;
        let camera = Camera::orbit(progress, options.width, options.height);
        // El mismo patrón de subpíxel en todos los cuadros evita ruido temporal y parpadeo.
        let (image, stats) = render(world, camera, config, options.seed);
        let path = options.output.join(format!("frame_{frame:04}.bmp"));
        image.save_bmp(&path)?;
        println!(
            "[{}/{}] {} — {:.2?}, {} hilos",
            frame + 1,
            options.frames,
            path.display(),
            stats.elapsed,
            stats.threads
        );
    }
    println!(
        "Fotogramas listos en {}. Consulte el README para crear el MP4.",
        options.output.display()
    );
    Ok(())
}

fn render_config(options: &Options) -> RenderConfig {
    RenderConfig {
        width: options.width,
        height: options.height,
        samples_per_pixel: options.samples,
        max_depth: options.depth,
        row_band_size: 1,
        threads: options.threads,
    }
}

fn parse_options() -> Result<Options, Box<dyn Error>> {
    let mut arguments = env::args().skip(1);
    let command_name = arguments.next().unwrap_or_else(|| "preview".to_owned());
    if command_name == "--help" || command_name == "-h" {
        print_help();
        std::process::exit(0);
    }
    let command = match command_name.as_str() {
        "preview" => Command::Preview,
        "render" => Command::Render,
        "animate" => Command::Animate,
        unknown => return Err(format!("comando desconocido: {unknown}. Use --help.").into()),
    };

    let available_threads = std::thread::available_parallelism().map_or(1, usize::from);
    let (mut width, mut height, mut samples, mut depth, mut frames, mut output) = match command {
        Command::Preview => (480, 270, 2, 4, 1, PathBuf::from("renders/preview.bmp")),
        Command::Render => (1920, 1080, 16, 7, 1, PathBuf::from("renders/final.bmp")),
        Command::Animate => (960, 540, 3, 6, 360, PathBuf::from("renders/frames")),
    };
    let mut threads = available_threads;
    let mut seed = 20_261_004_u32;

    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("falta el valor para {flag}"))?;
        match flag.as_str() {
            "--width" => width = parse_positive(&value, &flag)?,
            "--height" => height = parse_positive(&value, &flag)?,
            "--samples" => samples = parse_positive(&value, &flag)?,
            "--depth" => depth = parse_positive::<u32>(&value, &flag)?,
            "--frames" => frames = parse_positive(&value, &flag)?,
            "--threads" => threads = parse_positive(&value, &flag)?,
            "--output" => output = PathBuf::from(value),
            "--seed" => {
                seed = value
                    .parse()
                    .map_err(|_| format!("valor inválido para {flag}: {value}"))?
            }
            unknown => return Err(format!("opción desconocida: {unknown}").into()),
        }
    }
    if width > 7680 || height > 4320 {
        return Err("la resolución máxima admitida es 7680x4320".into());
    }
    Ok(Options {
        command,
        width,
        height,
        samples,
        depth,
        frames,
        threads,
        output,
        seed,
    })
}

fn parse_positive<T>(value: &str, flag: &str) -> Result<T, Box<dyn Error>>
where
    T: std::str::FromStr + PartialOrd + From<u8>,
{
    let parsed: T = value
        .parse()
        .map_err(|_| format!("valor inválido para {flag}: {value}"))?;
    if parsed <= T::from(0) {
        return Err(format!("{flag} debe ser mayor que cero").into());
    }
    Ok(parsed)
}

fn print_help() {
    println!(
        "\
Raytracer CPU del diorama The End (sin dependencias externas)\n\n\
USO:\n  cargo run --release -- <preview|render|animate> [opciones]\n\n\
COMANDOS:\n\
  preview       genera una vista previa BMP\n\
  render        genera la imagen final BMP\n\
  animate       genera los fotogramas de la órbita\n\
OPCIONES:\n\
  --width N      ancho en píxeles\n\
  --height N     alto en píxeles\n\
  --samples N    muestras por píxel\n\
  --depth N      rebotes recursivos máximos\n\
  --frames N     fotogramas de la animación\n\
  --threads N    hilos de CPU\n\
  --output PATH  archivo BMP o directorio de fotogramas\n\
  --seed N       semilla determinista del terreno\n"
    );
}
