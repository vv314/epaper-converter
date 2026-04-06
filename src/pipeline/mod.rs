mod analyze;
mod output;
mod preprocess;
mod strategy;
mod validate;

pub(crate) use analyze::{palette_histogram_exact, palette_histogram_nearest};
#[cfg(test)]
pub(crate) use output::indices_to_packed_buffer;
pub(crate) use output::{indices_to_rgb_image, save_bin_buffer, save_packed_buffer};
#[cfg(test)]
pub(crate) use preprocess::apply_gamma_to_rgb_image;
pub(crate) use preprocess::{prepare_image, resize_with_mode};
pub(crate) use strategy::choose_halftone_mode;
pub(crate) use validate::check_epaper_format;
