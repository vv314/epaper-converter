use anyhow::{Context, Result};
use exif::{In, Reader as ExifReader, Tag};
use image::{imageops, DynamicImage, GenericImageView, ImageBuffer, Rgb, RgbImage, Rgba};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::cli::{HalftoneMode, ResizeMode};
use crate::quantize::PALETTE;

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
) -> Result<RgbImage> {
    let img = image::open(path).with_context(|| format!("Failed to open: {}", path.display()))?;

    let oriented = if auto_rotate {
        apply_exif_orientation(path, img)?
    } else {
        img
    };

    Ok(resize_with_mode(&oriented, width, height, resize_mode))
}

pub(crate) fn choose_halftone_mode(img: &RgbImage) -> HalftoneMode {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let total = width * height;
    let step = ((total / 4096).max(1) as f64).sqrt().floor() as usize;
    let step = step.max(1);

    let mut buckets = [false; 4096];
    let mut unique_count = 0usize;
    let mut diff_sum = 0u64;
    let mut diff_samples = 0u64;

    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let pixel = img.get_pixel(x as u32, y as u32);
            let bucket = ((pixel[0] >> 4) as usize) << 8
                | ((pixel[1] >> 4) as usize) << 4
                | (pixel[2] >> 4) as usize;
            if !buckets[bucket] {
                buckets[bucket] = true;
                unique_count += 1;
            }

            if x + step < width {
                let other = img.get_pixel((x + step) as u32, y as u32);
                diff_sum += (pixel[0].abs_diff(other[0]) as u64)
                    + (pixel[1].abs_diff(other[1]) as u64)
                    + (pixel[2].abs_diff(other[2]) as u64);
                diff_samples += 1;
            }
            if y + step < height {
                let other = img.get_pixel(x as u32, (y + step) as u32);
                diff_sum += (pixel[0].abs_diff(other[0]) as u64)
                    + (pixel[1].abs_diff(other[1]) as u64)
                    + (pixel[2].abs_diff(other[2]) as u64);
                diff_samples += 1;
            }
        }
    }

    let avg_diff = if diff_samples == 0 {
        0.0
    } else {
        diff_sum as f64 / diff_samples as f64
    };

    if unique_count <= 8 && avg_diff < 12.0 {
        HalftoneMode::Bayer
    } else if unique_count > 96 || avg_diff > 48.0 {
        HalftoneMode::Atkinson
    } else {
        HalftoneMode::BlueNoise
    }
}

pub(crate) fn indices_to_rgb_image(indices: &[u8], width: u32, height: u32) -> RgbImage {
    let mut rgb_img = ImageBuffer::new(width, height);

    for (idx, &color_idx) in indices.iter().enumerate() {
        let x = (idx as u32) % width;
        let y = (idx as u32) / width;
        let color = PALETTE[color_idx as usize];
        rgb_img.put_pixel(x, y, Rgb(color));
    }

    rgb_img
}

pub(crate) fn save_bin_buffer(indices: &[u8], path: &Path) -> Result<()> {
    std::fs::write(path, indices)
        .with_context(|| format!("Failed to write bin file: {}", path.display()))?;
    Ok(())
}

pub(crate) fn indices_to_packed_buffer(indices: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(
        indices.len() % 2 == 0,
        "Packed display buffer requires an even number of pixels, got {}",
        indices.len()
    );

    #[inline(always)]
    fn map_color(idx: u8) -> Result<u8> {
        match idx {
            0 => Ok(0x0),
            1 => Ok(0x1),
            2 => Ok(0x3),
            3 => Ok(0x2),
            4 => Ok(0x5),
            5 => Ok(0x6),
            _ => anyhow::bail!("Invalid palette index for packed output: {idx}"),
        }
    }

    let mut packed = Vec::with_capacity(indices.len() / 2);
    for pair in indices.chunks_exact(2) {
        let left = map_color(pair[0])?;
        let right = map_color(pair[1])?;
        packed.push((left << 4) | right);
    }

    Ok(packed)
}

pub(crate) fn save_packed_buffer(indices: &[u8], path: &Path) -> Result<()> {
    let packed = indices_to_packed_buffer(indices)?;
    std::fs::write(path, packed)
        .with_context(|| format!("Failed to write packed file: {}", path.display()))?;
    Ok(())
}

pub(crate) fn check_epaper_format(path: &Path, verbose: bool) -> Result<bool> {
    let img =
        image::open(path).with_context(|| format!("Failed to open image: {}", path.display()))?;

    let (width, height) = img.dimensions();

    if verbose {
        println!("Image: {}", path.display());
        println!("  Resolution: {}x{}", width, height);
        println!("  Color type: {:?}", img.color());
    }

    if width != 800 || height != 480 {
        if verbose {
            println!("  [FAIL] Resolution mismatch (expected 800x480)");
        }
        return Ok(false);
    }

    let rgb_img = img.to_rgb8();
    let mut colors_found = [false; 6];
    let mut invalid_count = 0;

    for pixel in rgb_img.pixels() {
        let r = pixel[0];
        let g = pixel[1];
        let b = pixel[2];

        let mut found = false;
        for (idx, color) in PALETTE.iter().enumerate() {
            if r == color[0] && g == color[1] && b == color[2] {
                colors_found[idx] = true;
                found = true;
                break;
            }
        }

        if !found {
            invalid_count += 1;
        }
    }

    let valid_colors = colors_found.iter().filter(|&&x| x).count();

    if verbose {
        println!("  Valid palette colors: {}/6", valid_colors);
        println!("  Invalid pixels: {}", invalid_count);

        let names = ["Black", "White", "Red", "Yellow", "Blue", "Green"];
        for (idx, found) in colors_found.iter().enumerate() {
            println!(
                "    {}: {}",
                names[idx],
                if *found { "[OK]" } else { "[MISSING]" }
            );
        }
    }

    let is_valid = invalid_count == 0 && valid_colors > 0;

    if verbose {
        if is_valid {
            println!("  [OK] Image is ready for e-paper display");
        } else {
            println!("  [FAIL] Image needs conversion");
        }
    }

    Ok(is_valid)
}
