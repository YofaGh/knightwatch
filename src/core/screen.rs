use xcap::Monitor;

use crate::prelude::*;

pub fn screenshot_all_screens() -> Result<Vec<Vec<u8>>> {
    get_all_screens()?
        .into_iter()
        .map(|screen| take_screenshot(&screen))
        .collect()
}

pub fn get_all_screens() -> Result<Vec<Monitor>> {
    Monitor::all().map_err(|e| Error::Screen(format!("Failed to get monitors: {e}")))
}

#[allow(dead_code)]
pub fn get_first_screen() -> Result<Monitor> {
    get_all_screens()?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Screen("No screens found".to_string()))
}

pub fn take_screenshot(monitor: &Monitor) -> Result<Vec<u8>> {
    let rgba_img = monitor
        .capture_image()
        .map_err(|e| Error::Screen(format!("Failed to capture: {e}")))?;
    let mut buf = std::io::Cursor::new(Vec::new());
    rgba_img
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| Error::Screen(format!("Failed to encode PNG: {e}")))?;
    Ok(buf.into_inner())
}
