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

    let current_dir = std::env::current_dir().unwrap();
    let output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "wasm",
            "-p",
            temp_dir.path().to_str().unwrap(),
            "--engine-version",
            &format!(
                "path=\"{current_dir}\"",
                current_dir = current_dir.display()
            ),
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

#[test]
fn test_tinysearch_toml_configuration() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a custom tinysearch.toml
    let toml_content = r#"
[schema]
indexed_fields = ["title", "description", "tags"]
metadata_fields = ["author", "date", "category"]
url_field = "permalink"
"#;
    std::fs::write(temp_dir.path().join("tinysearch.toml"), toml_content)
        .expect("Failed to write tinysearch.toml");

    // Create a custom JSON file with the schema fields
    let json_content = r#"
[
    {
        "title": "Custom Post Title",
        "description": "This is a custom description",
        "tags": "rust webassembly search",
        "permalink": "https://example.com/custom",
        "author": "Test Author",
        "date": "2024-01-15",
        "category": "Technology"
    },
    {
        "title": "Another Post",
        "description": "Different content here",
        "tags": "javascript frontend",
        "permalink": "https://example.com/another",
        "author": "Another Author", 
        "date": "2024-01-20",
        "category": "Development"
    }
]
"#;
    let json_path = temp_dir.path().join("custom_index.json");
    std::fs::write(&json_path, json_content).expect("Failed to write custom JSON file");

    let output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "storage",
            "-p",
            temp_dir.path().to_str().unwrap(),
            json_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("Custom schema build failed. Stdout: {}", stdout);
        eprintln!("Stderr: {}", stderr);
        panic!("Custom schema build failed unexpectedly");
    }

    // Check that storage file was created
    let storage_path = temp_dir.path().join("storage");
    assert!(
        storage_path.exists(),
        "Storage file should be created with custom schema"
    );

    // Test search functionality with the custom schema
    let search_output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "search",
            "-S",
            "rust",
            "-N",
            "5",
            storage_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute search command");

    assert!(
        search_output.status.success(),
        "Search should work with custom schema"
    );

    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("Custom Post Title"),
        "Should find the custom post"
    );
    assert!(
        search_stdout.contains("https://example.com/custom"),
        "Should contain the custom URL from permalink field"
    );
}

#[test]
fn test_flexible_json_fields() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a tinysearch.toml with non-standard fields
    let toml_content = r#"
[schema]
indexed_fields = ["product_name", "product_description"]
metadata_fields = ["price", "brand", "availability"]
url_field = "product_url"
"#;
    std::fs::write(temp_dir.path().join("tinysearch.toml"), toml_content)
        .expect("Failed to write tinysearch.toml");

    // Create JSON with e-commerce-like fields
    let json_content = r#"
[
    {
        "product_name": "Wireless Headphones",
        "product_description": "High-quality wireless headphones with active noise cancellation",
        "product_url": "https://store.example.com/headphones",
        "price": "$299.99",
        "brand": "AudioTech",
        "availability": "In Stock"
    },
    {
        "product_name": "Bluetooth Speaker",
        "product_description": "Portable waterproof speaker with excellent sound quality",
        "product_url": "https://store.example.com/speaker",
        "price": "$149.99", 
        "brand": "SoundWave",
        "availability": "Limited Stock"
    }
]
"#;
    let json_path = temp_dir.path().join("products.json");
    std::fs::write(&json_path, json_content).expect("Failed to write products JSON file");

    let output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "storage",
            "-p",
            temp_dir.path().to_str().unwrap(),
            json_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("Flexible fields build failed. Stdout: {}", stdout);
        eprintln!("Stderr: {}", stderr);
        panic!("Flexible fields build failed unexpectedly");
    }

    // Verify storage was created
    let storage_path = temp_dir.path().join("storage");
    assert!(
        storage_path.exists(),
        "Storage file should be created with flexible fields"
    );

    // Test search works with the custom product fields
    let search_output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "search",
            "-S",
            "wireless",
            "-N",
            "1",
            storage_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute search command");

    assert!(
        search_output.status.success(),
        "Search should work with flexible product fields"
    );

    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("Wireless Headphones"),
        "Should find the wireless headphones product"
    );
    assert!(
        search_stdout.contains("https://store.example.com/headphones"),
        "Should contain the product URL"
    );
}
