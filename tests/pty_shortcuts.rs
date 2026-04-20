use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn run_probe(chunks: &[&[u8]]) -> String {
    run_probe_with_env::<&str, &str>(chunks, &[])
}

fn run_probe_with_env<K, V>(chunks: &[&[u8]], envs: &[(K, V)]) -> String
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let probe = env!("CARGO_BIN_EXE_shortcut_probe");
    let mut command = Command::new("/usr/bin/script");
    command
        .args(["-qfec", probe, "/dev/null"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .unwrap_or_else(|err| panic!("failed to spawn PTY probe: {err}"));

    let stdin = child
        .stdin
        .as_mut()
        .unwrap_or_else(|| panic!("probe stdin unavailable"));
    for chunk in chunks {
        stdin
            .write_all(chunk)
            .unwrap_or_else(|err| panic!("failed to write PTY input chunk: {err}"));
        stdin
            .flush()
            .unwrap_or_else(|err| panic!("failed to flush PTY input chunk: {err}"));
    }

    let output = child
        .wait_with_output()
        .unwrap_or_else(|err| panic!("failed to collect probe output: {err}"));
    assert!(
        output.status.success(),
        "PTY probe exited with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout)
        .unwrap_or_else(|err| panic!("probe stdout is not valid UTF-8: {err}"))
}

#[test]
fn pty_probe_accepts_ctrl_alt_layout_switch_sequences() {
    let transcript = run_probe(&[b"\x1b[118;7u", b"\x1b[104;7u", b"q"]);
    assert!(
        transcript.contains("step=1 event=key:char(v) mods=KeyModifiers(CONTROL | ALT)"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains(
            "layout_version=1 redraw=true resize=na list_rect=0,1,100,11 detail_rect=0,12,100,17"
        ),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("step=2 event=key:char(h) mods=KeyModifiers(CONTROL | ALT)"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains(
            "layout_version=2 redraw=true resize=na list_rect=0,1,42,28 detail_rect=42,1,58,28"
        ),
        "unexpected transcript:\n{transcript}"
    );
}

#[test]
fn pty_probe_accepts_ctrl_alt_resize_sequences() {
    let transcript = run_probe(&[b"\x1b[61;7u", b"\x1b[45;7u", b"q"]);
    assert!(
        transcript.contains("step=1 event=key:char(=) mods=KeyModifiers(CONTROL | ALT) action=None split=Horizontal focus=List primary_size=Some(47)"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("resize=applied list_rect=0,1,47,28 detail_rect=47,1,53,28"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("step=2 event=key:char(-) mods=KeyModifiers(CONTROL | ALT) action=None split=Horizontal focus=List primary_size=Some(42)"),
        "unexpected transcript:\n{transcript}"
    );
}

#[test]
fn pty_probe_covers_text_input_enter_and_mouse_focus() {
    let transcript = run_probe(&[b"j", b"\x1b[13u", b"\x1b[<0;70;10M", b"q"]);
    assert!(
        transcript.contains(
            "step=1 event=key:char(j) mods=KeyModifiers(0x0) action=LoadDetail(offset=0)"
        ),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains(
            "step=2 event=key:Enter mods=KeyModifiers(0x0) action=Resume(session_id=probe,cwd=/workspace/probe)"
        ),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains(
            "step=3 event=mouse:Down(Left)@69,9 action=None split=Horizontal focus=Detail"
        ),
        "unexpected transcript:\n{transcript}"
    );
}

#[test]
fn pty_probe_logs_resize_boundaries_and_trace_file_output() {
    let trace_dir = tempfile::tempdir().expect("trace tempdir");
    let transcript = run_probe_with_env(
        &[
            b"\x1b[45;7u",
            b"\x1b[45;7u",
            b"\x1b[45;7u",
            b"\x1b[45;7u",
            b"\x1b[45;7u",
            b"\x1b[45;7u",
            b"\x1b[118;7u",
            b"\x1b[45;7u",
            b"\x1b[45;7u",
            b"\x1b[45;7u",
            b"\x1b[45;7u",
            b"q",
        ],
        &[("SESSIONS_MANAGER_TRACE_DIR", trace_dir.path().as_os_str())],
    );

    assert!(
        transcript.contains("step=6 event=key:char(-) mods=KeyModifiers(CONTROL | ALT) action=None split=Horizontal focus=List primary_size=Some(17)"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("resize=blocked list_rect=0,1,17,28 detail_rect=17,1,83,28"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains(
            "step=7 event=key:char(v) mods=KeyModifiers(CONTROL | ALT) action=None split=Vertical"
        ),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("step=11 event=key:char(-) mods=KeyModifiers(CONTROL | ALT) action=None split=Vertical focus=List primary_size=Some(5)"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("resize=blocked list_rect=0,1,100,5 detail_rect=0,6,100,23"),
        "unexpected transcript:\n{transcript}"
    );

    let trace_path = trace_dir.path().join("layout-interaction.log");
    let trace_log = fs::read_to_string(&trace_path)
        .unwrap_or_else(|err| panic!("failed to read trace log {}: {err}", trace_path.display()));
    assert!(trace_log.contains("event=key:char(v) mods=KeyModifiers(CONTROL | ALT)"));
    assert!(trace_log.contains("resize=blocked"));
}
