mod enums;
mod models;
mod structs;

#[cfg(feature = "ssr")]
mod client;
#[cfg(feature = "ssr")]
mod tracker;
#[cfg(feature = "ssr")]
mod utils;

mod process_state_serde {
    use super::enums::ProcessState;
    pub fn serialize<S>(state: &ProcessState, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&state.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ProcessState, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use ProcessState;
        use serde::Deserialize;

        let s = String::deserialize(deserializer)?;

        match s.as_str() {
            "running" => Ok(ProcessState::Running),
            "sleeping" => Ok(ProcessState::Sleeping),
            "gone" => Ok(ProcessState::Gone),
            other => {
                if let Some(inner) = other
                    .strip_prefix("other(")
                    .and_then(|s| s.strip_suffix(")"))
                {
                    Ok(ProcessState::Other(inner.to_string()))
                } else {
                    Err(serde::de::Error::custom(format!(
                        "unknown process state: {other:?}"
                    )))
                }
            }
        }
    }
}

#[cfg(feature = "ssr")]
pub use client::*;
pub use models::*;
#[cfg(feature = "ssr")]
pub use tracker::init_process_tracker;
