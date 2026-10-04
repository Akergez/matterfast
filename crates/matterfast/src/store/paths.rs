use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn db_path(dir: &Path, host: &str) -> PathBuf {
    dir.join(format!("{host}.sqlite"))
}

pub(super) fn remove_at(dir: &Path, host: &str) {
    let path = db_path(dir, host);
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut p = path.clone().into_os_string();
        p.push(suffix);
        let _ = fs::remove_file(p);
    }
}
