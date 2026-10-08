//! Checks actual executable output across two runs.
#[test]
fn output_is_repeatable() {
    let exe = env!("CARGO_BIN_EXE_hello-world");
    let first = std::process::Command::new(exe).output().unwrap();
    let second = std::process::Command::new(exe).output().unwrap();
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(
        String::from_utf8(first.stdout).unwrap().trim(),
        "{\"entity_id\":0,\"ticks\":4,\"radius\":1.5,\"signed_distance\":0.5}"
    );
}
