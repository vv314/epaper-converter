use super::harness::TempImageFile;
use crate::cli::ResizeMode;
use crate::pipeline::{
    apply_gamma_to_rgb_image, check_epaper_format, indices_to_packed_buffer,
    palette_histogram_exact, palette_histogram_nearest, resize_with_mode,
};
use crate::quantize::{
    quantize_atkinson, quantize_bayer, quantize_blue_noise, quantize_burkes, quantize_yliluoma,
    PALETTE,
};
use image::{DynamicImage, ImageBuffer, Rgb};

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
fn gamma_below_one_brightens_midtones() {
    let mut img = ImageBuffer::from_pixel(1, 1, Rgb([128, 128, 128]));
    apply_gamma_to_rgb_image(&mut img, 0.85).unwrap();
    assert!(img.get_pixel(0, 0).0[0] > 128);
}

#[test]
fn gamma_above_one_darkens_midtones() {
    let mut img = ImageBuffer::from_pixel(1, 1, Rgb([128, 128, 128]));
    apply_gamma_to_rgb_image(&mut img, 1.15).unwrap();
    assert!(img.get_pixel(0, 0).0[0] < 128);
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
fn blue_noise_quantizer_is_deterministic_and_in_palette() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([
            (x * 17) as u8,
            (y * 17) as u8,
            ((x * 19 + y * 7) % 256) as u8,
        ])
    });
    let first = quantize_blue_noise(&img, 16, 16);
    let second = quantize_blue_noise(&img, 16, 16);

    assert_eq!(first, second);
    assert_eq!(first.len(), 16 * 16);
    assert!(first.iter().all(|&idx| idx < PALETTE.len() as u8));
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
fn atkinson_quantizer_is_deterministic_with_serpentine_scan() {
    let img = ImageBuffer::from_fn(17, 11, |x, y| {
        Rgb([
            ((x * 9 + y * 5) % 256) as u8,
            ((x * 7 + y * 13) % 256) as u8,
            ((x * 3 + y * 17) % 256) as u8,
        ])
    });
    let first = quantize_atkinson(&img, 17, 11);
    let second = quantize_atkinson(&img, 17, 11);

    assert_eq!(first, second);
}

#[test]
fn burkes_quantizer_preserves_dimensions_and_palette_range() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([
            (x * 15) as u8,
            (255 - y * 9) as u8,
            ((x * 7 + y * 13) % 256) as u8,
        ])
    });
    let indices = quantize_burkes(&img, 16, 16);

    assert_eq!(indices.len(), 16 * 16);
    assert!(indices.iter().all(|&idx| idx < PALETTE.len() as u8));
}

#[test]
fn burkes_quantizer_is_deterministic_with_serpentine_scan() {
    let img = ImageBuffer::from_fn(17, 11, |x, y| {
        Rgb([
            ((x * 5 + y * 11) % 256) as u8,
            ((x * 13 + y * 7) % 256) as u8,
            ((x * 19 + y * 3) % 256) as u8,
        ])
    });
    let first = quantize_burkes(&img, 17, 11);
    let second = quantize_burkes(&img, 17, 11);

    assert_eq!(first, second);
}

#[test]
fn yliluoma_quantizer_is_deterministic_and_in_palette() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([
            (x * 17) as u8,
            (255 - y * 13) as u8,
            ((x * 11 + y * 23) % 256) as u8,
        ])
    });
    let first = quantize_yliluoma(&img, 16, 16);
    let second = quantize_yliluoma(&img, 16, 16);

    assert_eq!(first, second);
    assert_eq!(first.len(), 16 * 16);
    assert!(first.iter().all(|&idx| idx < PALETTE.len() as u8));
}

#[test]
fn palette_histograms_distinguish_exact_and_nearest_projection() {
    let img = ImageBuffer::from_fn(2, 2, |x, y| match (x, y) {
        (0, 0) => Rgb(PALETTE[0]),
        (1, 0) => Rgb(PALETTE[4]),
        (0, 1) => Rgb([10, 20, 240]),
        _ => Rgb([250, 250, 250]),
    });

    let (exact_counts, invalid) = palette_histogram_exact(&img);
    let nearest_counts = palette_histogram_nearest(&img);

    assert_eq!(exact_counts[0], 1);
    assert_eq!(exact_counts[4], 1);
    assert_eq!(invalid, 2);

    assert_eq!(nearest_counts.iter().sum::<u64>(), 4);
    assert!(
        nearest_counts[4] >= 2,
        "expected blue-ish pixels to project to blue"
    );
    assert!(
        nearest_counts[1] >= 1,
        "expected near-white pixels to project to white"
    );
}

#[test]
fn check_accepts_valid_epaper_image() {
    let path = TempImageFile::new("valid");
    let img = ImageBuffer::from_pixel(800, 480, Rgb(PALETTE[0]));
    img.save(path.path()).unwrap();

    let result = check_epaper_format(path.path(), false).unwrap();

    assert!(result);
}

#[test]
fn check_rejects_wrong_resolution() {
    let path = TempImageFile::new("wrong_size");
    let img = ImageBuffer::from_pixel(16, 16, Rgb(PALETTE[0]));
    img.save(path.path()).unwrap();

    let result = check_epaper_format(path.path(), false).unwrap();

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
