/// Run an async block synchronously from within a `block_in_place` context.
/// Used by the sync proxy helper methods that need to call async zbus APIs
/// without spawning an extra task.
pub fn block_on_local<F, T>(f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
}
