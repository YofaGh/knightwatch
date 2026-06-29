use directories::ProjectDirs;
use std::path::PathBuf;

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from("com", "", "knightwatch").expect("Could not determine app data directory")
}

pub fn conig_file_path(file: &'static str) -> PathBuf {
    project_dirs().config_dir().join(file)
}

pub fn data_file_path(file: &'static str) -> PathBuf {
    project_dirs().data_local_dir().join(file)
}
