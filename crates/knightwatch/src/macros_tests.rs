#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::cell::Cell;

#[test]
fn all_none_is_true_when_every_option_is_none() {
    let a: Option<i32> = None;
    let b: Option<String> = None;
    let c: Option<Vec<u8>> = None;
    assert!(crate::all_none!(a, b, c));
}

#[test]
fn all_none_is_false_when_the_first_is_some() {
    let a = Some(1);
    let b: Option<String> = None;
    assert!(!crate::all_none!(a, b));
}

#[test]
fn all_none_is_false_when_the_last_is_some() {
    let a: Option<i32> = None;
    let b = Some("x".to_string());
    assert!(!crate::all_none!(a, b));
}

#[test]
fn all_none_is_false_when_a_middle_one_is_some() {
    let a: Option<i32> = None;
    let b = Some(2_u8);
    let c: Option<i32> = None;
    assert!(!crate::all_none!(a, b, c));
}

#[test]
fn all_none_works_with_a_single_argument() {
    let none: Option<i32> = None;
    let some = Some(1);
    assert!(crate::all_none!(none));
    assert!(!crate::all_none!(some));
}

#[test]
fn all_none_accepts_a_trailing_comma() {
    let a: Option<i32> = None;
    let b: Option<i32> = None;
    assert!(crate::all_none!(a, b,));
    assert!(crate::all_none!(a,));
}

#[test]
fn all_none_works_with_many_arguments() {
    let n: Option<i32> = None;
    assert!(crate::all_none!(n, n, n, n, n, n, n, n));
    assert!(!crate::all_none!(n, n, n, n, n, n, n, Some(1)));
}

#[test]
fn all_none_does_not_move_its_arguments() {
    let a: Option<String> = None;
    let b = Some("kept".to_string());
    let _ = crate::all_none!(a, b);
    assert!(a.is_none());
    assert_eq!(b.as_deref(), Some("kept"));
}

#[test]
fn all_none_accepts_arbitrary_expressions() {
    let empty: Vec<i32> = Vec::new();
    let one = vec![1];
    assert!(crate::all_none!(empty.first(), empty.get(3)));
    assert!(!crate::all_none!(one.first(), one.get(3)));
}

#[test]
fn all_none_negation_applies_to_the_whole_result() {
    // `!all_none!(a, b)` must mean "not (a and b are both none)".
    // If the macro were spliced as text it would become `!a.is_none() && ..`.
    assert!(!crate::all_none!(None::<i32>, Some(1)));
    assert!(!crate::all_none!(Some(1), Some(2)));
}

#[test]
fn all_none_evaluates_each_argument_once_when_all_are_none() {
    let calls = Cell::new(0_u32);
    let tick = || {
        calls.set(calls.get() + 1);
        None::<i32>
    };
    assert!(crate::all_none!(tick(), tick(), tick()));
    assert_eq!(calls.get(), 3);
}

#[test]
fn all_none_short_circuits_after_the_first_some() {
    // Pinned: it expands to `&&`, so later arguments are not evaluated.
    let calls = Cell::new(0_u32);
    let tick = || {
        calls.set(calls.get() + 1);
        None::<i32>
    };
    assert!(!crate::all_none!(Some(1), tick(), tick()));
    assert_eq!(calls.get(), 0);

    assert!(!crate::all_none!(tick(), Some(1), tick()));
    assert_eq!(calls.get(), 1, "only the argument before the Some runs");
}

#[test]
fn all_none_can_be_used_in_a_const() {
    const ALL: bool = crate::all_none!(None::<i32>, None::<u8>);
    const NOT_ALL: bool = crate::all_none!(None::<i32>, Some(1_u8));
    assert!(std::hint::black_box(ALL));
    assert!(!std::hint::black_box(NOT_ALL));
}
