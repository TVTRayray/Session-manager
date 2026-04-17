use std::io::Write;
use std::process::{Command, Stdio};

fn run_probe(input: &[u8]) -> String {
    let probe = env!("CARGO_BIN_EXE_shortcut_probe");
    let mut child = Command::new("/usr/bin/script")
        .args(["-qfec", probe, "/dev/null"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|err| panic!("failed to spawn PTY probe: {err}"));

    child
        .stdin
        .as_mut()
        .unwrap_or_else(|| panic!("probe stdin unavailable"))
        .write_all(input)
        .unwrap_or_else(|err| panic!("failed to write PTY input: {err}"));

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
fn pty_probe_accepts_ctrl_shift_layout_switch_sequences() {
    let transcript = run_probe(b"\x1b[86;6u\x1b[72;6uq");
    assert!(
        transcript.contains("step=1 split=Vertical primary_size=None layout_version=1"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("step=2 split=Horizontal primary_size=None layout_version=2"),
        "unexpected transcript:\n{transcript}"
    );
}

#[test]
fn pty_probe_accepts_ctrl_shift_resize_sequences() {
    let transcript = run_probe(b"\x1b[43;6u\x1b[95;6uq");
    assert!(
        transcript.contains("step=1 split=Horizontal primary_size=Some(47)"),
        "unexpected transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("step=2 split=Horizontal primary_size=Some(42)"),
        "unexpected transcript:\n{transcript}"
    );
}
