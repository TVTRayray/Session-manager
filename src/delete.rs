use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::validate_session_path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteRequest {
    pub path: PathBuf,
    pub session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeleteError {
    RejectedPath(String),
    Io(String),
}

impl DeleteError {
    pub fn message(&self) -> &str {
        match self {
            DeleteError::RejectedPath(message) | DeleteError::Io(message) => message,
        }
    }
}

impl fmt::Display for DeleteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

pub trait SessionDeleteExecutor {
    fn delete_session(&self, path: &Path) -> Result<(), DeleteError>;
}

#[derive(Clone, Debug)]
pub struct FilesystemSessionDeleteExecutor {
    base_dir: PathBuf,
}

impl FilesystemSessionDeleteExecutor {
    pub fn from_path(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SessionDeleteExecutor for FilesystemSessionDeleteExecutor {
    fn delete_session(&self, path: &Path) -> Result<(), DeleteError> {
        delete_session_file(&self.base_dir, path)
    }
}

pub fn delete_session_file(base_dir: &Path, path: &Path) -> Result<(), DeleteError> {
    let canonical_root = fs::canonicalize(base_dir).map_err(|err| {
        DeleteError::Io(format!(
            "Unable to read session directory {}: {err}",
            base_dir.display()
        ))
    })?;
    let validated_path =
        validate_session_path(&canonical_root, path).map_err(DeleteError::RejectedPath)?;

    fs::remove_file(&validated_path).map_err(|err| {
        DeleteError::Io(format!(
            "Unable to delete session file {}: {err}",
            validated_path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    #[test]
    fn deletes_file_within_root() {
        let dir = must(tempdir());
        let session = dir.path().join("keep.jsonl");
        must(fs::write(&session, "{}\n"));

        must(delete_session_file(dir.path(), &session));
        assert!(!session.exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_path_outside_root() {
        use std::os::unix::fs::symlink;

        let root_dir = must(tempdir());
        let outside_dir = must(tempdir());
        let outside_file = outside_dir.path().join("outside.jsonl");
        must(fs::write(&outside_file, "{}\n"));

        let linked = root_dir.path().join("linked.jsonl");
        must(symlink(&outside_file, &linked));

        let result = delete_session_file(root_dir.path(), &linked);
        match result {
            Ok(_) => panic!("expected rejection for out-of-root path"),
            Err(DeleteError::RejectedPath(message)) => {
                assert!(message.contains("Rejected out-of-root session file"))
            }
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }
}
