#[cfg(feature = "ssr")]
mod constants;
#[cfg(feature = "ssr")]
mod end_points;
#[cfg(feature = "ssr")]
mod handlers;
mod models;

#[cfg(feature = "ssr")]
pub use handlers::init_api_server;
pub use models::*;
