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

/// Percentage-ish `f32` -> `u32`, saturating on negative/NaN/out-of-range
/// instead of the undefined-ish behavior of `as`.
#[must_use]
pub fn f32_percent_to_u32(v: f32) -> u32 {
    v.to_u32().unwrap_or(0)
}

/// `f32 -> u64`, saturating on negative/NaN instead of the UB-ish `as` cast.
#[must_use]
pub fn f32_to_u64_saturating(v: f32) -> u64 {
    v.to_u64().unwrap_or(0)
}

/// `usize -> f64`. Exact for any width we'd realistically render in a terminal.
#[must_use]
pub fn usize_to_f64(v: usize) -> f64 {
    v.to_f64().unwrap_or(0.0)
}

/// `f64 -> usize`, saturating on negative/NaN instead of the UB-ish `as` cast.
#[must_use]
pub fn f64_to_usize_saturating(v: f64) -> usize {
    v.to_usize().unwrap_or(0)
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

/// `char -> usize` via its Unicode scalar value, saturating instead of using
/// a silent `as` cast. Used for turning digit keys ('1'..'9') into indices.
#[must_use]
pub fn char_to_usize_saturating(c: char) -> usize {
    usize::try_from(u32::from(c)).unwrap_or(usize::MAX)
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
