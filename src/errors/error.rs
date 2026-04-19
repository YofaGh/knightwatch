use std::io::Error as IoError;

#[derive(Debug)]
pub enum Error {
    Network(String),
    Screen(String),
    Config(String),
    ProcessTracker(String),
    Other(String),
}

impl Error {
    pub fn bind_address(address: &str, err: IoError) -> Self {
        Self::Network(format!("Failed to bind address: {address}, {err}"))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Other(msg)
            | Error::Screen(msg)
            | Error::Config(msg)
            | Error::Network(msg)
            | Error::ProcessTracker(msg) => {
                write!(f, "{msg}")
            }
        }
    }
}
