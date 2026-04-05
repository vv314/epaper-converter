use crate::cli::{HalftoneMode, ResizeMode};
use crate::pipeline::{
    check_epaper_format, choose_halftone_mode, indices_to_packed_buffer, resize_with_mode,
};
use crate::quantize::{quantize_atkinson, quantize_bayer, PALETTE};
use image::{DynamicImage, ImageBuffer, Rgb};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_file_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("epaper_converter_{name}_{nanos}.png"))
}

#[test]
fn contain_mode_pads_with_white_background() {
    let src = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 2, Rgb([0, 0, 0])));
    let resized = resize_with_mode(&src, 8, 8, ResizeMode::Contain);

    assert_eq!(resized.dimensions(), (8, 8));
    assert_eq!(resized.get_pixel(0, 0).0, [255, 255, 255]);
    assert_eq!(resized.get_pixel(4, 4).0, [0, 0, 0]);
}

#[test]
fn cover_mode_fills_target_size() {
    let src = DynamicImage::ImageRgb8(ImageBuffer::from_fn(10, 4, |x, _| {
        if x < 5 {
            Rgb([255, 0, 0])
        } else {
            Rgb([0, 0, 255])
        }
    }));
    let resized = resize_with_mode(&src, 8, 8, ResizeMode::Cover);

    assert_eq!(resized.dimensions(), (8, 8));
}

#[test]
fn auto_strategy_prefers_bayer_for_flat_image() {
    let img = ImageBuffer::from_pixel(64, 64, Rgb([255, 255, 255]));
    assert_eq!(choose_halftone_mode(&img), HalftoneMode::Bayer);
}

#[test]
fn auto_strategy_prefers_bayer_for_smooth_gradient() {
    let img = ImageBuffer::from_fn(64, 64, |x, _| {
        let value = (x * 4).min(255) as u8;
        Rgb([value, value, 255])
    });
    assert_eq!(choose_halftone_mode(&img), HalftoneMode::Bayer);
}

#[test]
fn auto_strategy_prefers_atkinson_for_complex_image() {
    let img = ImageBuffer::from_fn(128, 128, |x, y| {
        Rgb([(x * 2) as u8, (y * 2) as u8, ((x + y) % 256) as u8])
    });
    assert_eq!(choose_halftone_mode(&img), HalftoneMode::Atkinson);
}

#[test]
fn bayer_quantizer_preserves_dimensions_and_palette_range() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([(x * 17) as u8, (y * 17) as u8, ((x + y) * 8) as u8])
    });
    let indices = quantize_bayer(&img, 16, 16);

    assert_eq!(indices.len(), 16 * 16);
    assert!(indices.iter().all(|&idx| idx < PALETTE.len() as u8));
}

#[test]
fn atkinson_quantizer_preserves_dimensions_and_palette_range() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([(x * 13) as u8, (y * 11) as u8, ((x * y) % 256) as u8])
    });
    let indices = quantize_atkinson(&img, 16, 16);

    assert_eq!(indices.len(), 16 * 16);
    assert!(indices.iter().all(|&idx| idx < PALETTE.len() as u8));
}

#[test]
fn check_accepts_valid_epaper_image() {
    let path = temp_file_path("valid");
    let img = ImageBuffer::from_pixel(800, 480, Rgb(PALETTE[0]));
    img.save(&path).unwrap();

    let result = check_epaper_format(&path, false).unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(result);
}

#[test]
fn check_rejects_wrong_resolution() {
    let path = temp_file_path("wrong_size");
    let img = ImageBuffer::from_pixel(16, 16, Rgb(PALETTE[0]));
    img.save(&path).unwrap();

    let result = check_epaper_format(&path, false).unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(!result);
}

#[test]
fn packed_buffer_matches_driver_color_encoding() {
    let packed = indices_to_packed_buffer(&[0, 1, 2, 3, 4, 5]).unwrap();
    assert_eq!(packed, vec![0x01, 0x32, 0x56]);
}

#[test]
fn packed_buffer_rejects_odd_pixel_count() {
    let err = indices_to_packed_buffer(&[0, 1, 2]).unwrap_err();
    assert!(err.to_string().contains("even number of pixels"));
}
