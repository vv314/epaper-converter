pub(super) const BAYER_8X8: [[u8; 8]; 8] = [
    [0, 48, 12, 60, 3, 51, 15, 63],
    [32, 16, 44, 28, 35, 19, 47, 31],
    [8, 56, 4, 52, 11, 59, 7, 55],
    [40, 24, 36, 20, 43, 27, 39, 23],
    [2, 50, 14, 62, 1, 49, 13, 61],
    [34, 18, 46, 30, 33, 17, 45, 29],
    [10, 58, 6, 54, 9, 57, 5, 53],
    [42, 26, 38, 22, 41, 25, 37, 21],
];

#[inline(always)]
pub(super) fn ordered_threshold_8x8(x: usize, y: usize) -> usize {
    BAYER_8X8[y & 7][x & 7] as usize
}

#[inline(always)]
pub(super) fn ordered_bias(rank: u16, levels: i32, strength: i32) -> i32 {
    ((((rank as i32) << 1) - (levels - 1)) * strength) / levels
}

#[inline(always)]
pub(super) fn apply_bias(channel: u8, bias: i32) -> u8 {
    (channel as i32 + bias).clamp(0, 255) as u8
}
