//! Tests for `crates/knightwatch/src/systemd/utils.rs` (`block_on_local`).
//!
//! Save as `crates/knightwatch/src/systemd/utils_tests.rs` and declare it in
//! `systemd/mod.rs` with: `#[cfg(test)] mod utils_tests;`
//!
//! `block_on_local` uses `block_in_place`, which only works on a worker thread of
//! the multi-thread runtime. A plain `#[tokio::test]` body is not a worker thread
//! (calling it there panics with "Cannot start a runtime from within a runtime"),
//! so every call below is made from inside a spawned task, as the real code does.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

use super::utils::block_on_local;

const LONG: Duration = Duration::from_secs(10);

/// Runs `f` inside a spawned task (a runtime worker thread) and returns its result.
async fn on_worker<T, F>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    timeout(LONG, tokio::spawn(async move { f() }))
        .await
        .expect("timed out")
        .expect("task failed")
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_the_value_of_a_ready_future() {
    assert_eq!(on_worker(|| block_on_local(async { 42 })).await, 42);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_owned_and_composite_values() {
    let got = on_worker(|| block_on_local(async { (String::from("unit"), vec![1, 2, 3]) })).await;
    assert_eq!(got, ("unit".to_string(), vec![1, 2, 3]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn passes_an_ok_result_through() {
    let got = on_worker(|| block_on_local(async { Ok::<u8, String>(7) })).await;
    assert_eq!(got, Ok(7));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn passes_an_err_result_through() {
    let got = on_worker(|| block_on_local(async { Err::<u8, String>("nope".into()) })).await;
    assert_eq!(got, Err("nope".to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn can_be_called_repeatedly_from_the_same_task() {
    let got = on_worker(|| {
        let mut out = Vec::new();
        for i in 0..5 {
            out.push(block_on_local(async move { i * 10 }));
        }
        out
    })
    .await;
    assert_eq!(got, vec![0, 10, 20, 30, 40]);
}

// ---------------------------------------------------------------------------
// What the future may do
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waits_for_a_timer_inside_the_future() {
    let started = std::time::Instant::now();
    on_worker(|| block_on_local(async { sleep(Duration::from_millis(80)).await })).await;
    assert!(started.elapsed() >= Duration::from_millis(70));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waits_for_a_message_sent_by_another_task() {
    let got = on_worker(|| {
        let (tx, rx) = oneshot::channel::<u32>();
        tokio::spawn(async move {
            sleep(Duration::from_millis(20)).await;
            let _ = tx.send(7);
        });
        block_on_local(async { rx.await.unwrap() })
    })
    .await;
    assert_eq!(got, 7);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepts_a_future_that_is_not_send() {
    // `Rc` is not `Send`. `block_on_local` has no `Send` bound on the future.
    let got = on_worker(|| {
        let rc = Rc::new(5);
        block_on_local(async move { *rc + 1 })
    })
    .await;
    assert_eq!(got, 6);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepts_a_future_that_borrows_local_data() {
    let got = on_worker(|| {
        let values = vec![1, 2, 3, 4];
        block_on_local(async { values.iter().sum::<i32>() })
    })
    .await;
    assert_eq!(got, 10);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_future_is_dropped_after_it_finishes() {
    struct Guard(Rc<Cell<bool>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = on_worker(|| {
        let flag = Rc::new(Cell::new(false));
        let guard = Guard(flag.clone());
        block_on_local(async move {
            let _keep = guard;
        });
        flag.get()
    })
    .await;
    assert!(dropped);
}

// ---------------------------------------------------------------------------
// Does not block the rest of the runtime
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn other_tasks_keep_running_while_one_task_blocks() {
    let flag = Arc::new(AtomicBool::new(false));
    let setter = flag.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(10)).await;
        setter.store(true, Ordering::SeqCst);
    });
    on_worker(|| block_on_local(async { sleep(Duration::from_millis(300)).await })).await;
    assert!(flag.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn many_tasks_can_block_at_the_same_time() {
    // More blocked tasks than worker threads: `block_in_place` hands the worker off.
    let finished = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let finished = finished.clone();
            tokio::spawn(async move {
                let v = block_on_local(async move {
                    sleep(Duration::from_millis(50)).await;
                    i * 2
                });
                finished.fetch_add(1, Ordering::SeqCst);
                v
            })
        })
        .collect();
    let mut results = Vec::new();
    for h in handles {
        results.push(timeout(LONG, h).await.expect("timed out").expect("task failed"));
    }
    assert_eq!(results, vec![0, 2, 4, 6, 8, 10, 12, 14]);
    assert_eq!(finished.load(Ordering::SeqCst), 8);
}

// ---------------------------------------------------------------------------
// Failure modes (pinned)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_panic_inside_the_future_reaches_the_calling_task() {
    let joined = timeout(
        LONG,
        tokio::spawn(async { block_on_local::<_, ()>(async { panic!("inner") }) }),
    )
    .await
    .expect("timed out");
    assert!(joined.unwrap_err().is_panic());
}

#[tokio::test]
#[should_panic]
async fn panics_on_a_current_thread_runtime() {
    // Pinned: it needs the multi-thread runtime. The default `#[tokio::main]` is
    // multi-thread, so this only matters for tests and custom runtimes.
    let _ = block_on_local(async { 1 });
}

#[test]
#[should_panic]
fn panics_outside_any_runtime() {
    let _ = block_on_local(async { 1 });
}