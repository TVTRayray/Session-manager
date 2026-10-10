use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

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
        if request.engine == SessionEngine::OpenCode {
            return delete_opencode_session(&request.session_id);
        }
        let root_dir = request.engine.root_dir(&self.home_dir);
        if request.engine == SessionEngine::Claude {
            return delete_claude_session(&root_dir, &request.path);
        }
        delete_session_file(&root_dir, &request.path)
    }
}

/// Claude keeps per-session companions beside the transcript: `<id>/` (subagent
/// transcripts, tool results, title cache) and `agent-<id>.meta.json` for subagents.
/// Removing only the `.jsonl` would leave them orphaned forever.
fn delete_claude_session(root_dir: &Path, path: &Path) -> Result<(), DeleteError> {
    if fs::symlink_metadata(path).is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound) {
        // A subagent transcript disappears together with its parent's companion directory.
        return Ok(());
    }
    delete_session_file(root_dir, path)?;

    let canonical_root = fs::canonicalize(root_dir).map_err(|err| {
        DeleteError::Io(format!(
            "Unable to read session directory {}: {err}",
            root_dir.display()
        ))
    })?;
    let companion_dir = path.with_extension("");
    if companion_dir.is_dir() {
        let validated = validate_session_path(&canonical_root, &companion_dir)
            .map_err(DeleteError::RejectedPath)?;
        if validated != canonical_root {
            fs::remove_dir_all(&validated).map_err(|err| {
                DeleteError::Io(format!(
                    "Deleted session but could not remove {}: {err}",
                    validated.display()
                ))
            })?;
        }
    }
    let meta_file = path.with_extension("meta.json");
    if meta_file.is_file() {
        let validated = validate_session_path(&canonical_root, &meta_file)
            .map_err(DeleteError::RejectedPath)?;
        let _ = fs::remove_file(validated);
    }
    Ok(())
}

fn delete_opencode_session(session_id: &str) -> Result<(), DeleteError> {
    delete_opencode_session_with(session_id, Command::status)
}

fn delete_opencode_session_with(
    session_id: &str,
    run: impl FnOnce(&mut Command) -> std::io::Result<ExitStatus>,
) -> Result<(), DeleteError> {
    #[cfg(target_os = "windows")]
    let program = "opencode.cmd";
    #[cfg(not(target_os = "windows"))]
    let program = "opencode";

    let mut command = Command::new(program);
    command.args(["session", "delete", session_id]);
    let status = run(&mut command).map_err(|err| {
        DeleteError::Io(format!("Failed to launch opencode session delete: {err}"))
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(DeleteError::Io(match status.code() {
            Some(code) => format!("opencode session delete exited with status {code}"),
            None => "opencode session delete terminated without an exit status".to_string(),
        }))
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
    fn engine_aware_executor_uses_pi_root() {
        let home = must(tempdir());
        let pi_root = home.path().join(".pi").join("agent").join("sessions");
        must(fs::create_dir_all(&pi_root));
        let pi_session = pi_root.join("pi.jsonl");
        must(fs::write(&pi_session, "{}\n"));

        let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.path().to_path_buf());
        let request = DeleteRequest {
            engine: SessionEngine::Pi,
            path: pi_session.clone(),
            session_id: "pi".to_string(),
        };

        must(executor.delete_session(&request));
        assert!(!pi_session.exists());
    }

    #[test]
    fn engine_aware_executor_rejects_pi_delete_outside_pi_root() {
        let home = must(tempdir());
        let pi_root = home.path().join(".pi").join("agent").join("sessions");
        must(fs::create_dir_all(&pi_root));
        let outside = home.path().join("outside.jsonl");
        must(fs::write(&outside, "{}\n"));

        let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.path().to_path_buf());
        let request = DeleteRequest {
            engine: SessionEngine::Pi,
            path: outside,
            session_id: "outside".to_string(),
        };

        assert!(matches!(
            executor.delete_session(&request),
            Err(DeleteError::RejectedPath(message))
                if message.contains("Rejected out-of-root session file")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn engine_aware_executor_rejects_pi_symlink_outside_pi_root() {
        use std::os::unix::fs::symlink;

        let home = must(tempdir());
        let pi_root = home.path().join(".pi").join("agent").join("sessions");
        must(fs::create_dir_all(&pi_root));
        let outside = home.path().join("outside.jsonl");
        must(fs::write(&outside, "{}\n"));
        let linked = pi_root.join("linked.jsonl");
        must(symlink(&outside, &linked));

        let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.path().to_path_buf());
        let request = DeleteRequest {
            engine: SessionEngine::Pi,
            path: linked,
            session_id: "linked".to_string(),
        };

        assert!(matches!(
            executor.delete_session(&request),
            Err(DeleteError::RejectedPath(message))
                if message.contains("Rejected out-of-root session file")
        ));
        assert!(outside.exists());
    }

    #[test]
    fn opencode_delete_uses_exact_argv_and_accepts_success() {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt;

        let session_id = "../../not-a-path; still-one-arg";
        must(delete_opencode_session_with(session_id, |command| {
            #[cfg(target_os = "windows")]
            assert_eq!(command.get_program(), "opencode.cmd");
            #[cfg(not(target_os = "windows"))]
            assert_eq!(command.get_program(), "opencode");
            assert_eq!(
                command.get_args().collect::<Vec<_>>(),
                ["session", "delete", session_id]
            );
            Ok(ExitStatus::from_raw(0))
        }));
    }

    #[test]
    fn opencode_delete_reports_spawn_failure() {
        let error = delete_opencode_session_with("session-id", |_| {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))
        })
        .expect_err("spawn should fail");

        assert!(
            error
                .message()
                .contains("Failed to launch opencode session delete")
        );
        assert!(error.message().contains("missing"));
    }

    #[cfg(unix)]
    #[test]
    fn opencode_delete_reports_nonzero_and_missing_exit_status() {
        use std::os::unix::process::ExitStatusExt;

        let nonzero =
            delete_opencode_session_with("session-id", |_| Ok(ExitStatus::from_raw(7 << 8)))
                .expect_err("nonzero exit should fail");
        assert!(nonzero.message().contains("exited with status 7"));

        let signaled = delete_opencode_session_with("session-id", |_| Ok(ExitStatus::from_raw(9)))
            .expect_err("signal exit should fail");
        assert!(
            signaled
                .message()
                .contains("terminated without an exit status")
        );
    }

    #[test]
    fn engine_aware_executor_uses_codex_root() {
        let home = must(tempdir());
        let codex_root = home.path().join(".codex").join("sessions");
        must(fs::create_dir_all(&codex_root));
        let codex_session = codex_root.join("codex.jsonl");
        must(fs::write(&codex_session, "{}\n"));

        let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.path().to_path_buf());
        let request = DeleteRequest {
            engine: SessionEngine::Codex,
            path: codex_session.clone(),
            session_id: "codex".to_string(),
        };

        must(executor.delete_session(&request));
        assert!(!codex_session.exists());
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
    fn claude_delete_removes_companion_directory_and_tolerates_cascaded_children() {
        let home = must(tempdir());
        let project = home
            .path()
            .join(".claude")
            .join("projects")
            .join("-tmp-demo");
        let subagents = project.join("parent").join("subagents");
        must(fs::create_dir_all(&subagents));
        must(fs::create_dir_all(
            project.join("parent").join("tool-results"),
        ));
        let parent = project.join("parent.jsonl");
        let child = subagents.join("agent-a1.jsonl");
        must(fs::write(&parent, "{}\n"));
        must(fs::write(&child, "{}\n"));
        must(fs::write(subagents.join("agent-a1.meta.json"), "{}"));
        let sibling = project.join("other.jsonl");
        must(fs::write(&sibling, "{}\n"));

        let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.path().to_path_buf());
        let request = |path: &Path, id: &str| DeleteRequest {
            engine: SessionEngine::Claude,
            path: path.to_path_buf(),
            session_id: id.to_string(),
        };

        must(executor.delete_session(&request(&parent, "parent")));
        assert!(!parent.exists());
        assert!(!project.join("parent").exists());
        assert!(sibling.exists());
        must(executor.delete_session(&request(&child, "agent-a1")));
    }

    #[test]
    fn claude_delete_of_subagent_removes_its_meta_file() {
        let home = must(tempdir());
        let subagents = home
            .path()
            .join(".claude")
            .join("projects")
            .join("-tmp-demo")
            .join("parent")
            .join("subagents");
        must(fs::create_dir_all(&subagents));
        let child = subagents.join("agent-a1.jsonl");
        let meta = subagents.join("agent-a1.meta.json");
        must(fs::write(&child, "{}\n"));
        must(fs::write(&meta, "{}"));

        let executor = EngineAwareSessionDeleteExecutor::from_home_dir(home.path().to_path_buf());
        must(executor.delete_session(&DeleteRequest {
            engine: SessionEngine::Claude,
            path: child.clone(),
            session_id: "agent-a1".to_string(),
        }));
        assert!(!child.exists());
        assert!(!meta.exists());
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
