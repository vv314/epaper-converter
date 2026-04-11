use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cli::DitherMode;

pub(crate) const TARGET_WIDTH: u32 = 800;
pub(crate) const TARGET_HEIGHT: u32 = 480;
pub(crate) const DEFAULT_GAMMA: f32 = 1.0;
pub(crate) const GAMMA_CASES: [(f32, &str); 3] = [(0.85, "g085"), (1.0, "g100"), (1.15, "g115")];
#[allow(dead_code)]
pub(crate) const DITHER_CASES: [(DitherMode, &str); 6] = [
    (DitherMode::Bayer, "bayer"),
    (DitherMode::BlueNoise, "blue-noise"),
    (DitherMode::Atkinson, "atkinson"),
    (DitherMode::FloydSteinberg, "floyd-steinberg"),
    (DitherMode::ClusteredDot, "clustered-dot"),
    (DitherMode::Yliluoma, "yliluoma"),
];
pub(crate) const HARNESS_DITHER_CASES: [(DitherMode, &str); 6] = [
    (DitherMode::Bayer, "bayer"),
    (DitherMode::BlueNoise, "blue-noise"),
    (DitherMode::Atkinson, "atkinson"),
    (DitherMode::FloydSteinberg, "floyd-steinberg"),
    (DitherMode::ClusteredDot, "clustered-dot"),
    (DitherMode::Yliluoma, "yliluoma"),
];

static FIXTURE_SPECS: OnceLock<Vec<HarnessFixture>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HarnessFixture {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

pub(crate) fn fixture_specs() -> Result<&'static [HarnessFixture]> {
    if let Some(fixtures) = FIXTURE_SPECS.get() {
        return Ok(fixtures.as_slice());
    }

    let loaded = load_fixture_specs()?;
    let _ = FIXTURE_SPECS.set(loaded);
    Ok(FIXTURE_SPECS
        .get()
        .expect("fixture specs initialized")
        .as_slice())
}

fn load_fixture_specs() -> Result<Vec<HarnessFixture>> {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let mut fixtures = fs::read_dir(&fixtures_dir)
        .with_context(|| {
            format!(
                "Failed to read fixtures directory: {}",
                fixtures_dir.display()
            )
        })?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| is_supported_fixture_extension(path.extension()))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().trim().to_string();
            if name.is_empty() {
                None
            } else {
                Some(HarnessFixture { name, path })
            }
        })
        .collect::<Vec<_>>();

    fixtures.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    fixtures.dedup_by(|lhs, rhs| lhs.name == rhs.name);

    anyhow::ensure!(
        !fixtures.is_empty(),
        "No supported fixtures found under {}",
        fixtures_dir.display()
    );

    Ok(fixtures)
}

fn is_supported_fixture_extension(ext: Option<&OsStr>) -> bool {
    ext.and_then(OsStr::to_str).is_some_and(|ext| {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "webp"
        )
    })
}

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
