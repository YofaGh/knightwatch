use tokio::sync::mpsc::Sender;

use crate::events::{AppEvent, CommandOutcome};

/// Fires one command in the background and reports the outcome back
/// through the same event channel everything else flows through — no
/// separate channel or actor loop needed, since these commands don't
/// need to be serialized against each other.
///
/// `request` is a bound future (`async move { api.foo().await }`).
/// `into_outcome` converts its Ok value into a `CommandOutcome`; pass
/// `|_| CommandOutcome::Ack` for endpoints that just return `()`.
pub fn spawn_command<T, Fut, M>(
    tx: Sender<AppEvent>,
    tab: &'static str,
    label: &'static str,
    request: Fut,
    into_outcome: M,
) where
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error>>> + Send + 'static,
    M: FnOnce(T) -> CommandOutcome + Send + 'static,
{
    tokio::spawn(async move {
        let result = request.await.map(into_outcome).map_err(|e| e.to_string());
        let _ = tx
            .send(AppEvent::CommandResult { tab, label, result })
            .await;
    });
}
