use lab::Lab;
use std::sync::OnceLock;

pub(crate) const PALETTE: [[u8; 3]; 6] = [
    [0, 0, 0],       // Black (0)
    [255, 255, 255], // White (1)
    [255, 0, 0],     // Red (2)
    [255, 255, 0],   // Yellow (3)
    [0, 0, 255],     // Blue (4)
    [0, 255, 0],     // Green (5)
];

const LUT_BITS: usize = 6;
const LUT_SIZE: usize = 1 << (LUT_BITS * 3);
const LUT_MASK_USIZE: usize = (1 << LUT_BITS) - 1;

static COLOR_LUT: OnceLock<Box<[u8]>> = OnceLock::new();
static PALETTE_LAB: OnceLock<Box<[[f32; 3]]>> = OnceLock::new();
static PALETTE_LINEAR: OnceLock<Box<[[f32; 3]]>> = OnceLock::new();
static PALETTE_LUMA: OnceLock<Box<[f32]>> = OnceLock::new();

#[inline(always)]
fn weighted_distance(r: u8, g: u8, b: u8, color: [u8; 3]) -> u32 {
    let dr = r as i32 - color[0] as i32;
    let dg = g as i32 - color[1] as i32;
    let db = b as i32 - color[2] as i32;

    (dr * dr * 299 + dg * dg * 587 + db * db * 114) as u32
}

#[inline(always)]
fn color_lut() -> &'static [u8] {
    COLOR_LUT.get_or_init(|| {
        let mut lut = vec![0u8; LUT_SIZE];

        for packed in 0..LUT_SIZE {
            let r6 = (packed >> 12) & LUT_MASK_USIZE;
            let g6 = (packed >> 6) & LUT_MASK_USIZE;
            let b6 = packed & LUT_MASK_USIZE;

            let r = (r6 << 2) | (r6 >> 4);
            let g = (g6 << 2) | (g6 >> 4);
            let b = (b6 << 2) | (b6 >> 4);

            let mut best_idx = 0u8;
            let mut best_dist = u32::MAX;

            for (idx, color) in PALETTE.iter().enumerate() {
                let dist = weighted_distance(r as u8, g as u8, b as u8, *color);

                if dist < best_dist {
                    best_dist = dist;
                    best_idx = idx as u8;
                }
            }

            lut[packed] = best_idx;
        }

        lut.into_boxed_slice()
    })
}

#[inline(always)]
fn nearest_color_6bit(r6: u8, g6: u8, b6: u8) -> u8 {
    let idx = ((r6 as usize) << 12) | ((g6 as usize) << 6) | (b6 as usize);
    color_lut()[idx]
}

pub(super) fn warm_up_color_lut() {
    color_lut();
}

#[inline(always)]
pub(super) fn nearest_color(r: u8, g: u8, b: u8) -> u8 {
    let r6 = r >> 2;
    let g6 = g >> 2;
    let b6 = b >> 2;
    nearest_color_6bit(r6, g6, b6)
}

pub(crate) fn nearest_palette_index(color: [u8; 3]) -> u8 {
    nearest_color(color[0], color[1], color[2])
}

pub(crate) fn exact_palette_index(color: [u8; 3]) -> Option<u8> {
    PALETTE
        .iter()
        .position(|&palette_color| palette_color == color)
        .map(|idx| idx as u8)
}

#[inline(always)]
pub(super) fn lab_components_from_rgb(color: [u8; 3]) -> [f32; 3] {
    let lab = Lab::from_rgb(&color);
    [lab.l, lab.a, lab.b]
}

#[inline(always)]
fn srgb_to_linear(channel: u8) -> f32 {
    let value = channel as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[inline(always)]
fn linear_to_srgb(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let srgb = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round().clamp(0.0, 255.0) as u8
}

#[inline(always)]
fn rgb_to_linear_array(color: [u8; 3]) -> [f32; 3] {
    [
        srgb_to_linear(color[0]),
        srgb_to_linear(color[1]),
        srgb_to_linear(color[2]),
    ]
}

#[inline(always)]
pub(super) fn linear_array_to_lab(linear: [f32; 3]) -> [f32; 3] {
    let srgb = [
        linear_to_srgb(linear[0]),
        linear_to_srgb(linear[1]),
        linear_to_srgb(linear[2]),
    ];
    let lab = Lab::from_rgb(&srgb);
    [lab.l, lab.a, lab.b]
}

#[inline(always)]
fn linear_luma(linear: [f32; 3]) -> f32 {
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

#[inline(always)]
pub(super) fn ciede2000_distance_sq(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    let (l1, a1, b1) = (lhs[0], lhs[1], lhs[2]);
    let (l2, a2, b2) = (rhs[0], rhs[1], rhs[2]);

    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let avg_c = 0.5 * (c1 + c2);
    let avg_c7 = avg_c.powi(7);
    let g = 0.5 * (1.0 - (avg_c7 / (avg_c7 + 6_103_515_625.0)).sqrt());

    let a1_prime = (1.0 + g) * a1;
    let a2_prime = (1.0 + g) * a2;
    let c1_prime = (a1_prime * a1_prime + b1 * b1).sqrt();
    let c2_prime = (a2_prime * a2_prime + b2 * b2).sqrt();

    fn hue_angle_degrees(b: f32, a: f32) -> f32 {
        let mut angle = b.atan2(a).to_degrees();
        if angle < 0.0 {
            angle += 360.0;
        }
        angle
    }

    let h1_prime = if c1_prime < 1e-9 {
        0.0
    } else {
        hue_angle_degrees(b1, a1_prime)
    };
    let h2_prime = if c2_prime < 1e-9 {
        0.0
    } else {
        hue_angle_degrees(b2, a2_prime)
    };

    let delta_l_prime = l2 - l1;
    let delta_c_prime = c2_prime - c1_prime;

    let delta_h_prime = if c1_prime < 1e-9 || c2_prime < 1e-9 {
        0.0
    } else {
        let mut delta = h2_prime - h1_prime;
        if delta > 180.0 {
            delta -= 360.0;
        } else if delta < -180.0 {
            delta += 360.0;
        }
        delta
    };

    let delta_big_h_prime =
        2.0 * (c1_prime * c2_prime).sqrt() * (0.5 * delta_h_prime).to_radians().sin();

    let avg_l_prime = 0.5 * (l1 + l2);
    let avg_c_prime = 0.5 * (c1_prime + c2_prime);

    let avg_h_prime = if c1_prime < 1e-9 || c2_prime < 1e-9 {
        h1_prime + h2_prime
    } else if (h1_prime - h2_prime).abs() > 180.0 {
        if h1_prime + h2_prime < 360.0 {
            0.5 * (h1_prime + h2_prime + 360.0)
        } else {
            0.5 * (h1_prime + h2_prime - 360.0)
        }
    } else {
        0.5 * (h1_prime + h2_prime)
    };

    let t = 1.0 - 0.17 * (avg_h_prime - 30.0).to_radians().cos()
        + 0.24 * (2.0 * avg_h_prime).to_radians().cos()
        + 0.32 * (3.0 * avg_h_prime + 6.0).to_radians().cos()
        - 0.20 * (4.0 * avg_h_prime - 63.0).to_radians().cos();

    let delta_theta = 30.0 * (-(((avg_h_prime - 275.0) / 25.0).powi(2))).exp();
    let avg_c_prime7 = avg_c_prime.powi(7);
    let r_c = 2.0 * (avg_c_prime7 / (avg_c_prime7 + 6_103_515_625.0)).sqrt();
    let s_l =
        1.0 + (0.015 * (avg_l_prime - 50.0).powi(2)) / (20.0 + (avg_l_prime - 50.0).powi(2)).sqrt();
    let s_c = 1.0 + 0.045 * avg_c_prime;
    let s_h = 1.0 + 0.015 * avg_c_prime * t;
    let r_t = -r_c * (2.0 * delta_theta).to_radians().sin();

    let term_l = delta_l_prime / s_l;
    let term_c = delta_c_prime / s_c;
    let term_h = delta_big_h_prime / s_h;

    term_l * term_l + term_c * term_c + term_h * term_h + r_t * term_c * term_h
}

#[inline(always)]
pub(super) fn palette_lab() -> &'static [[f32; 3]] {
    PALETTE_LAB.get_or_init(|| {
        PALETTE
            .iter()
            .map(|&color| lab_components_from_rgb(color))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}

#[inline(always)]
pub(super) fn palette_linear() -> &'static [[f32; 3]] {
    PALETTE_LINEAR.get_or_init(|| {
        PALETTE
            .iter()
            .map(|&color| rgb_to_linear_array(color))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}

#[inline(always)]
pub(super) fn palette_luma() -> &'static [f32] {
    PALETTE_LUMA.get_or_init(|| {
        palette_linear()
            .iter()
            .map(|&color| linear_luma(color))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}
