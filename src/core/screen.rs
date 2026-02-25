use screenshots::{
    Screen,
    image::{ImageBuffer, Rgba},
};

use crate::prelude::*;

pub fn get_all_screens() -> Result<Vec<Screen>> {
    Screen::all().map_err(|err| Error::Screen(format!("Failed to get screens: {err}")))
}

#[allow(dead_code)]
pub fn get_first_screen() -> Result<Screen> {
    get_all_screens()?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Screen("No screens found".to_string()))
}

pub fn screenshot_all_screens() -> Result<Vec<ImageBuffer<Rgba<u8>, Vec<u8>>>> {
    get_all_screens()?
        .into_iter()
        .map(|screen| take_screenshot(&screen))
        .collect()
}

pub fn take_screenshot(screen: &Screen) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    screen
        .capture()
        .map_err(|err| Error::Screen(format!("Failed to capture screen: {err}")))
}
