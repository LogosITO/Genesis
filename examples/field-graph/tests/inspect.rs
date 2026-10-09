//! Headless JSON validation must work without requesting a GPU adapter.
use std::{path::Path, process::Command};

#[test]
fn inspect_reports_bounded_graph_and_rejects_invalid_source() {
    let binary = env!("CARGO_BIN_EXE_field-graph-preview");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("definitions/balanced-16.json");
    let output = Command::new(binary)
        .args(["--inspect", fixture.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["nodes"], 32);
    assert_eq!(json["visits_per_sample"], 47);
    assert_eq!(json["id"], "balanced_16");
    let invalid = Command::new(binary)
        .args(["--inspect", "does-not-exist.json"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
}
