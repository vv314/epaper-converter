use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "epaper-converter")]
#[command(
    about = "High-performance image converter for Waveshare 7.3inch e-Paper E (ACeP 6-color)"
)]
pub(super) struct Cli {
    #[command(subcommand)]
    pub(super) command: Commands,
}

#[derive(Subcommand)]
pub(super) enum Commands {
    /// Convert an image to e-paper format
    Convert(ConvertArgs),
    /// Check if image is already in e-paper format
    Check(CheckArgs),
    /// Benchmark the converter with a test image
    Benchmark(BenchmarkArgs),
    /// Compare source projection and output palette occupancy ratios for engineering analysis
    PaletteReport(PaletteReportArgs),
}

#[derive(Args)]
pub(super) struct ConvertArgs {
    /// Input image path
    pub(super) input: String,
    /// Output image path
    pub(super) output: String,
    /// Target width (default: 800)
    #[arg(short, long, default_value = "800")]
    pub(super) width: u32,
    /// Target height (default: 480)
    #[arg(short = 'H', long, default_value = "480")]
    pub(super) height: u32,
    /// Halftone mode
    #[arg(short = 'm', long = "halftone", value_enum, default_value = "bayer")]
    pub(super) halftone: HalftoneMode,
    /// Resize strategy for fitting image into the target canvas
    #[arg(long, value_enum, default_value = "contain")]
    pub(super) resize_mode: ResizeMode,
    /// Apply EXIF orientation before resizing
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub(super) auto_rotate: bool,
    /// Apply a power-law gamma curve after resizing (1.0 keeps original; <1 brightens, >1 darkens)
    #[arg(long, default_value = "1.0")]
    pub(super) gamma: f32,
    /// Output format
    #[arg(short, long, value_enum, default_value = "bmp")]
    pub(super) format: OutputFormat,
    /// Show benchmark timing
    #[arg(short, long)]
    pub(super) benchmark: bool,
}

#[derive(Args)]
pub(super) struct CheckArgs {
    /// Input image path
    pub(super) input: String,
    /// Show detailed information
    #[arg(short, long)]
    pub(super) verbose: bool,
    /// Silent mode (only exit code)
    #[arg(short, long)]
    pub(super) quiet: bool,
}

#[derive(Args)]
pub(super) struct BenchmarkArgs {
    /// Input image path
    pub(super) input: String,
    /// Target width
    #[arg(short, long, default_value = "800")]
    pub(super) width: u32,
    /// Target height
    #[arg(short = 'H', long, default_value = "480")]
    pub(super) height: u32,
}

#[derive(Args)]
pub(super) struct PaletteReportArgs {
    /// Source image path used to build the palette projection baseline
    pub(super) source: String,
    /// Rendered image path to validate and compare against the source projection
    pub(super) rendered: String,
    /// Target width used before conversion
    #[arg(short, long, default_value = "800")]
    pub(super) width: u32,
    /// Target height used before conversion
    #[arg(short = 'H', long, default_value = "480")]
    pub(super) height: u32,
    /// Resize strategy used before conversion
    #[arg(long, value_enum, default_value = "cover")]
    pub(super) resize_mode: ResizeMode,
    /// Apply EXIF orientation before resizing source image
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub(super) auto_rotate: bool,
    /// Apply the same gamma used during conversion before projecting source colors
    #[arg(long, default_value = "1.0")]
    pub(super) gamma: f32,
    /// Allow non-palette rendered pixels and compare via nearest-palette projection instead of failing
    #[arg(long)]
    pub(super) allow_non_palette: bool,
}

#[derive(Default, Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum HalftoneMode {
    /// Bayer ordered dithering - cleaner and more stable on e-paper panels
    #[default]
    Bayer,
    /// Blue noise dithering - finer and less structured texture on gradients
    BlueNoise,
    /// Yliluoma ordered dithering - palette-aware mixing tuned for limited color panels
    Yliluoma,
    /// Atkinson dithering - sharper diffusion with less gray haze than Floyd
    Atkinson,
}

#[derive(Default, Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum ResizeMode {
    /// Stretch to target size exactly
    Stretch,
    /// Preserve aspect ratio and pad with white background
    #[default]
    Contain,
    /// Preserve aspect ratio and crop center area to fill target size
    Cover,
}

#[derive(Default, Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub(super) enum OutputFormat {
    /// Windows Bitmap - good for preview
    Bmp,
    /// Raw binary buffer - one byte per pixel (0-5), directly usable by display
    #[default]
    Bin,
    /// Packed 4-bit display buffer - two pixels per byte, ready for Waveshare driver display()
    Packed,
    /// PNG image
    Png,
    /// Both BMP and BIN
    Both,
}
