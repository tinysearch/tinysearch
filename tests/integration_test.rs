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
    // Check if wasm32-unknown-unknown target is available
    let target_check = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .expect("Failed to check installed targets");

    let installed_targets = String::from_utf8_lossy(&target_check.stdout);
    if !installed_targets.contains("wasm32-unknown-unknown") {
        panic!(
            "wasm32-unknown-unknown target is not installed. Install it with: rustup target add wasm32-unknown-unknown"
        );
    }

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

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("WASM build failed. Stdout: {}", stdout);
        eprintln!("Stderr: {}", stderr);
        panic!("WASM build failed unexpectedly");
    }

    // Verify that WASM and JS files were created
    let wasm_files: Vec<_> = std::fs::read_dir(&temp_dir)
        .expect("Failed to read output directory")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "wasm" || ext == "js")
        })
        .collect();

    assert!(!wasm_files.is_empty(), "No WASM/JS files were generated");

    // Specifically check for both .wasm and .js files
    let has_wasm = wasm_files
        .iter()
        .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("wasm"));
    let has_js = wasm_files
        .iter()
        .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("js"));

    assert!(has_wasm, "No .wasm file was generated");
    assert!(has_js, "No .js file was generated");
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
