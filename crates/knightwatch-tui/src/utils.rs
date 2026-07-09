pub fn base64_to_image(
    base64_data: &str,
) -> Result<image::DynamicImage, Box<dyn std::error::Error>> {
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_data)?;
    let image = image::load_from_memory(&bytes)?;
    Ok(image)
}
