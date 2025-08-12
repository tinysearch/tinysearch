use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_cli_version() {
    let output = Command::new("cargo")
        .args(["run", "--features=bin", "--", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("tinysearch"));
}

#[test]
fn test_cli_wasm_mode() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    let output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "wasm",
            "-p",
            temp_dir.path().to_str().unwrap(),
            "fixtures/index.json",
        ])
        .output()
        .expect("Failed to execute command");

    // Should succeed or fail gracefully - not crash
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either succeeds or fails due to missing wasm-pack, both are acceptable
    if !output.status.success() {
        // If it fails, it should be due to missing tools, not a crash
        assert!(
            stderr.contains("wasm-pack")
                || stderr.contains("Command")
                || stderr.contains("failed to run"),
            "Unexpected error: {}",
            stderr
        );
    }
}

#[test]
fn test_cli_storage_mode() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    let output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "storage",
            "-p",
            temp_dir.path().to_str().unwrap(),
            "fixtures/index.json",
        ])
        .output()
        .expect("Failed to execute command");

    // Storage mode should work with the provided fixtures
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Command failed: {}", stderr);
    }

    assert!(output.status.success());

    // Check that storage file was created
    let storage_path = temp_dir.path().join("storage");
    assert!(storage_path.exists(), "Storage file should be created");
}
