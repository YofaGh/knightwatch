#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::{
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    sync::{Mutex, MutexGuard},
};

use super::*;

/// `bind_socket` changes the process umask for a moment, which would hit other
/// tests creating files at the same time.
fn fs_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn me() -> u32 {
    // SAFETY: getuid() has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

#[tokio::test]
async fn the_socket_is_created_for_its_owner_only() {
    let _lock = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("helper.sock");

    let _listener = bind_socket(&path, me()).unwrap();

    let meta = fs::symlink_metadata(&path).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    assert_eq!(meta.uid(), me());
}

#[tokio::test]
async fn a_stale_socket_file_is_replaced() {
    let _lock = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("helper.sock");
    fs::write(&path, "").unwrap();

    bind_socket(&path, me()).unwrap();
}

#[tokio::test]
async fn a_symlinked_socket_dir_is_refused() {
    let _lock = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    let link = dir.path().join("link");
    symlink(&real, &link).unwrap();

    assert!(bind_socket(&link.join("helper.sock"), me()).is_err());
    assert!(!real.join("helper.sock").exists());
}

#[tokio::test]
async fn a_socket_dir_others_can_write_is_refused() {
    let _lock = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
    let path = dir.path().join("helper.sock");

    assert!(bind_socket(&path, me()).is_err());
    assert!(!path.exists());
}

#[tokio::test]
async fn a_socket_dir_owned_by_another_user_is_refused() {
    if me() == 0 {
        return; // root owns everything it creates, so there is no other owner to test
    }
    let _lock = fs_lock();
    let dir = tempfile::tempdir().unwrap();

    assert!(bind_socket(&dir.path().join("helper.sock"), me() + 1).is_err());
}

#[tokio::test]
async fn a_missing_socket_dir_is_created() {
    let _lock = fs_lock();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("knightwatch").join("helper.sock");

    bind_socket(&path, me()).unwrap();
}
