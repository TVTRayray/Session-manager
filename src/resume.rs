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
pub struct NewSessionRequest {
    pub engine: SessionEngine,
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

pub trait NewSessionExecutor {
    fn start_new_session(&self, request: &NewSessionRequest) -> Result<(), ResumeSessionError>;
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

impl Default for CodexResumeExecutor<ProcessResumeCommandRunner> {
    fn default() -> Self {
        Self::new()
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
pub struct CodexNewSessionExecutor<R = ProcessResumeCommandRunner> {
    runner: R,
}

impl CodexNewSessionExecutor<ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessResumeCommandRunner,
        }
    }
}

impl Default for CodexNewSessionExecutor<ProcessResumeCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> CodexNewSessionExecutor<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ResumeCommandRunner> NewSessionExecutor for CodexNewSessionExecutor<R> {
    fn start_new_session(&self, request: &NewSessionRequest) -> Result<(), ResumeSessionError> {
        run_resume_command(&self.runner, "codex", &[], &request.cwd, "codex")
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

impl Default for ClaudeResumeExecutor<ProcessResumeCommandRunner> {
    fn default() -> Self {
        Self::new()
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
pub struct ClaudeNewSessionExecutor<R = ProcessResumeCommandRunner> {
    runner: R,
}

impl ClaudeNewSessionExecutor<ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessResumeCommandRunner,
        }
    }
}

impl Default for ClaudeNewSessionExecutor<ProcessResumeCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> ClaudeNewSessionExecutor<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ResumeCommandRunner> NewSessionExecutor for ClaudeNewSessionExecutor<R> {
    fn start_new_session(&self, request: &NewSessionRequest) -> Result<(), ResumeSessionError> {
        #[cfg(target_os = "windows")]
        let program = "claude.cmd";
        #[cfg(not(target_os = "windows"))]
        let program = "claude";

        run_resume_command(&self.runner, program, &[], &request.cwd, "claude")
    }
}

#[derive(Clone, Debug)]
pub struct OpenCodeResumeExecutor<R = ProcessResumeCommandRunner> {
    runner: R,
}

impl OpenCodeResumeExecutor<ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessResumeCommandRunner,
        }
    }
}

impl Default for OpenCodeResumeExecutor<ProcessResumeCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> OpenCodeResumeExecutor<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ResumeCommandRunner> ResumeSessionExecutor for OpenCodeResumeExecutor<R> {
    fn resume_session(&self, request: &ResumeSessionRequest) -> Result<(), ResumeSessionError> {
        #[cfg(target_os = "windows")]
        let program = "opencode.cmd";
        #[cfg(not(target_os = "windows"))]
        let program = "opencode";

        let cwd = request.cwd.to_string_lossy();
        run_resume_command(
            &self.runner,
            program,
            &[cwd.as_ref(), "--session", request.session_id.as_str()],
            &request.cwd,
            "opencode --session",
        )
    }
}

#[derive(Clone, Debug)]
pub struct OpenCodeNewSessionExecutor<R = ProcessResumeCommandRunner> {
    runner: R,
}

impl OpenCodeNewSessionExecutor<ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessResumeCommandRunner,
        }
    }
}

impl Default for OpenCodeNewSessionExecutor<ProcessResumeCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> OpenCodeNewSessionExecutor<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ResumeCommandRunner> NewSessionExecutor for OpenCodeNewSessionExecutor<R> {
    fn start_new_session(&self, request: &NewSessionRequest) -> Result<(), ResumeSessionError> {
        #[cfg(target_os = "windows")]
        let program = "opencode.cmd";
        #[cfg(not(target_os = "windows"))]
        let program = "opencode";

        let cwd = request.cwd.to_string_lossy();
        run_resume_command(
            &self.runner,
            program,
            &[cwd.as_ref()],
            &request.cwd,
            "opencode",
        )
    }
}

#[derive(Clone, Debug)]
pub struct PiResumeExecutor<R = ProcessResumeCommandRunner> {
    runner: R,
}

impl PiResumeExecutor<ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessResumeCommandRunner,
        }
    }
}

impl Default for PiResumeExecutor<ProcessResumeCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> PiResumeExecutor<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ResumeCommandRunner> ResumeSessionExecutor for PiResumeExecutor<R> {
    fn resume_session(&self, request: &ResumeSessionRequest) -> Result<(), ResumeSessionError> {
        run_resume_command(
            &self.runner,
            "pi",
            &["--session", request.session_id.as_str()],
            &request.cwd,
            "pi --session",
        )
    }
}

#[derive(Clone, Debug)]
pub struct PiNewSessionExecutor<R = ProcessResumeCommandRunner> {
    runner: R,
}

impl PiNewSessionExecutor<ProcessResumeCommandRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessResumeCommandRunner,
        }
    }
}

impl Default for PiNewSessionExecutor<ProcessResumeCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> PiNewSessionExecutor<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ResumeCommandRunner> NewSessionExecutor for PiNewSessionExecutor<R> {
    fn start_new_session(&self, request: &NewSessionRequest) -> Result<(), ResumeSessionError> {
        run_resume_command(&self.runner, "pi", &[], &request.cwd, "pi")
    }
}

#[derive(Clone, Debug)]
pub struct EngineAwareResumeExecutor<
    RPi = ProcessResumeCommandRunner,
    RCodex = ProcessResumeCommandRunner,
    RClaude = ProcessResumeCommandRunner,
    ROpenCode = ProcessResumeCommandRunner,
> {
    pi: PiResumeExecutor<RPi>,
    codex: CodexResumeExecutor<RCodex>,
    claude: ClaudeResumeExecutor<RClaude>,
    opencode: OpenCodeResumeExecutor<ROpenCode>,
}

impl
    EngineAwareResumeExecutor<
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
    >
{
    pub fn new() -> Self {
        Self {
            pi: PiResumeExecutor::new(),
            codex: CodexResumeExecutor::new(),
            claude: ClaudeResumeExecutor::new(),
            opencode: OpenCodeResumeExecutor::new(),
        }
    }
}

impl Default
    for EngineAwareResumeExecutor<
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
    >
{
    fn default() -> Self {
        Self::new()
    }
}

impl<RPi, RCodex, RClaude, ROpenCode> EngineAwareResumeExecutor<RPi, RCodex, RClaude, ROpenCode> {
    pub fn with_executors(
        pi: PiResumeExecutor<RPi>,
        codex: CodexResumeExecutor<RCodex>,
        claude: ClaudeResumeExecutor<RClaude>,
        opencode: OpenCodeResumeExecutor<ROpenCode>,
    ) -> Self {
        Self {
            pi,
            codex,
            claude,
            opencode,
        }
    }
}

impl<
    RPi: ResumeCommandRunner,
    RCodex: ResumeCommandRunner,
    RClaude: ResumeCommandRunner,
    ROpenCode: ResumeCommandRunner,
> ResumeSessionExecutor for EngineAwareResumeExecutor<RPi, RCodex, RClaude, ROpenCode>
{
    fn resume_session(&self, request: &ResumeSessionRequest) -> Result<(), ResumeSessionError> {
        match request.engine {
            SessionEngine::Pi => self.pi.resume_session(request),
            SessionEngine::Codex => self.codex.resume_session(request),
            SessionEngine::Claude => self.claude.resume_session(request),
            SessionEngine::OpenCode => self.opencode.resume_session(request),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EngineAwareNewSessionExecutor<
    RPi = ProcessResumeCommandRunner,
    RCodex = ProcessResumeCommandRunner,
    RClaude = ProcessResumeCommandRunner,
    ROpenCode = ProcessResumeCommandRunner,
> {
    pi: PiNewSessionExecutor<RPi>,
    codex: CodexNewSessionExecutor<RCodex>,
    claude: ClaudeNewSessionExecutor<RClaude>,
    opencode: OpenCodeNewSessionExecutor<ROpenCode>,
}

impl
    EngineAwareNewSessionExecutor<
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
    >
{
    pub fn new() -> Self {
        Self {
            pi: PiNewSessionExecutor::new(),
            codex: CodexNewSessionExecutor::new(),
            claude: ClaudeNewSessionExecutor::new(),
            opencode: OpenCodeNewSessionExecutor::new(),
        }
    }
}

impl Default
    for EngineAwareNewSessionExecutor<
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
        ProcessResumeCommandRunner,
    >
{
    fn default() -> Self {
        Self::new()
    }
}

impl<RPi, RCodex, RClaude, ROpenCode>
    EngineAwareNewSessionExecutor<RPi, RCodex, RClaude, ROpenCode>
{
    pub fn with_executors(
        pi: PiNewSessionExecutor<RPi>,
        codex: CodexNewSessionExecutor<RCodex>,
        claude: ClaudeNewSessionExecutor<RClaude>,
        opencode: OpenCodeNewSessionExecutor<ROpenCode>,
    ) -> Self {
        Self {
            pi,
            codex,
            claude,
            opencode,
        }
    }
}

impl<
    RPi: ResumeCommandRunner,
    RCodex: ResumeCommandRunner,
    RClaude: ResumeCommandRunner,
    ROpenCode: ResumeCommandRunner,
> NewSessionExecutor for EngineAwareNewSessionExecutor<RPi, RCodex, RClaude, ROpenCode>
{
    fn start_new_session(&self, request: &NewSessionRequest) -> Result<(), ResumeSessionError> {
        match request.engine {
            SessionEngine::Pi => self.pi.start_new_session(request),
            SessionEngine::Codex => self.codex.start_new_session(request),
            SessionEngine::Claude => self.claude.start_new_session(request),
            SessionEngine::OpenCode => self.opencode.start_new_session(request),
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
    fn resume_executor_rejects_empty_missing_or_non_directory_cwd_before_spawn() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let file = temp.path().join("not-a-directory");
        std::fs::write(&file, "not a directory")
            .unwrap_or_else(|err| panic!("fixture write failed: {err}"));
        let runner = RecordingRunner::default();
        let executor = CodexResumeExecutor::with_runner(runner.clone());

        for cwd in [
            PathBuf::new(),
            PathBuf::from("/definitely/missing/cwd"),
            file,
        ] {
            let result = executor.resume_session(&ResumeSessionRequest {
                engine: SessionEngine::Codex,
                session_id: "session-123".to_string(),
                cwd,
            });
            assert!(result.is_err());
        }
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
    fn pi_resume_executor_preserves_argv_boundaries_and_request_cwd() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner = RecordingRunner::default();
        let executor = PiResumeExecutor::with_runner(runner.clone());
        let request = ResumeSessionRequest {
            engine: SessionEngine::Pi,
            session_id: "session with spaces;not-shell-code".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        assert!(executor.resume_session(&request).is_ok());
        assert_eq!(
            runner.calls(),
            vec![(
                "pi".to_string(),
                vec![
                    "--session".to_string(),
                    "session with spaces;not-shell-code".to_string()
                ],
                temp.path().to_path_buf()
            )]
        );
    }

    #[test]
    fn opencode_resume_executor_uses_platform_command_session_arg_and_request_cwd() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner = RecordingRunner::default();
        let executor = OpenCodeResumeExecutor::with_runner(runner.clone());
        let request = ResumeSessionRequest {
            engine: SessionEngine::OpenCode,
            session_id: "opencode-session".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        assert!(executor.resume_session(&request).is_ok());

        #[cfg(target_os = "windows")]
        let expected_program = "opencode.cmd";
        #[cfg(not(target_os = "windows"))]
        let expected_program = "opencode";
        assert_eq!(
            runner.calls(),
            vec![(
                expected_program.to_string(),
                vec![
                    temp.path().to_string_lossy().into_owned(),
                    "--session".to_string(),
                    "opencode-session".to_string(),
                ],
                temp.path().to_path_buf(),
            )]
        );
    }

    #[test]
    fn resume_executor_reports_termination_without_exit_status() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner = RecordingRunner::with_output(Ok(ResumeCommandOutput {
            success: false,
            code: None,
        }));
        let executor = PiResumeExecutor::with_runner(runner);
        let request = ResumeSessionRequest {
            engine: SessionEngine::Pi,
            session_id: "session-123".to_string(),
            cwd: temp.path().to_path_buf(),
        };

        let result = executor.resume_session(&request);
        match result {
            Ok(()) => panic!("expected missing exit status to fail"),
            Err(err) => assert!(err.message().contains("terminated without an exit status")),
        }
    }

    #[test]
    fn engine_aware_executor_dispatches_all_engines_without_substitution() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let pi_runner = RecordingRunner::default();
        let codex_runner = RecordingRunner::default();
        let claude_runner = RecordingRunner::default();
        let opencode_runner = RecordingRunner::default();
        let executor = EngineAwareResumeExecutor::with_executors(
            PiResumeExecutor::with_runner(pi_runner.clone()),
            CodexResumeExecutor::with_runner(codex_runner.clone()),
            ClaudeResumeExecutor::with_runner(claude_runner.clone()),
            OpenCodeResumeExecutor::with_runner(opencode_runner.clone()),
        );

        for (engine, session_id) in [
            (SessionEngine::Pi, "pi-session"),
            (SessionEngine::Codex, "codex-session"),
            (SessionEngine::Claude, "claude-session"),
            (SessionEngine::OpenCode, "opencode-session"),
        ] {
            assert!(
                executor
                    .resume_session(&ResumeSessionRequest {
                        engine,
                        session_id: session_id.to_string(),
                        cwd: temp.path().to_path_buf(),
                    })
                    .is_ok()
            );
        }

        assert_eq!(pi_runner.calls().len(), 1);
        assert_eq!(codex_runner.calls().len(), 1);
        assert_eq!(claude_runner.calls().len(), 1);
        assert_eq!(opencode_runner.calls().len(), 1);
        assert_eq!(pi_runner.calls()[0].0, "pi");
        assert_eq!(codex_runner.calls()[0].0, "codex");
        #[cfg(target_os = "windows")]
        assert_eq!(claude_runner.calls()[0].0, "claude.cmd");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(claude_runner.calls()[0].0, "claude");
        #[cfg(target_os = "windows")]
        assert_eq!(opencode_runner.calls()[0].0, "opencode.cmd");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(opencode_runner.calls()[0].0, "opencode");
    }

    #[test]
    fn new_session_executor_uses_bare_codex_command_and_request_cwd() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let runner = RecordingRunner::default();
        let executor = CodexNewSessionExecutor::with_runner(runner.clone());
        let request = NewSessionRequest {
            engine: SessionEngine::Codex,
            cwd: temp.path().to_path_buf(),
        };

        let result = executor.start_new_session(&request);

        assert!(result.is_ok());
        assert_eq!(
            runner.calls(),
            vec![("codex".to_string(), Vec::new(), temp.path().to_path_buf())]
        );
    }

    #[test]
    fn pi_and_opencode_new_session_executors_use_bare_commands_and_request_cwd() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let pi_runner = RecordingRunner::default();
        let opencode_runner = RecordingRunner::default();

        assert!(
            PiNewSessionExecutor::with_runner(pi_runner.clone())
                .start_new_session(&NewSessionRequest {
                    engine: SessionEngine::Pi,
                    cwd: temp.path().to_path_buf(),
                })
                .is_ok()
        );
        assert!(
            OpenCodeNewSessionExecutor::with_runner(opencode_runner.clone())
                .start_new_session(&NewSessionRequest {
                    engine: SessionEngine::OpenCode,
                    cwd: temp.path().to_path_buf(),
                })
                .is_ok()
        );

        assert_eq!(
            pi_runner.calls(),
            vec![("pi".to_string(), Vec::new(), temp.path().to_path_buf())]
        );
        #[cfg(target_os = "windows")]
        let expected_program = "opencode.cmd";
        #[cfg(not(target_os = "windows"))]
        let expected_program = "opencode";
        assert_eq!(
            opencode_runner.calls(),
            vec![(
                expected_program.to_string(),
                vec![temp.path().to_string_lossy().into_owned()],
                temp.path().to_path_buf(),
            )]
        );
    }

    #[test]
    fn engine_aware_new_session_executor_dispatches_all_engines_without_substitution() {
        let temp = tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
        let pi_runner = RecordingRunner::default();
        let codex_runner = RecordingRunner::default();
        let claude_runner = RecordingRunner::default();
        let opencode_runner = RecordingRunner::default();
        let executor = EngineAwareNewSessionExecutor::with_executors(
            PiNewSessionExecutor::with_runner(pi_runner.clone()),
            CodexNewSessionExecutor::with_runner(codex_runner.clone()),
            ClaudeNewSessionExecutor::with_runner(claude_runner.clone()),
            OpenCodeNewSessionExecutor::with_runner(opencode_runner.clone()),
        );

        for engine in [
            SessionEngine::Pi,
            SessionEngine::Codex,
            SessionEngine::Claude,
            SessionEngine::OpenCode,
        ] {
            assert!(
                executor
                    .start_new_session(&NewSessionRequest {
                        engine,
                        cwd: temp.path().to_path_buf(),
                    })
                    .is_ok()
            );
        }

        assert_eq!(pi_runner.calls().len(), 1);
        assert_eq!(codex_runner.calls().len(), 1);
        assert_eq!(claude_runner.calls().len(), 1);
        assert_eq!(opencode_runner.calls().len(), 1);
        assert_eq!(pi_runner.calls()[0].0, "pi");
        assert_eq!(codex_runner.calls()[0].0, "codex");
        #[cfg(target_os = "windows")]
        assert_eq!(claude_runner.calls()[0].0, "claude.cmd");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(claude_runner.calls()[0].0, "claude");
        #[cfg(target_os = "windows")]
        assert_eq!(opencode_runner.calls()[0].0, "opencode.cmd");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(opencode_runner.calls()[0].0, "opencode");
    }
}
