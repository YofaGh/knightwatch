#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerAction {
    Stop,
    Kill,
    Start,
    Restart,
    Pause,
    Unpause,
}

impl std::fmt::Display for ContainerAction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Stop => write!(f, "stop"),
            Self::Kill => write!(f, "kill"),
            Self::Start => write!(f, "start"),
            Self::Restart => write!(f, "restart"),
            Self::Pause => write!(f, "pause"),
            Self::Unpause => write!(f, "unpause"),
        }
    }
}
