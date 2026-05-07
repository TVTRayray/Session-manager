use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::catalog::SessionEngine;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeSessionRequest {
    pub engine: SessionEngine,
    pub session_id: String,
    pub cwd: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeCommandOutput {
    pub success: bool,
    pub code: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeSessionError {
    message: String,
}

impl ResumeSessionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub trait ResumeCommandRunner {
    fn run(&self, program: &str, args: &[&str], cwd: &Path) -> Result<ResumeCommandOutput, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessResumeCommandRunner;

impl ResumeCommandRunner for ProcessResumeCommandRunner {
    fn run(&self, program: &str, args: &[&str], cwd: &Path) -> Result<ResumeCommandOutput, String> {
        let status = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|err| format!("Failed to launch {program}: {err}"))?;

        Ok(ResumeCommandOutput {
            success: status.success(),
            code: status.code(),
        })
    }
}

pub trait ResumeSessionExecutor {
    fn resume_session(&self, request: &ResumeSessionRequest) -> Result<(), ResumeSessionError>;
}

#[derive(Clone, Debug)]
pub struct CodexResumeExecutor<R = ProcessResumeCommandRunner> {
    runner: R,
}

impl CodexResumeExecutor<ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessResumeCommandRunner,
        }
    }
}

impl<R> CodexResumeExecutor<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ResumeCommandRunner> ResumeSessionExecutor for CodexResumeExecutor<R> {
    fn resume_session(&self, request: &ResumeSessionRequest) -> Result<(), ResumeSessionError> {
        run_resume_command(
            &self.runner,
            "codex",
            &["resume", request.session_id.as_str()],
            &request.cwd,
            "codex resume",
        )
    }
}

#[derive(Clone, Debug)]
pub struct ClaudeResumeExecutor<R = ProcessResumeCommandRunner> {
    runner: R,
}

impl ClaudeResumeExecutor<ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessResumeCommandRunner,
        }
    }
}

impl<R> ClaudeResumeExecutor<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ResumeCommandRunner> ResumeSessionExecutor for ClaudeResumeExecutor<R> {
    fn resume_session(&self, request: &ResumeSessionRequest) -> Result<(), ResumeSessionError> {
        #[cfg(target_os = "windows")]
        let program = "claude.cmd";
        #[cfg(not(target_os = "windows"))]
        let program = "claude";

        run_resume_command(
            &self.runner,
            program,
            &["--resume", request.session_id.as_str()],
            &request.cwd,
            "claude --resume",
        )
    }
}

#[derive(Clone, Debug)]
pub struct EngineAwareResumeExecutor<
    RCodex = ProcessResumeCommandRunner,
    RClaude = ProcessResumeCommandRunner,
> {
    codex: CodexResumeExecutor<RCodex>,
    claude: ClaudeResumeExecutor<RClaude>,
}

impl EngineAwareResumeExecutor<ProcessResumeCommandRunner, ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            codex: CodexResumeExecutor::new(),
            claude: ClaudeResumeExecutor::new(),
        }
    }
}

impl<RCodex, RClaude> EngineAwareResumeExecutor<RCodex, RClaude> {
    pub fn with_executors(
        codex: CodexResumeExecutor<RCodex>,
        claude: ClaudeResumeExecutor<RClaude>,
    ) -> Self {
        Self { codex, claude }
    }
}

impl<RCodex: ResumeCommandRunner, RClaude: ResumeCommandRunner> ResumeSessionExecutor
    for EngineAwareResumeExecutor<RCodex, RClaude>
{
    fn resume_session(&self, request: &ResumeSessionRequest) -> Result<(), ResumeSessionError> {
        match request.engine {
            SessionEngine::Codex => self.codex.resume_session(request),
            SessionEngine::Claude => self.claude.resume_session(request),
        }
    }
}

fn run_resume_command<R: ResumeCommandRunner>(
    runner: &R,
    program: &str,
    args: &[&str],
    cwd: &Path,
    command_label: &str,
) -> Result<(), ResumeSessionError> {
    validate_resume_cwd(cwd)?;

    let output = runner
        .run(program, args, cwd)
        .map_err(ResumeSessionError::new)?;

    if output.success {
        Ok(())
    } else {
        Err(ResumeSessionError::new(match output.code {
            Some(code) => format!("{command_label} exited with status {code}"),
            None => format!("{command_label} terminated without an exit status"),
        }))
    }
}

fn validate_resume_cwd(cwd: &Path) -> Result<(), ResumeSessionError> {
    if cwd.as_os_str().is_empty() {
        return Err(ResumeSessionError::new("Session cwd is missing"));
    }

    let metadata = std::fs::metadata(cwd)
        .map_err(|err| ResumeSessionError::new(format!("Session cwd is not accessible: {err}")))?;

    if !metadata.is_dir() {
        return Err(ResumeSessionError::new(format!(
            "Session cwd is not a directory: {}",
            cwd.display()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use tempfile::tempdir;

    #[derive(Clone, Default)]
    struct RecordingRunner {
        shared: Rc<RefCell<State>>,
    }

    #[derive(Default)]
    struct State {
        calls: Vec<(String, Vec<String>, PathBuf)>,
        output: Option<Result<ResumeCommandOutput, String>>,
    }

    impl RecordingRunner {
        fn with_output(output: Result<ResumeCommandOutput, String>) -> Self {
            let runner = Self::default();
            runner.shared.borrow_mut().output = Some(output);
            runner
        }

        fn calls(&self) -> Vec<(String, Vec<String>, PathBuf)> {
            self.shared.borrow().calls.clone()
        }
    }

    impl ResumeCommandRunner for RecordingRunner {
        fn run(
            &self,
            program: &str,
            args: &[&str],
            cwd: &Path,
        ) -> Result<ResumeCommandOutput, String> {
            self.shared.borrow_mut().calls.push((
                program.to_string(),
                args.iter().map(|arg| (*arg).to_string()).collect(),
                cwd.to_path_buf(),
            ));

            self.shared
                .borrow_mut()
                .output
                .take()
                .unwrap_or(Ok(ResumeCommandOutput {
                    success: true,
                    code: Some(0),
                }))
        }
    }

    #[test]
    fn resume_executor_uses_fixed_codex_resume_command_and_request_cwd() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner = RecordingRunner::default();
        let executor = CodexResumeExecutor::with_runner(runner.clone());
        let request = ResumeSessionRequest {
            engine: SessionEngine::Codex,
            session_id: "session-123".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        let result = executor.resume_session(&request);
        assert!(result.is_ok());
        assert_eq!(
            runner.calls(),
            vec![(
                "codex".to_string(),
                vec!["resume".to_string(), "session-123".to_string()],
                temp.path().to_path_buf()
            )]
        );
    }

    #[test]
    fn resume_executor_rejects_missing_or_invalid_cwd_before_spawn() {
        let runner = RecordingRunner::default();
        let executor = CodexResumeExecutor::with_runner(runner.clone());
        let request = ResumeSessionRequest {
            engine: SessionEngine::Codex,
            session_id: "session-123".to_string(),
            cwd: PathBuf::from("/definitely/missing/cwd"),
        };

        let result = executor.resume_session(&request);
        assert!(result.is_err());
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn resume_executor_surfaces_non_zero_exit() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner = RecordingRunner::with_output(Ok(ResumeCommandOutput {
            success: false,
            code: Some(7),
        }));
        let executor = CodexResumeExecutor::with_runner(runner);
        let request = ResumeSessionRequest {
            engine: SessionEngine::Codex,
            session_id: "session-123".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        let result = executor.resume_session(&request);
        match result {
            Ok(()) => panic!("expected non-zero exit to fail"),
            Err(err) => assert!(err.message().contains("status 7")),
        }
    }

    #[test]
    fn resume_executor_surfaces_launch_failure() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner =
            RecordingRunner::with_output(Err("Failed to launch codex: not found".to_string()));
        let executor = CodexResumeExecutor::with_runner(runner);
        let request = ResumeSessionRequest {
            engine: SessionEngine::Codex,
            session_id: "session-123".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        let result = executor.resume_session(&request);
        match result {
            Ok(()) => panic!("expected launch failure"),
            Err(err) => assert!(err.message().contains("not found")),
        }
    }

    #[test]
    fn claude_resume_executor_uses_fixed_claude_resume_command_and_request_cwd() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner = RecordingRunner::default();
        let executor = ClaudeResumeExecutor::with_runner(runner.clone());
        let request = ResumeSessionRequest {
            engine: SessionEngine::Claude,
            session_id: "session-456".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        let result = executor.resume_session(&request);
        assert!(result.is_ok());
        
        #[cfg(target_os = "windows")]
        let expected_program = "claude.cmd".to_string();
        #[cfg(not(target_os = "windows"))]
        let expected_program = "claude".to_string();

        assert_eq!(
            runner.calls(),
            vec![(
                expected_program,
                vec!["--resume".to_string(), "session-456".to_string()],
                temp.path().to_path_buf()
            )]
        );
    }

    #[test]
    fn claude_resume_executor_surfaces_non_zero_exit() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner = RecordingRunner::with_output(Ok(ResumeCommandOutput {
            success: false,
            code: Some(9),
        }));
        let executor = ClaudeResumeExecutor::with_runner(runner);
        let request = ResumeSessionRequest {
            engine: SessionEngine::Claude,
            session_id: "session-456".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        let result = executor.resume_session(&request);
        match result {
            Ok(()) => panic!("expected non-zero exit to fail"),
            Err(err) => assert!(
                err.message()
                    .contains("claude --resume exited with status 9")
            ),
        }
    }

    #[test]
    fn claude_resume_executor_surfaces_launch_failure() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner =
            RecordingRunner::with_output(Err("Failed to launch claude: not found".to_string()));
        let executor = ClaudeResumeExecutor::with_runner(runner);
        let request = ResumeSessionRequest {
            engine: SessionEngine::Claude,
            session_id: "session-456".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        let result = executor.resume_session(&request);
        match result {
            Ok(()) => panic!("expected launch failure"),
            Err(err) => assert!(err.message().contains("not found")),
        }
    }

    #[test]
    fn engine_aware_executor_dispatches_by_request_engine() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let codex_runner = RecordingRunner::default();
        let claude_runner = RecordingRunner::default();
        let executor = EngineAwareResumeExecutor::with_executors(
            CodexResumeExecutor::with_runner(codex_runner.clone()),
            ClaudeResumeExecutor::with_runner(claude_runner.clone()),
        );

        let codex_request = ResumeSessionRequest {
            engine: SessionEngine::Codex,
            session_id: "codex-session".to_string(),
            cwd: temp.path().to_path_buf(),
        };
        let claude_request = ResumeSessionRequest {
            engine: SessionEngine::Claude,
            session_id: "claude-session".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        assert!(executor.resume_session(&codex_request).is_ok());
        assert!(executor.resume_session(&claude_request).is_ok());

        assert_eq!(codex_runner.calls().len(), 1);
        assert_eq!(claude_runner.calls().len(), 1);
        assert_eq!(codex_runner.calls()[0].0, "codex");
        #[cfg(target_os = "windows")]
        assert_eq!(claude_runner.calls()[0].0, "claude.cmd");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(claude_runner.calls()[0].0, "claude");
    }
}
