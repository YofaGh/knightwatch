use crate::prelude::*;

pub trait JsonStore: Sized + Default + serde::Serialize + for<'de> serde::Deserialize<'de> {
    const NAME: &'static str;
    fn path() -> Option<std::path::PathBuf>;

    fn load() -> Result<Self> {
        let path = Self::path();
        let Some(path) = path else {
            return Ok(Self::default());
        };
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| Error::Config(format!("Failed to read {} file: {e}", Self::NAME)))?;
        serde_json::from_str(&contents)
            .map_err(|e| Error::Config(format!("Failed to parse {} file: {e}", Self::NAME)))
    }

    fn save(&self) -> Result<()> {
        let Some(path) = Self::path() else {
            return Err(Error::Config(format!(
                "Failed to determine path for {} file",
                Self::NAME
            )));
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Error::Config(format!("Failed to create directory: {e}")))?;
        }
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("Failed to serialize {}: {e}", Self::NAME)))?;
        std::fs::write(path, contents)
            .map_err(|e| Error::Config(format!("Failed to write {} file: {e}", Self::NAME)))
    }
}
