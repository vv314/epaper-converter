use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cli::HalftoneMode;

pub(crate) const TARGET_WIDTH: u32 = 800;
pub(crate) const TARGET_HEIGHT: u32 = 480;
pub(crate) const DEFAULT_GAMMA: f32 = 1.0;
pub(crate) const FIXTURE_NAMES: [&str; 3] = ["gradient", "starry_night", "tree"];
pub(crate) const GAMMA_CASES: [(f32, &str); 3] = [(0.85, "g085"), (1.0, "g100"), (1.15, "g115")];
#[allow(dead_code)]
pub(crate) const HALFTONE_CASES: [(HalftoneMode, &str); 5] = [
    (HalftoneMode::Bayer, "bayer"),
    (HalftoneMode::BlueNoise, "blue-noise"),
    (HalftoneMode::Atkinson, "atkinson"),
    (HalftoneMode::Yliluoma, "yliluoma"),
    (HalftoneMode::Auto, "auto"),
];
pub(crate) const HARNESS_HALFTONE_CASES: [(HalftoneMode, &str); 4] = [
    (HalftoneMode::Bayer, "bayer"),
    (HalftoneMode::BlueNoise, "blue-noise"),
    (HalftoneMode::Atkinson, "atkinson"),
    (HalftoneMode::Auto, "auto"),
];

pub(crate) struct TempImageFile {
    path: PathBuf,
}

impl TempImageFile {
    pub(crate) fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        Self {
            path: std::env::temp_dir().join(format!("epaper_converter_{label}_{nanos}.png")),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempImageFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
