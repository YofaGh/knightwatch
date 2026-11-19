use num_traits::ToPrimitive;

/// `used / total * 100`, as `f32`. `0.0` if `total == 0` or a value doesn't
/// fit in `f32` (never happens for our byte counts in practice).
#[must_use]
pub fn u64_ratio_percent_f32(used: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    let used = used.to_f32().unwrap_or(0.0);
    let total = total.to_f32().unwrap_or(0.0);
    if total == 0.0 {
        return 0.0;
    }
    (used / total) * 100.0
}

/// `numerator / denominator * 100`, as `f64`.
#[must_use]
pub fn u64_ratio_percent_f64(numerator: u64, denominator: u64) -> f64 {
    u64_ratio_f64(numerator, denominator) * 100.0
}

/// Plain 0.0–1.0 ratio as `f64`. `0.0` if `denominator == 0`.
#[must_use]
pub fn u64_ratio_f64(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    let n = numerator.to_f64().unwrap_or(0.0);
    let d = denominator.to_f64().unwrap_or(0.0);
    if d == 0.0 {
        return 0.0;
    }
    n / d
}

/// `u32 -> f32`. Always exact for our value ranges (percentages, temps, watts).
#[must_use]
pub fn u32_to_f32(v: u32) -> f32 {
    v.to_f32().unwrap_or(0.0)
}

/// Percentage-ish `f32` -> `u32`, truncated. Negative and NaN give 0, too
/// large gives `u32::MAX`.
#[must_use]
pub fn f32_percent_to_u32(v: f32) -> u32 {
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    v.to_u32().unwrap_or(u32::MAX)
}

/// `f32 -> u64`, truncated. Negative and NaN give 0, too large gives `u64::MAX`.
#[must_use]
pub fn f32_to_u64_saturating(v: f32) -> u64 {
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    v.to_u64().unwrap_or(u64::MAX)
}

/// `usize -> f64`. Exact for any width we'd realistically render in a terminal.
#[must_use]
pub fn usize_to_f64(v: usize) -> f64 {
    v.to_f64().unwrap_or(0.0)
}

/// `u32 -> usize`. Always exact on 32-bit+ targets (usize is at least as
/// wide as u32 on every platform we build for).
#[must_use]
pub fn u32_to_usize(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// `f64 -> usize`, truncated. Negative and NaN give 0, too large gives
/// `usize::MAX`.
#[must_use]
pub fn f64_to_usize_saturating(v: f64) -> usize {
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    v.to_usize().unwrap_or(usize::MAX)
}

#[must_use]
pub fn u64_to_f64_lossy(v: u64) -> f64 {
    v.to_f64().unwrap_or(0.0)
}

/// `usize -> i32`, saturating to `i32::MAX` instead of a silent/UB-ish `as`
/// cast. Never actually saturates for realistic list lengths.
#[must_use]
pub fn usize_to_i32_saturating(v: usize) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// `i32 -> usize`, saturating to `0` on negative values instead of a silent
/// `as` cast.
#[must_use]
pub fn i32_to_usize_saturating(v: i32) -> usize {
    usize::try_from(v).unwrap_or(0)
}

/// `usize -> u16`, saturating to `u16::MAX` instead of a silent/UB-ish `as`
/// cast. Never actually saturates for realistic terminal row/col counts.
#[must_use]
pub fn usize_to_u16_saturating(v: usize) -> u16 {
    u16::try_from(v).unwrap_or(u16::MAX)
}

/// The value of an ASCII digit key ('0'..'9'). Anything else gives
/// `usize::MAX`, which is never a valid index.
#[must_use]
pub fn char_to_usize_saturating(c: char) -> usize {
    c.to_digit(10).map_or(usize::MAX, u32_to_usize)
}

/// `usize -> u32`, saturating to `u32::MAX` instead of a silent/UB-ish `as`
/// cast. Never actually saturates for realistic unit counts.
#[must_use]
pub fn usize_to_u32_saturating(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// `u64 -> u32`, saturating to `u32::MAX` instead of a silent/UB-ish `as`
/// cast. Never actually saturates here since the value is always
/// `< 1_000_000_000` (nanoseconds from a sub-second `usec` remainder).
#[must_use]
pub fn u64_to_u32_saturating(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}
