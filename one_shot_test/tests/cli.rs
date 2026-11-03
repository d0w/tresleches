//! Smoke test for the executable.

use std::process::Command;

#[test]
fn demo_runs_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_tresleches"))
        .output()
        .expect("demo binary should start");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("valid UTF-8 output");
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 5, "{stdout}");
    assert!(lines[0].starts_with("published 58 bytes as "), "{stdout}");
    assert_eq!(lines[1], "stored 8 chunks with 2 replicas each");
    assert_eq!(lines[2], "publisher peer 1 disconnected");
    assert_eq!(lines[3], "reconstructed from 3 chunk providers");
    assert_eq!(
        lines[4],
        "verified: Data belongs to the network, not one supplying connection."
    );
}
