#![recursion_limit = "256"]

pub mod api;
pub mod dashboard;
pub mod process_tracker;
pub mod system_monitor;
pub mod utils;

#[cfg(feature = "ssr")]
pub mod config;
#[cfg(feature = "ssr")]
pub mod errors;
#[cfg(feature = "ssr")]
pub mod prelude;
#[cfg(feature = "ssr")]
pub mod screen_capture;
#[cfg(feature = "ssr")]
pub mod types;
