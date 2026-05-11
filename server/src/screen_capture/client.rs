use super::models::{ScreenCaptureQuery, Screenshot};

fn get_screen_capture_query_sender()
-> Option<&'static tokio::sync::mpsc::Sender<ScreenCaptureQuery>> {
    super::capture::SCREEN_CAPTURE_QUERY_SENDER.get()
}

pub async fn get_screenshots() -> Vec<Screenshot> {
    let Some(tx_ref) = get_screen_capture_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = tx_ref
        .send(ScreenCaptureQuery::GetScreenshots { response: tx })
        .await;
    rx.await.unwrap_or_default()
}
