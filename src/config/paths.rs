use directories::ProjectDirs;
use std::path::PathBuf;

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from("com", "", "screenwatch").expect("Could not determine app data directory")
}

pub fn config_file_path() -> PathBuf {
    project_dirs().config_dir().join("config.json")
}

pub fn config_dir_path() -> PathBuf {
    project_dirs().config_dir().to_path_buf()
}
