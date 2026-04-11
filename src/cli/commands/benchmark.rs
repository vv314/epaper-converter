use anyhow::{Context, Result};
use std::hint::black_box;
use std::time::Instant;

use crate::cli::args::{BenchmarkArgs, ResizeMode};
use crate::pipeline::{indices_to_rgb_image, resize_with_mode};
use crate::quantize::{
    quantize_atkinson, quantize_bayer, quantize_blue_noise, quantize_clustered_dot,
    quantize_floyd_steinberg, quantize_yliluoma,
};

pub(in crate::cli) fn run(args: BenchmarkArgs) -> Result<()> {
    let BenchmarkArgs {
        input,
        width,
        height,
    } = args;

    println!("=== Benchmarking {} ===", input);

    let img = image::open(&input).context("Failed to open image")?;
    let rgb_img = resize_with_mode(&img, width, height, ResizeMode::Stretch);

    let start = Instant::now();
    let indices_bayer = black_box(quantize_bayer(&rgb_img, width, height));
    let bayer_time = start.elapsed();

    let start = Instant::now();
    black_box(quantize_blue_noise(&rgb_img, width, height));
    let blue_noise_time = start.elapsed();

    let start = Instant::now();
    black_box(quantize_yliluoma(&rgb_img, width, height));
    let yliluoma_time = start.elapsed();

    let start = Instant::now();
    black_box(quantize_atkinson(&rgb_img, width, height));
    let atkinson_time = start.elapsed();

    let start = Instant::now();
    black_box(quantize_floyd_steinberg(&rgb_img, width, height));
    let floyd_steinberg_time = start.elapsed();

    let start = Instant::now();
    black_box(quantize_clustered_dot(&rgb_img, width, height));
    let clustered_dot_time = start.elapsed();

    let start = Instant::now();
    let _rgb_out = indices_to_rgb_image(&indices_bayer, width, height);
    let convert_time = start.elapsed();

    println!("=== Results ({}x{}) ===", width, height);
    println!(
        "Bayer mode:    {:>8.2}ms",
        bayer_time.as_secs_f64() * 1000.0
    );
    println!(
        "Blue noise:    {:>8.2}ms",
        blue_noise_time.as_secs_f64() * 1000.0
    );
    println!(
        "Yliluoma:      {:>8.2}ms",
        yliluoma_time.as_secs_f64() * 1000.0
    );
    println!(
        "Atkinson mode:{:>8.2}ms",
        atkinson_time.as_secs_f64() * 1000.0
    );
    println!(
        "Floyd baseline:{:>6.2}ms",
        floyd_steinberg_time.as_secs_f64() * 1000.0
    );
    println!(
        "Clustered-dot:{:>7.2}ms",
        clustered_dot_time.as_secs_f64() * 1000.0
    );
    println!(
        "RGB convert:   {:>8.2}ms",
        convert_time.as_secs_f64() * 1000.0
    );
    println!(
        "Total Bayer:  {:>8.2}ms",
        (bayer_time + convert_time).as_secs_f64() * 1000.0
    );
    println!(
        "Total Blue:   {:>8.2}ms",
        (blue_noise_time + convert_time).as_secs_f64() * 1000.0
    );
    println!(
        "Total Yliluoma:{:>7.2}ms",
        (yliluoma_time + convert_time).as_secs_f64() * 1000.0
    );
    println!(
        "Total Atkinson:{:>8.2}ms",
        (atkinson_time + convert_time).as_secs_f64() * 1000.0
    );
    println!(
        "Total baseline:{:>5.2}ms",
        (floyd_steinberg_time + convert_time).as_secs_f64() * 1000.0
    );
    println!(
        "Total Cluster:{:>8.2}ms",
        (clustered_dot_time + convert_time).as_secs_f64() * 1000.0
    );

    Ok(())
}
