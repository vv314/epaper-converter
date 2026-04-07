pub(super) mod benchmark;
pub(super) mod check;
pub(super) mod convert;
pub(super) mod palette_report;

use super::args::DitherMode;

pub(super) use benchmark::run as run_benchmark;
pub(super) use check::run as run_check;
pub(super) use convert::run as run_convert;
pub(super) use palette_report::run as run_palette_report;

#[inline(always)]
pub(super) fn dither_mode_label(mode: DitherMode) -> &'static str {
    match mode {
        DitherMode::Bayer => "Bayer ordered dithering",
        DitherMode::BlueNoise => "Blue noise dithering",
        DitherMode::Yliluoma => "Yliluoma ordered dithering",
        DitherMode::Atkinson => "Atkinson dithering",
        DitherMode::Burkes => "Burkes dithering",
    }
}

#[inline(always)]
pub(super) fn palette_label(idx: usize) -> &'static str {
    match idx {
        0 => "black",
        1 => "white",
        2 => "red",
        3 => "yellow",
        4 => "blue",
        5 => "green",
        _ => unreachable!(),
    }
}

#[inline(always)]
pub(super) fn ratio(count: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}
