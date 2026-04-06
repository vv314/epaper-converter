mod color;
mod error_diffusion;
mod ordered;
mod yliluoma;

pub(crate) use color::{exact_palette_index, nearest_palette_index, PALETTE};
pub(crate) use error_diffusion::quantize_atkinson;
pub(crate) use ordered::{quantize_bayer, quantize_blue_noise};
pub(crate) use yliluoma::quantize_yliluoma;
