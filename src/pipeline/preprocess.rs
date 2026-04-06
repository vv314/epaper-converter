use anyhow::{Context, Result};
use exif::{In, Reader as ExifReader, Tag};
use image::{imageops, DynamicImage, ImageBuffer, RgbImage, Rgba};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::cli::ResizeMode;

pub(crate) fn resize_with_mode(
    img: &DynamicImage,
    width: u32,
    height: u32,
    mode: ResizeMode,
) -> RgbImage {
    match mode {
        ResizeMode::Stretch => img
            .resize_exact(width, height, imageops::FilterType::Lanczos3)
            .to_rgb8(),
        ResizeMode::Contain => {
            let resized = img
                .resize(width, height, imageops::FilterType::Lanczos3)
                .to_rgba8();
            let x = i64::from((width.saturating_sub(resized.width())) / 2);
            let y = i64::from((height.saturating_sub(resized.height())) / 2);
            let mut canvas = ImageBuffer::from_pixel(width, height, Rgba([255, 255, 255, 255]));
            imageops::overlay(&mut canvas, &resized, x, y);
            DynamicImage::ImageRgba8(canvas).to_rgb8()
        }
        ResizeMode::Cover => {
            let src_w = img.width();
            let src_h = img.height();
            let target_ratio = width as f32 / height as f32;
            let src_ratio = src_w as f32 / src_h as f32;

            let (crop_w, crop_h) = if src_ratio > target_ratio {
                (
                    ((src_h as f32 * target_ratio).round() as u32).min(src_w),
                    src_h,
                )
            } else {
                (
                    src_w,
                    ((src_w as f32 / target_ratio).round() as u32).min(src_h),
                )
            };

            let x = (src_w - crop_w) / 2;
            let y = (src_h - crop_h) / 2;
            img.crop_imm(x, y, crop_w, crop_h)
                .resize_exact(width, height, imageops::FilterType::Lanczos3)
                .to_rgb8()
        }
    }
}

fn apply_exif_orientation(path: &Path, img: DynamicImage) -> Result<DynamicImage> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Ok(img),
    };
    let mut reader = BufReader::new(file);
    let exif = match ExifReader::new().read_from_container(&mut reader) {
        Ok(exif) => exif,
        Err(_) => return Ok(img),
    };
    let Some(field) = exif.get_field(Tag::Orientation, In::PRIMARY) else {
        return Ok(img);
    };

    let orientation = field.value.get_uint(0).unwrap_or(1);
    let oriented = match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    };

    Ok(oriented)
}

pub(crate) fn prepare_image(
    path: &Path,
    width: u32,
    height: u32,
    resize_mode: ResizeMode,
    auto_rotate: bool,
    gamma: f32,
) -> Result<RgbImage> {
    let img = image::open(path).with_context(|| format!("Failed to open: {}", path.display()))?;

    let oriented = if auto_rotate {
        apply_exif_orientation(path, img)?
    } else {
        img
    };

    let mut rgb = resize_with_mode(&oriented, width, height, resize_mode);
    apply_gamma_to_rgb_image(&mut rgb, gamma)?;
    Ok(rgb)
}

pub(crate) fn apply_gamma_to_rgb_image(img: &mut RgbImage, gamma: f32) -> Result<()> {
    anyhow::ensure!(gamma.is_finite() && gamma > 0.0, "Gamma must be a finite value greater than 0");

    if (gamma - 1.0).abs() < f32::EPSILON {
        return Ok(());
    }

    for pixel in img.pixels_mut() {
        for channel in &mut pixel.0 {
            let normalized = (*channel as f32) / 255.0;
            let corrected = normalized.powf(gamma);
            *channel = (corrected * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    Ok(())
}
