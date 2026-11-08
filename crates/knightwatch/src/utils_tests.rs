#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use chrono::{TimeZone, Timelike, Utc};
use std::future::Future;
use std::net::IpAddr;
use std::pin::{Pin, pin};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;

use super::{
    get_listener, get_local_ip, get_version, now_rfc3339, print_local_ips, recv_or_pending,
};

const SHORT: Duration = Duration::from_millis(100);
const LONG: Duration = Duration::from_secs(5);

fn poll_once<F: Future>(fut: Pin<&mut F>) -> Poll<F::Output> {
    let mut cx = Context::from_waker(Waker::noop());
    fut.poll(&mut cx)
}

/// True if `recv_or_pending` does not produce a value within `SHORT`.
async fn stays_pending<T: Clone>(rx: &mut Option<broadcast::Receiver<T>>) -> bool {
    timeout(SHORT, recv_or_pending(rx, "test")).await.is_err()
}

/// The next value from `recv_or_pending`, failing the test if it takes too long.
async fn next<T: Clone>(rx: &mut Option<broadcast::Receiver<T>>) -> T {
    timeout(LONG, recv_or_pending(rx, "test"))
        .await
        .expect("timed out waiting for a value")
}

// ---------------------------------------------------------------------------
// now_rfc3339
// ---------------------------------------------------------------------------

#[test]
fn now_rfc3339_parses_as_rfc3339() {
    let s = now_rfc3339();
    assert!(
        chrono::DateTime::parse_from_rfc3339(&s).is_ok(),
        "not RFC 3339: {s}"
    );
}

#[test]
fn now_rfc3339_is_utc_written_as_plus_zero_offset() {
    let s = now_rfc3339();
    assert!(s.ends_with("+00:00"), "expected a +00:00 suffix: {s}");
    assert!(!s.ends_with('Z'), "expected +00:00, not Z: {s}");
    let parsed = chrono::DateTime::parse_from_rfc3339(&s).unwrap();
    assert_eq!(parsed.offset().local_minus_utc(), 0);
}

#[test]
fn now_rfc3339_has_the_expected_fixed_prefix_layout() {
    // YYYY-MM-DDTHH:MM:SS
    let s = now_rfc3339();
    let b = s.as_bytes();
    assert!(b.len() >= 25, "too short: {s}");
    assert_eq!(
        (b[4], b[7], b[10], b[13], b[16]),
        (b'-', b'-', b'T', b':', b':')
    );
    assert!(b[..4].iter().all(u8::is_ascii_digit));
}

#[test]
fn now_rfc3339_lies_between_two_clock_reads() {
    let before = Utc::now();
    let s = now_rfc3339();
    let after = Utc::now();
    let parsed = chrono::DateTime::parse_from_rfc3339(&s)
        .unwrap()
        .with_timezone(&Utc);
    assert!(before <= parsed, "{parsed} is before {before}");
    assert!(parsed <= after, "{parsed} is after {after}");
}

#[test]
fn now_rfc3339_first_ten_chars_are_the_utc_date() {
    // The history log files are named after these ten characters.
    let before = Utc::now().date_naive().to_string();
    let s = now_rfc3339();
    let after = Utc::now().date_naive().to_string();
    let date = &s[..10];
    // Either date is fine if midnight passed during the test.
    assert!(
        date == before || date == after,
        "{date} vs {before}/{after}"
    );
}

#[test]
fn now_rfc3339_fraction_has_zero_three_six_or_nine_digits() {
    // Pinned: the width is not fixed (chrono's "auto" format). See the sorting
    // test below for why that is still safe for string comparison.
    for _ in 0..50 {
        let s = now_rfc3339();
        let main_part = s.strip_suffix("+00:00").unwrap();
        match main_part.split_once('.') {
            None => {}
            Some((_, frac)) => {
                assert!(frac.bytes().all(|c| c.is_ascii_digit()), "{s}");
                assert!(matches!(frac.len(), 3 | 6 | 9), "odd fraction width: {s}");
            }
        }
    }
}

#[test]
fn utc_rfc3339_text_order_matches_time_order_across_fraction_widths() {
    // This checks the chrono format `now_rfc3339` relies on. It shows that
    // UTC timestamps with 0, 3, 6 or 9 fraction digits still sort correctly as
    // plain text (which `HistoryQuery::filter` and the history file selection do).
    let at = |sec: u32, nanos: u32| {
        Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, sec)
            .unwrap()
            .with_nanosecond(nanos)
            .unwrap()
            .to_rfc3339()
    };
    let stamps = vec![
        at(0, 0),
        at(0, 1_000),
        at(0, 1_000_000),
        at(0, 500_000_000),
        at(0, 500_000_001),
        at(1, 0),
    ];
    assert_eq!(stamps[0], "2024-05-01T12:00:00+00:00");
    assert_eq!(stamps[1], "2024-05-01T12:00:00.000001+00:00");
    assert_eq!(stamps[2], "2024-05-01T12:00:00.001+00:00");
    assert_eq!(stamps[3], "2024-05-01T12:00:00.500+00:00");
    assert_eq!(stamps[4], "2024-05-01T12:00:00.500000001+00:00");
    assert_eq!(stamps[5], "2024-05-01T12:00:01+00:00");
    let mut sorted = stamps.clone();
    sorted.sort();
    assert_eq!(sorted, stamps);
}

// ---------------------------------------------------------------------------
// get_version
// ---------------------------------------------------------------------------

#[test]
fn get_version_is_the_cargo_package_version() {
    assert_eq!(get_version(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn get_version_works_in_a_const_context() {
    const V: &str = get_version();
    assert_eq!(V, env!("CARGO_PKG_VERSION"));
}

#[test]
fn get_version_looks_like_major_minor_patch() {
    let v = get_version();
    assert!(!v.is_empty());
    assert_eq!(v, v.trim());
    let core = v.split(['-', '+']).next().unwrap();
    let parts: Vec<&str> = core.split('.').collect();
    assert_eq!(parts.len(), 3, "not x.y.z: {v}");
    for p in parts {
        assert!(p.parse::<u64>().is_ok(), "{p:?} is not a number in {v}");
    }
}

// ---------------------------------------------------------------------------
// get_listener
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_listener_binds_loopback_on_a_free_port() {
    let listener = get_listener("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    assert!(addr.ip().is_loopback());
    assert_ne!(addr.port(), 0, "port 0 should be replaced by a real port");
}

#[tokio::test]
async fn get_listener_accepts_connections() {
    let listener = get_listener("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (accepted, connected) = timeout(LONG, async {
        tokio::join!(listener.accept(), tokio::net::TcpStream::connect(addr))
    })
    .await
    .expect("timed out");
    let (_server_side, peer) = accepted.unwrap();
    let client_side = connected.unwrap();
    assert_eq!(peer, client_side.local_addr().unwrap());
}

#[tokio::test]
async fn get_listener_is_nonblocking_so_accept_just_waits() {
    // A blocking socket would hang the whole test runtime here instead of
    // letting the timeout fire.
    let listener = get_listener("127.0.0.1:0").unwrap();
    assert!(timeout(SHORT, listener.accept()).await.is_err());
}

#[tokio::test]
async fn get_listener_fails_when_the_address_is_in_use() {
    let first = get_listener("127.0.0.1:0").unwrap();
    let taken = first.local_addr().unwrap().to_string();
    let err = get_listener(&taken).err().expect("second bind should fail");
    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn get_listener_fails_for_malformed_addresses() {
    for bad in [
        "",
        "not-an-address",
        "127.0.0.1",
        "127.0.0.1:99999",
        "127.0.0.1:port",
    ] {
        let err = get_listener(bad)
            .err()
            .unwrap_or_else(|| panic!("{bad:?} should fail"));
        assert!(!err.to_string().is_empty(), "empty message for {bad:?}");
    }
}

#[tokio::test]
async fn get_listener_can_bind_after_the_previous_listener_is_dropped() {
    let first = get_listener("127.0.0.1:0").unwrap();
    let addr = first.local_addr().unwrap().to_string();
    drop(first);
    assert!(get_listener(&addr).is_ok());
}

// ---------------------------------------------------------------------------
// get_local_ip / print_local_ips
// ---------------------------------------------------------------------------

#[test]
fn get_local_ip_is_none_or_a_plain_ipv4_address() {
    // Depends on the machine having a route out, so `None` is allowed.
    if let Some(ip) = get_local_ip() {
        let parsed: IpAddr = ip.parse().expect("should be a bare IP with no port");
        assert!(parsed.is_ipv4());
        assert!(!parsed.is_unspecified());
    }
}

#[test]
fn get_local_ip_is_stable_between_calls() {
    assert_eq!(get_local_ip(), get_local_ip());
}

#[test]
fn print_local_ips_does_not_panic() {
    // Output goes to stdout, which the test harness captures.
    for port in [0, 80, 8080, u16::MAX] {
        print_local_ips(port);
    }
}

// ---------------------------------------------------------------------------
// start_dev_server (debug builds only)
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
#[test]
fn start_dev_server_does_nothing_when_port_5173_is_already_open() {
    // Hold the port if we can. If something else already has it, that works too.
    let _guard = std::net::TcpListener::bind("localhost:5173");
    if std::net::TcpStream::connect("localhost:5173").is_err() {
        // Could not make the port reachable. Skip, so that `npm` is never started.
        return;
    }
    assert!(super::start_dev_server().is_none());
}

// ---------------------------------------------------------------------------
// recv_or_pending
// ---------------------------------------------------------------------------

#[test]
fn recv_or_pending_with_no_receiver_never_completes() {
    let mut rx: Option<broadcast::Receiver<i32>> = None;
    let fut = recv_or_pending(&mut rx, "none");
    let mut fut = pin!(fut);
    assert!(poll_once(fut.as_mut()).is_pending());
    assert!(poll_once(fut.as_mut()).is_pending());
}

#[tokio::test]
async fn recv_or_pending_with_no_receiver_stays_pending_for_a_while() {
    let mut rx: Option<broadcast::Receiver<i32>> = None;
    assert!(stays_pending(&mut rx).await);
}

#[tokio::test]
async fn recv_or_pending_returns_a_buffered_value() {
    let (tx, rx) = broadcast::channel::<i32>(4);
    let mut rx = Some(rx);
    tx.send(42).unwrap();
    assert_eq!(next(&mut rx).await, 42);
}

#[tokio::test]
async fn recv_or_pending_returns_values_in_order() {
    let (tx, rx) = broadcast::channel::<i32>(8);
    let mut rx = Some(rx);
    for v in [1, 2, 3] {
        tx.send(v).unwrap();
    }
    assert_eq!(next(&mut rx).await, 1);
    assert_eq!(next(&mut rx).await, 2);
    assert_eq!(next(&mut rx).await, 3);
}

#[tokio::test]
async fn recv_or_pending_waits_while_the_channel_is_empty() {
    let (tx, rx) = broadcast::channel::<i32>(4);
    let mut rx = Some(rx);
    assert!(stays_pending(&mut rx).await);
    tx.send(9).unwrap();
    assert_eq!(next(&mut rx).await, 9);
}

#[tokio::test]
async fn recv_or_pending_wakes_when_a_value_is_sent_later() {
    let (tx, rx) = broadcast::channel::<i32>(4);
    let handle = tokio::spawn(async move {
        let mut rx = Some(rx);
        recv_or_pending(&mut rx, "spawned").await
    });
    tokio::task::yield_now().await;
    tx.send(7).unwrap();
    let got = timeout(LONG, handle)
        .await
        .expect("timed out")
        .expect("task panicked");
    assert_eq!(got, 7);
}

#[tokio::test]
async fn recv_or_pending_skips_lagged_messages_and_returns_the_oldest_kept() {
    // Capacity 2, four sends: 1 and 2 are overwritten, 3 and 4 remain.
    // The `Lagged` error is swallowed (pinned: nothing is logged or counted).
    let (tx, rx) = broadcast::channel::<i32>(2);
    let mut rx = Some(rx);
    for v in 1..=4 {
        tx.send(v).unwrap();
    }
    assert_eq!(next(&mut rx).await, 3);
    assert_eq!(next(&mut rx).await, 4);
    assert!(
        stays_pending(&mut rx).await,
        "sender is alive, so it should wait"
    );
    drop(tx);
}

#[tokio::test]
async fn recv_or_pending_keeps_working_after_a_lag() {
    let (tx, rx) = broadcast::channel::<i32>(1);
    let mut rx = Some(rx);
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    assert_eq!(next(&mut rx).await, 2);
    tx.send(3).unwrap();
    assert_eq!(next(&mut rx).await, 3);
}

#[tokio::test]
async fn recv_or_pending_pends_forever_once_the_channel_is_closed() {
    let (tx, rx) = broadcast::channel::<i32>(4);
    let mut rx = Some(rx);
    drop(tx);
    assert!(stays_pending(&mut rx).await);
    // And again: it never turns into an error or a value.
    // (Pinned: each call logs "channel closed" again, see finding 36.)
    assert!(stays_pending(&mut rx).await);
}

#[tokio::test]
async fn recv_or_pending_delivers_buffered_values_before_pending_on_close() {
    let (tx, rx) = broadcast::channel::<i32>(8);
    let mut rx = Some(rx);
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    drop(tx);
    assert_eq!(next(&mut rx).await, 1);
    assert_eq!(next(&mut rx).await, 2);
    assert!(stays_pending(&mut rx).await);
}

#[tokio::test]
async fn recv_or_pending_after_lag_and_close_still_returns_the_last_value() {
    let (tx, rx) = broadcast::channel::<i32>(1);
    let mut rx = Some(rx);
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    drop(tx);
    assert_eq!(next(&mut rx).await, 2);
    assert!(stays_pending(&mut rx).await);
}

#[tokio::test]
async fn recv_or_pending_works_with_non_copy_values() {
    let (tx, rx) = broadcast::channel::<String>(4);
    let mut rx = Some(rx);
    tx.send("hello".to_string()).unwrap();
    assert_eq!(next(&mut rx).await, "hello");
}

#[tokio::test]
async fn recv_or_pending_each_subscriber_gets_every_value() {
    let (tx, first) = broadcast::channel::<i32>(4);
    let mut a = Some(first);
    let mut b = Some(tx.subscribe());
    tx.send(5).unwrap();
    assert_eq!(next(&mut a).await, 5);
    assert_eq!(next(&mut b).await, 5);
}

#[tokio::test]
async fn recv_or_pending_does_not_consume_the_receiver_option() {
    let (tx, rx) = broadcast::channel::<i32>(4);
    let mut rx = Some(rx);
    tx.send(1).unwrap();
    let _ = next(&mut rx).await;
    assert!(
        rx.is_some(),
        "the receiver must stay in place for the next call"
    );
}
