use directories::ProjectDirs;
use std::path::PathBuf;

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "", "knightwatch")
}

pub fn config_file_path(file: &'static str) -> Option<PathBuf> {
    project_dirs().map(|dir| dir.config_dir().join(file))
}

pub fn data_file_path(file: &'static str) -> Option<PathBuf> {
    project_dirs().map(|dir| dir.data_local_dir().join(file))
}
