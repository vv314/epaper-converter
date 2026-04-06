use anyhow::{Context, Result};
use image::GenericImageView;
use std::path::Path;

use crate::quantize::PALETTE;

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
