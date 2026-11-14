#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use super::WebhookTarget;

fn parse(raw: &str) -> WebhookTarget {
    WebhookTarget::parse(raw).expect("the test url should parse")
}

#[test]
fn a_plain_url_is_kept_and_skips_ticks() {
    let t = parse("https://example.com/hook");
    assert_eq!(t.url, "https://example.com/hook");
    assert!(!t.include_ticks);
}

#[test]
fn a_bare_host_gains_a_trailing_slash() {
    // Pinned: `Url` normalises the empty path.
    assert_eq!(parse("http://example.com").url, "http://example.com/");
}

#[test]
fn webhook_events_tick_turns_ticks_on_and_is_stripped() {
    let t = parse("http://h/x?webhook_events=tick");
    assert!(t.include_ticks);
    assert_eq!(t.url, "http://h/x");
}

#[test]
fn tick_is_found_in_a_comma_list() {
    assert!(parse("http://h/?webhook_events=state,tick").include_ticks);
}

#[test]
fn only_an_exact_tick_entry_counts() {
    assert!(!parse("http://h/?webhook_events=ticks").include_ticks);
    assert!(!parse("http://h/?webhook_events=state").include_ticks);
    assert!(!parse("http://h/?webhook_events=").include_ticks);
    // A space is not trimmed.
    assert!(!parse("http://h/?webhook_events=state,%20tick").include_ticks);
}

#[test]
fn the_last_webhook_events_pair_wins() {
    // Pinned: every pair overwrites the flag.
    assert!(!parse("http://h/?webhook_events=tick&webhook_events=state").include_ticks);
    assert!(parse("http://h/?webhook_events=state&webhook_events=tick").include_ticks);
}

#[test]
fn other_query_pairs_are_kept_in_order() {
    let t = parse("http://h/x?a=1&webhook_events=tick&b=2");
    assert_eq!(t.url, "http://h/x?a=1&b=2");
}

#[test]
fn the_fragment_is_kept() {
    assert_eq!(
        parse("http://h/x?webhook_events=tick#frag").url,
        "http://h/x#frag"
    );
}

#[test]
fn any_query_is_re_encoded_as_form_data() {
    // Pinned: the query is rebuilt even without `webhook_events`, so the URL
    // posted to is not always the one configured.
    assert_eq!(parse("http://h/?q=a%20b").url, "http://h/?q=a+b");
    assert_eq!(parse("http://h/?flag").url, "http://h/?flag=");
}

#[test]
fn an_invalid_url_is_an_error_naming_it() {
    let err = WebhookTarget::parse("not a url").unwrap_err();
    assert!(err.to_string().contains("not a url"), "{err}");
}

#[test]
fn any_scheme_is_accepted() {
    // Pinned: no http(s) check, so a bad scheme only fails at send time,
    // three times per event.
    assert_eq!(parse("ftp://h/x").url, "ftp://h/x");
}
