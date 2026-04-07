mod atkinson;
mod bayer;
mod blue_noise;
mod burkes;
mod diffusion;
mod ordered;
mod palette;
mod yliluoma;

pub(crate) use atkinson::quantize_atkinson;
pub(crate) use bayer::quantize_bayer;
pub(crate) use blue_noise::quantize_blue_noise;
pub(crate) use burkes::quantize_burkes;
pub(crate) use palette::{exact_palette_index, nearest_palette_index, PALETTE};
pub(crate) use yliluoma::quantize_yliluoma;
