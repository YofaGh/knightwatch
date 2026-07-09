/// Fetches the latest screen image from the shared API and decodes it.
/// Runs on the poller task spawned in `main`, never on the UI thread —
/// `reqwest`'s async I/O and `image::load_from_memory`'s (fast, in-memory)
/// decode both happen off the render path.
pub async fn fetch_screen_image(
    api: &kw_clients::ApiClient,
) -> Result<image::DynamicImage, Box<dyn std::error::Error>> {
    let resp = api.screenshot().await?;
    let screen = resp.screens.get(0).ok_or("no screens in response")?;

    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        screen.data.clone(),
    )?;
    let image = image::load_from_memory(&bytes)?;
    Ok(image)
}
