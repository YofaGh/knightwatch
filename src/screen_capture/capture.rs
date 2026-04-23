use xcap::Monitor;

use super::structs::Screenshot;
use crate::prelude::*;

pub fn screenshot_all_screens() -> Result<Vec<Screenshot>> {
    if get_config().args.blind {
        return Ok(vec![]);
    }
    get_all_screens()?
        .into_iter()
        .map(|screen| take_screenshot(&screen))
        .collect()
}

pub fn get_all_screens() -> Result<Vec<Monitor>> {
    Monitor::all().map_err(|e| Error::Screen(format!("Failed to get monitors: {e}")))
}

/// Convenience helper for single-monitor use cases or testing.
/// Kept as public API even though it's currently unused in this crate.
#[allow(dead_code)]
pub fn get_first_screen() -> Result<Monitor> {
    get_all_screens()?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Screen("No screens found".to_string()))
}

pub fn take_screenshot(monitor: &Monitor) -> Result<Screenshot> {
    let rgba_img = monitor
        .capture_image()
        .map_err(|e| Error::Screen(format!("Failed to capture: {e}")))?;
    let timestamp = crate::utils::now_rfc3339();
    let width = rgba_img.width();
    let height = rgba_img.height();
    let mut buf = std::io::Cursor::new(Vec::new());
    rgba_img
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| Error::Screen(format!("Failed to encode PNG: {e}")))?;
    Ok(Screenshot {
        image: buf.into_inner(),
        monitor_name: monitor
            .name()
            .map_err(|e| Error::Screen(format!("Failed to get monitor name: {e}")))?,
        monitor_id: monitor
            .id()
            .map_err(|e| Error::Screen(format!("Failed to get monitor id: {e}")))?,
        width,
        height,
        timestamp,
    })
}
