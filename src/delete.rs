use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::SessionEngine;
use crate::catalog::validate_session_path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteRequest {
    pub engine: SessionEngine,
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
    fn delete_session(&self, request: &DeleteRequest) -> Result<(), DeleteError>;
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
    fn delete_session(&self, request: &DeleteRequest) -> Result<(), DeleteError> {
        delete_session_file(&self.base_dir, &request.path)
    }
}

#[derive(Clone, Debug)]
pub struct EngineAwareSessionDeleteExecutor {
    home_dir: PathBuf,
}

impl EngineAwareSessionDeleteExecutor {
    pub fn from_home_dir(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }
}

impl SessionDeleteExecutor for EngineAwareSessionDeleteExecutor {
    fn delete_session(&self, request: &DeleteRequest) -> Result<(), DeleteError> {
        let root_dir = request.engine.root_dir(&self.home_dir);
        delete_session_file(&root_dir, &request.path)
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
    use crate::catalog::SessionEngine;
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

    #[test]
    fn engine_aware_executor_uses_claude_root() {
        let home = must(tempdir());
        let claude_root = home.path().join(".claude").join("projects");
        must(fs::create_dir_all(&claude_root));
        let claude_session = claude_root.join("claude.jsonl");
        must(fs::write(&claude_session, "{}\n"));

        let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.path().to_path_buf());
        let request = DeleteRequest {
            engine: SessionEngine::Claude,
            path: claude_session.clone(),
            session_id: "claude".to_string(),
        };

        must(executor.delete_session(&request));
        assert!(!claude_session.exists());
    }

    #[test]
    fn engine_aware_executor_rejects_claude_delete_outside_claude_root() {
        let home = must(tempdir());
        let claude_root = home.path().join(".claude").join("projects");
        let codex_root = home.path().join(".codex").join("sessions");
        must(fs::create_dir_all(&claude_root));
        must(fs::create_dir_all(&codex_root));
        let codex_session = codex_root.join("codex.jsonl");
        must(fs::write(&codex_session, "{}\n"));

        let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.path().to_path_buf());
        let request = DeleteRequest {
            engine: SessionEngine::Claude,
            path: codex_session,
            session_id: "wrong-root".to_string(),
        };

        match executor.delete_session(&request) {
            Ok(()) => panic!("expected rejection for non-claude root"),
            Err(DeleteError::RejectedPath(message)) => {
                assert!(message.contains("Rejected out-of-root session file"))
            }
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }
}
