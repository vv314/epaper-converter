use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb, RgbImage};
use std::path::Path;

use crate::quantize::PALETTE;

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
