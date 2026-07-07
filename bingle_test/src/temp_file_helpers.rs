use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn project_tmp_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let project_dir = crate_dir
        .parent()
        .expect("crate directory should have project parent");
    let tmp_dir = project_dir.join("tmp");
    fs::create_dir_all(&tmp_dir).expect("failed to create project tmp directory");
    tmp_dir
}

pub fn project_tmp_file_path(prefix: &str, suffix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    project_tmp_root().join(format!(
        "{}-{}-{}{}",
        prefix,
        std::process::id(),
        nanos,
        suffix
    ))
}

pub fn project_tmp_dir_path(prefix: &str) -> PathBuf {
    let path = project_tmp_file_path(prefix, "");
    fs::create_dir_all(&path).expect("failed to create project tmp subdirectory");
    path
}

pub fn write_project_tmp_file(prefix: &str, suffix: &str, content: &str) -> PathBuf {
    let path = project_tmp_file_path(prefix, suffix);
    fs::write(&path, content).expect("failed to write project tmp file");
    path
}
