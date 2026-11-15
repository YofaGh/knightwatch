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
        // write a private temp file next to the target, then rename it into place,
        // so a crash never leaves a half-written or world-readable file
        let write_err =
            |e: std::io::Error| Error::Config(format!("Failed to write {} file: {e}", Self::NAME));
        let tmp = path.with_extension("tmp");
        let _ = std::fs::remove_file(&tmp); // a stale one would keep its old mode
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&tmp).map_err(write_err)?;
        let written = std::io::Write::write_all(&mut file, contents.as_bytes())
            .and_then(|()| file.sync_all())
            .and_then(|()| std::fs::rename(&tmp, &path));
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        written.map_err(write_err)
    }
}
