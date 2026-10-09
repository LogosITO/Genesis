//! Deterministic CPU inspector output for external definition files.

use std::{path::PathBuf, process::Command};

#[test]
fn default_summary_is_repeatable_and_definitions_differ() {
    let executable = env!("CARGO_BIN_EXE_authoring-inspect");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run = |name| {
        let output = Command::new(executable)
            .arg(root.join(name))
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    let first = run("branch-a.json");
    assert_eq!(first, run("branch-a.json"));
    let a: serde_json::Value = serde_json::from_slice(&first).unwrap();
    let b: serde_json::Value = serde_json::from_slice(&run("branch-b.json")).unwrap();
    assert_eq!(a["segments"], 26);
    assert_eq!(b["segments"], 15);
    assert_ne!(a, b);
    assert!(a.get("load_compile_ms").is_none());
}
