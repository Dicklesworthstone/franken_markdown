//! Execute the bundled offline renderer, not just assertions on its source.
#![cfg(not(target_arch = "wasm32"))]

#[test]
fn interactive_live_renderer_behavior() {
    use std::process::Command;

    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("SKIP interactive renderer runtime proof: Node unavailable");
        return;
    }
    let output = Command::new("node")
        .args([
            "--test",
            "tests/interactive_renderer.test.mjs",
            "tests/interactive_controller.test.mjs",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();
    match output {
        Ok(output) => assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(error) => panic!("start interactive renderer behavior tests: {error}"),
    }
}
