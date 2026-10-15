#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use kw_utils::{conv, format_bytes, format_time};

fn close32(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1e-3,
        "expected about {expected}, got {actual}"
    );
}

fn close64(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-9,
        "expected about {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// format_time
// ---------------------------------------------------------------------------

mod time_tests {
    use super::*;

    /// Parses `format_time` output back into seconds, asserting on the way
    /// that the shape is valid: single spaces, units in d/h/m/s order, no
    /// zero components (except the lone "0s"), and h < 24, m < 60, s < 60.
    fn parse_time(s: &str) -> u128 {
        if s == "0s" {
            return 0;
        }
        let mut total: u128 = 0;
        let mut last_rank = 0_u8;
        for tok in s.split(' ') {
            assert!(!tok.is_empty(), "empty token (double space?) in {s:?}");
            let (num, unit) = tok.split_at(tok.len() - 1);
            let n: u128 = num
                .parse()
                .unwrap_or_else(|_| panic!("bad number in {s:?}"));
            assert!(n != 0, "zero component in {s:?}");
            let (rank, mult, limit) = match unit {
                "d" => (1_u8, 86_400_u128, u128::MAX),
                "h" => (2, 3_600, 24),
                "m" => (3, 60, 60),
                "s" => (4, 1, 60),
                other => panic!("unknown unit {other:?} in {s:?}"),
            };
            assert!(n < limit, "component out of range in {s:?}");
            assert!(rank > last_rank, "units out of order in {s:?}");
            last_rank = rank;
            total += n * mult;
        }
        total
    }

    #[test]
    fn zero_is_0s() {
        assert_eq!(format_time(0), "0s");
    }

    #[test]
    fn single_units() {
        assert_eq!(format_time(1), "1s");
        assert_eq!(format_time(59), "59s");
        assert_eq!(format_time(60), "1m");
        assert_eq!(format_time(3_600), "1h");
        assert_eq!(format_time(86_400), "1d");
    }

    #[test]
    fn mixed_units() {
        assert_eq!(format_time(61), "1m 1s");
        assert_eq!(format_time(3_599), "59m 59s");
        assert_eq!(format_time(3_661), "1h 1m 1s");
        assert_eq!(format_time(86_399), "23h 59m 59s");
        assert_eq!(format_time(90_061), "1d 1h 1m 1s");
        assert_eq!(format_time(183_845), "2d 3h 4m 5s");
    }

    #[test]
    fn zero_components_are_skipped() {
        assert_eq!(format_time(3_601), "1h 1s");
        assert_eq!(format_time(3_660), "1h 1m");
        assert_eq!(format_time(86_401), "1d 1s");
        assert_eq!(format_time(86_460), "1d 1m");
        assert_eq!(format_time(90_000), "1d 1h");
    }

    #[test]
    fn days_are_unbounded() {
        // No weeks or months: everything above a day stays in days.
        assert_eq!(format_time(34_560_000), "400d");
    }

    #[test]
    fn u64_max_does_not_panic() {
        let expected = format!("{}d 7h 15s", u64::MAX / 86_400);
        assert_eq!(format_time(u64::MAX), expected);
    }

    #[test]
    fn output_parses_back_to_the_same_number_of_seconds() {
        let edge = [86_399, 86_400, 86_401, u64::MAX - 1, u64::MAX];
        for secs in (0..=200_000_u64).chain(edge) {
            let text = format_time(secs);
            assert_eq!(
                parse_time(&text),
                u128::from(secs),
                "round trip failed for {secs} -> {text:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// format_bytes
// ---------------------------------------------------------------------------

mod bytes_tests {
    use super::*;

    const KB: u64 = 1_024;
    const MB: u64 = KB * 1_024;
    const GB: u64 = MB * 1_024;
    const TB: u64 = GB * 1_024;

    #[test]
    fn small_values_are_plain_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(1_000), "1000 B");
        assert_eq!(format_bytes(1_023), "1023 B");
    }

    #[test]
    fn exact_unit_boundaries() {
        assert_eq!(format_bytes(KB), "1.0 KB");
        assert_eq!(format_bytes(MB), "1.0 MB");
        assert_eq!(format_bytes(GB), "1.0 GB");
        assert_eq!(format_bytes(TB), "1.0 TB");
    }

    #[test]
    fn halves_and_whole_numbers() {
        assert_eq!(format_bytes(1_536), "1.5 KB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
        assert_eq!(format_bytes(512 * MB), "512.0 MB");
        assert_eq!(format_bytes(2_684_354_560), "2.5 GB");
        assert_eq!(format_bytes(1_649_267_441_664), "1.5 TB");
        assert_eq!(format_bytes(5 * TB), "5.0 TB");
    }

    #[test]
    fn tenths_are_truncated_not_rounded() {
        // 1024 + 102 is 1.0996 KB: truncates to 1.0.
        assert_eq!(format_bytes(KB + 102), "1.0 KB");
        // 1024 + 103 is 1.1006 KB: the first value that shows 1.1.
        assert_eq!(format_bytes(KB + 103), "1.1 KB");
        assert_eq!(format_bytes(KB + 1), "1.0 KB");
    }

    #[test]
    fn just_below_a_boundary_stays_in_the_smaller_unit() {
        // Pinned: truncation means these show 1023.9 rather than rounding up
        // to "1024.0" or switching unit. Flip these if rounding is added.
        assert_eq!(format_bytes(MB - 1), "1023.9 KB");
        assert_eq!(format_bytes(GB - 1), "1023.9 MB");
        assert_eq!(format_bytes(TB - 1), "1023.9 GB");
    }

    #[test]
    fn units_are_binary_even_though_labels_say_kb_mb() {
        // Pinned: 1000-based numbers do not look round. The labels are KB/MB
        // but the divisor is 1024 (KiB/MiB). Flip if the labels are changed.
        assert_eq!(format_bytes(1_000_000), "976.5 KB");
        assert_eq!(format_bytes(1_000_000_000), "953.6 MB");
    }

    #[test]
    fn terabytes_are_the_largest_unit() {
        // Pinned: no PB. 1024 TB prints as "1024.0 TB".
        assert_eq!(format_bytes(TB * 1_024), "1024.0 TB");
    }

    #[test]
    fn u64_max_does_not_panic() {
        // 2^64 - 1 = 16_777_215 TB plus a remainder of 2^40 - 1, which is
        // 0.99999.. TB, so the tenths digit is 9.
        assert_eq!(format_bytes(u64::MAX), "16777215.9 TB");
    }

    fn expected_unit(bytes: u64) -> (&'static str, u64) {
        if bytes >= TB {
            ("TB", TB)
        } else if bytes >= GB {
            ("GB", GB)
        } else if bytes >= MB {
            ("MB", MB)
        } else if bytes >= KB {
            ("KB", KB)
        } else {
            ("B", 1)
        }
    }

    /// Checks one output against the rules instead of a literal string:
    /// right unit, "<whole>.<one digit>" shape, and the digits are the
    /// floor of the real value.
    fn check_shape(bytes: u64) {
        let text = format_bytes(bytes);
        let parts: Vec<&str> = text.split(' ').collect();
        assert_eq!(parts.len(), 2, "unexpected shape {text:?} for {bytes}");
        let (unit_name, unit) = expected_unit(bytes);
        assert_eq!(parts[1], unit_name, "wrong unit for {bytes}: {text:?}");

        if unit_name == "B" {
            assert_eq!(parts[0], bytes.to_string(), "plain bytes for {bytes}");
            return;
        }

        let (whole, tenths) = parts[0]
            .split_once('.')
            .unwrap_or_else(|| panic!("no decimal point in {text:?}"));
        assert_eq!(tenths.len(), 1, "want exactly one decimal in {text:?}");
        let whole: u128 = whole.parse().unwrap();
        let tenths: u128 = tenths.parse().unwrap();
        let unit = u128::from(unit);
        let bytes = u128::from(bytes);

        assert!(whole * unit <= bytes, "whole too large in {text:?}");
        let rem = bytes - whole * unit;
        assert!(rem < unit, "whole too small in {text:?}");
        assert!(rem * 10 >= tenths * unit, "tenths too large in {text:?}");
        assert!(
            rem * 10 < (tenths + 1) * unit,
            "tenths too small in {text:?}"
        );
    }

    #[test]
    fn output_shape_holds_around_every_power_of_two() {
        for shift in 0..64 {
            let base = 1_u64 << shift;
            check_shape(base.saturating_sub(1));
            check_shape(base);
            check_shape(base.saturating_add(1));
        }
        check_shape(u64::MAX);
    }

    #[test]
    fn output_shape_holds_for_a_dense_low_range() {
        for bytes in 0..=5_000_u64 {
            check_shape(bytes);
        }
    }
}

// ---------------------------------------------------------------------------
// conv: ratios and percentages
// ---------------------------------------------------------------------------

mod ratio_tests {
    use super::*;

    #[test]
    fn percent_f32_basic() {
        close32(conv::u64_ratio_percent_f32(0, 100), 0.0);
        close32(conv::u64_ratio_percent_f32(1, 4), 25.0);
        close32(conv::u64_ratio_percent_f32(50, 100), 50.0);
        close32(conv::u64_ratio_percent_f32(100, 100), 100.0);
        close32(conv::u64_ratio_percent_f32(1, 3), 33.333_332);
    }

    #[test]
    fn percent_f32_zero_total_is_zero() {
        assert_eq!(conv::u64_ratio_percent_f32(0, 0), 0.0);
        assert_eq!(conv::u64_ratio_percent_f32(5, 0), 0.0);
        assert_eq!(conv::u64_ratio_percent_f32(u64::MAX, 0), 0.0);
    }

    #[test]
    fn percent_f32_is_not_clamped() {
        // Pinned: used > total gives more than 100. Flip if clamping is added.
        close32(conv::u64_ratio_percent_f32(200, 100), 200.0);
    }

    #[test]
    fn percent_f32_handles_huge_values() {
        close32(conv::u64_ratio_percent_f32(u64::MAX, u64::MAX), 100.0);
        close32(conv::u64_ratio_percent_f32(u64::MAX / 2, u64::MAX), 50.0);
    }

    #[test]
    fn ratio_f64_basic() {
        assert_eq!(conv::u64_ratio_f64(0, 5), 0.0);
        assert_eq!(conv::u64_ratio_f64(1, 2), 0.5);
        assert_eq!(conv::u64_ratio_f64(3, 2), 1.5);
        assert_eq!(conv::u64_ratio_f64(7, 7), 1.0);
        close64(conv::u64_ratio_f64(1, 3), 0.333_333_333_333);
    }

    #[test]
    fn ratio_f64_zero_denominator_is_zero() {
        assert_eq!(conv::u64_ratio_f64(0, 0), 0.0);
        assert_eq!(conv::u64_ratio_f64(1, 0), 0.0);
        assert_eq!(conv::u64_ratio_f64(u64::MAX, 0), 0.0);
    }

    #[test]
    fn ratio_f64_huge_values() {
        assert_eq!(conv::u64_ratio_f64(u64::MAX, u64::MAX), 1.0);
    }

    #[test]
    fn percent_f64_basic() {
        close64(conv::u64_ratio_percent_f64(0, 10), 0.0);
        close64(conv::u64_ratio_percent_f64(1, 4), 25.0);
        close64(conv::u64_ratio_percent_f64(1, 1), 100.0);
        close64(conv::u64_ratio_percent_f64(3, 2), 150.0);
    }

    #[test]
    fn percent_f64_zero_denominator_is_zero() {
        assert_eq!(conv::u64_ratio_percent_f64(0, 0), 0.0);
        assert_eq!(conv::u64_ratio_percent_f64(42, 0), 0.0);
    }

    /// The assumptions the `kw-types` tests rely on, stated explicitly.
    #[test]
    fn assumptions_used_by_kw_types() {
        assert_eq!(conv::f32_percent_to_u32(25.0), 25);
        close64(conv::u64_ratio_percent_f64(50, 1_000), 5.0);
        let mib: u64 = 1_024 * 1_024;
        assert_eq!(conv::u64_ratio_f64(512 * mib, 1_024 * mib), 0.5);
    }
}

// ---------------------------------------------------------------------------
// conv: float -> integer
// ---------------------------------------------------------------------------

mod float_to_int_tests {
    use super::*;

    #[test]
    fn f32_percent_to_u32_normal_values() {
        assert_eq!(conv::f32_percent_to_u32(0.0), 0);
        assert_eq!(conv::f32_percent_to_u32(25.0), 25);
        assert_eq!(conv::f32_percent_to_u32(100.0), 100);
        assert_eq!(conv::f32_percent_to_u32(4_294_967_040.0), 4_294_967_040);
    }

    #[test]
    fn f32_percent_to_u32_truncates_toward_zero() {
        assert_eq!(conv::f32_percent_to_u32(0.99), 0);
        assert_eq!(conv::f32_percent_to_u32(99.99), 99);
        assert_eq!(conv::f32_percent_to_u32(-0.5), 0);
    }

    #[test]
    fn f32_percent_to_u32_negative_and_nan_are_zero() {
        assert_eq!(conv::f32_percent_to_u32(-1.0), 0);
        assert_eq!(conv::f32_percent_to_u32(-1_000.0), 0);
        assert_eq!(conv::f32_percent_to_u32(f32::NEG_INFINITY), 0);
        assert_eq!(conv::f32_percent_to_u32(f32::NAN), 0);
    }

    #[test]
    fn f32_percent_to_u32_out_of_range_is_zero() {
        // Pinned: the doc comment says out-of-range saturates, but the value
        // becomes 0, not u32::MAX. See the ignored test below. Flip this one
        // if the function is changed.
        assert_eq!(conv::f32_percent_to_u32(4_294_967_296.0), 0);
        assert_eq!(conv::f32_percent_to_u32(1e20), 0);
        assert_eq!(conv::f32_percent_to_u32(f32::INFINITY), 0);
    }

    #[test]
    #[ignore = "doc says out-of-range saturates, but f32_percent_to_u32(1e20) returns 0"]
    fn f32_percent_to_u32_out_of_range_should_saturate() {
        assert_eq!(conv::f32_percent_to_u32(1e20), u32::MAX);
        assert_eq!(conv::f32_percent_to_u32(f32::INFINITY), u32::MAX);
    }

    #[test]
    fn f32_to_u64_saturating_normal_values() {
        assert_eq!(conv::f32_to_u64_saturating(0.0), 0);
        assert_eq!(conv::f32_to_u64_saturating(25.9), 25);
        assert_eq!(
            conv::f32_to_u64_saturating(10_000_000_000.0),
            10_000_000_000
        );
    }

    #[test]
    fn f32_to_u64_saturating_negative_and_nan_are_zero() {
        assert_eq!(conv::f32_to_u64_saturating(-3.0), 0);
        assert_eq!(conv::f32_to_u64_saturating(f32::NAN), 0);
    }

    #[test]
    fn f32_to_u64_saturating_out_of_range_is_zero() {
        // Pinned: the name says "saturating" but values too large for u64
        // become 0, not u64::MAX. Flip if the function is changed.
        assert_eq!(conv::f32_to_u64_saturating(1e30), 0);
        assert_eq!(conv::f32_to_u64_saturating(f32::INFINITY), 0);
    }

    #[test]
    fn f64_to_usize_saturating_normal_values() {
        assert_eq!(conv::f64_to_usize_saturating(0.0), 0);
        assert_eq!(conv::f64_to_usize_saturating(3.9), 3);
        assert_eq!(conv::f64_to_usize_saturating(1_000_000.0), 1_000_000);
    }

    #[test]
    fn f64_to_usize_saturating_negative_and_nan_are_zero() {
        assert_eq!(conv::f64_to_usize_saturating(-1.0), 0);
        assert_eq!(conv::f64_to_usize_saturating(f64::NAN), 0);
    }

    #[test]
    fn f64_to_usize_saturating_out_of_range_is_zero() {
        // Pinned, same as the u64 version above.
        assert_eq!(conv::f64_to_usize_saturating(1e30), 0);
        assert_eq!(conv::f64_to_usize_saturating(f64::INFINITY), 0);
    }
}

// ---------------------------------------------------------------------------
// conv: integer -> float
// ---------------------------------------------------------------------------

mod int_to_float_tests {
    use super::*;

    #[test]
    fn u32_to_f32_exact_in_the_useful_range() {
        assert_eq!(conv::u32_to_f32(0), 0.0);
        assert_eq!(conv::u32_to_f32(100), 100.0);
        // 2^24 is the last point where every integer is exact in f32.
        assert_eq!(conv::u32_to_f32(16_777_216), 16_777_216.0);
    }

    #[test]
    fn u32_to_f32_max_is_close_to_u32_max() {
        assert!(conv::u32_to_f32(u32::MAX) >= 4.29e9);
    }

    #[test]
    fn usize_to_f64_values() {
        assert_eq!(conv::usize_to_f64(0), 0.0);
        assert_eq!(conv::usize_to_f64(1_000), 1_000.0);
        assert!(conv::usize_to_f64(usize::MAX) > 4.0e9);
    }

    #[test]
    fn u64_to_f64_lossy_values() {
        assert_eq!(conv::u64_to_f64_lossy(0), 0.0);
        assert_eq!(conv::u64_to_f64_lossy(1_000), 1_000.0);
        assert_eq!(conv::u64_to_f64_lossy(1_u64 << 53), 2.0_f64.powi(53));
        assert_eq!(conv::u64_to_f64_lossy(u64::MAX), 2.0_f64.powi(64));
    }

    #[test]
    fn u64_to_f64_lossy_loses_precision_above_2_pow_53() {
        // Pinned: this is why the function is called "lossy". 2^53 + 1 rounds
        // to 2^53.
        assert_eq!(conv::u64_to_f64_lossy((1_u64 << 53) + 1), 2.0_f64.powi(53));
    }
}

// ---------------------------------------------------------------------------
// conv: integer width changes
// ---------------------------------------------------------------------------

mod int_width_tests {
    use super::*;

    #[test]
    fn u32_to_usize_values() {
        assert_eq!(conv::u32_to_usize(0), 0);
        assert_eq!(conv::u32_to_usize(42), 42);
        assert_eq!(
            conv::u32_to_usize(u32::MAX),
            usize::try_from(u32::MAX).unwrap()
        );
    }

    #[test]
    fn usize_to_i32_saturating_values() {
        let max = usize::try_from(i32::MAX).unwrap();
        let over = usize::try_from(2_147_483_648_u32).unwrap();
        assert_eq!(conv::usize_to_i32_saturating(0), 0);
        assert_eq!(conv::usize_to_i32_saturating(42), 42);
        assert_eq!(conv::usize_to_i32_saturating(max), i32::MAX);
        assert_eq!(conv::usize_to_i32_saturating(over), i32::MAX);
        assert_eq!(conv::usize_to_i32_saturating(usize::MAX), i32::MAX);
    }

    #[test]
    fn i32_to_usize_saturating_values() {
        assert_eq!(conv::i32_to_usize_saturating(0), 0);
        assert_eq!(conv::i32_to_usize_saturating(7), 7);
        assert_eq!(
            conv::i32_to_usize_saturating(i32::MAX),
            usize::try_from(i32::MAX).unwrap()
        );
        assert_eq!(conv::i32_to_usize_saturating(-1), 0);
        assert_eq!(conv::i32_to_usize_saturating(i32::MIN), 0);
    }

    #[test]
    fn usize_to_u16_saturating_values() {
        assert_eq!(conv::usize_to_u16_saturating(0), 0);
        assert_eq!(conv::usize_to_u16_saturating(80), 80);
        assert_eq!(conv::usize_to_u16_saturating(65_535), u16::MAX);
        assert_eq!(conv::usize_to_u16_saturating(65_536), u16::MAX);
        assert_eq!(conv::usize_to_u16_saturating(usize::MAX), u16::MAX);
    }

    #[test]
    fn usize_to_u32_saturating_values() {
        assert_eq!(conv::usize_to_u32_saturating(0), 0);
        assert_eq!(conv::usize_to_u32_saturating(5), 5);
        assert_eq!(
            conv::usize_to_u32_saturating(usize::try_from(u32::MAX).unwrap()),
            u32::MAX
        );
        assert_eq!(conv::usize_to_u32_saturating(usize::MAX), u32::MAX);
    }

    #[test]
    fn u64_to_u32_saturating_values() {
        assert_eq!(conv::u64_to_u32_saturating(0), 0);
        assert_eq!(conv::u64_to_u32_saturating(999_999_999), 999_999_999);
        assert_eq!(conv::u64_to_u32_saturating(u64::from(u32::MAX)), u32::MAX);
        assert_eq!(
            conv::u64_to_u32_saturating(u64::from(u32::MAX) + 1),
            u32::MAX
        );
        assert_eq!(conv::u64_to_u32_saturating(u64::MAX), u32::MAX);
    }

    #[test]
    fn char_to_usize_returns_the_unicode_scalar_not_the_digit() {
        // Pinned: '1' maps to 49, not 1. The doc comment says this is "used
        // for turning digit keys ('1'..'9') into indices", so any caller
        // that uses the result directly as an index is off by 48 and has to
        // subtract '0' itself. Check the callers in the TUI.
        assert_eq!(conv::char_to_usize_saturating('0'), 48);
        assert_eq!(conv::char_to_usize_saturating('1'), 49);
        assert_eq!(conv::char_to_usize_saturating('9'), 57);
        assert_eq!(conv::char_to_usize_saturating('A'), 65);
        assert_eq!(conv::char_to_usize_saturating('\u{10FFFF}'), 0x10_FFFF);
    }
}
