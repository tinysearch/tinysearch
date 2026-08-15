//! End-to-end tests for the tinysearch command-line interface.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::missing_docs_in_private_items
)]

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
    assert!(
        installed_targets.contains("wasm32-unknown-unknown"),
        "wasm32-unknown-unknown target is not installed. Install it with: rustup target add wasm32-unknown-unknown"
    );

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let crate_temp_dir = TempDir::new().expect("Failed to create generated crate directory");
    let generated_crate = crate_temp_dir.path().join("generated-crate");

    let current_dir = std::env::current_dir().unwrap();
    let output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "--shard-size",
            "4096",
            "-m",
            "wasm",
            "--crate-path",
            generated_crate.to_str().unwrap(),
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
        eprintln!("WASM build failed. Stdout: {stdout}");
        eprintln!("Stderr: {stderr}");
        panic!("WASM build failed unexpectedly");
    }

    assert_sharded_wasm_artifacts(&temp_dir);
    assert_sharded_loader(&temp_dir);

    assert_release_wasm_output(&temp_dir, &generated_crate, &current_dir);
    assert_xor8_wasm_output(&generated_crate, &current_dir);
}

fn assert_sharded_wasm_artifacts(temp_dir: &TempDir) {
    assert!(temp_dir.path().join("tinysearch_engine.wasm").exists());
    assert!(temp_dir.path().join("tinysearch_engine.js").exists());
    assert!(temp_dir.path().join("tinysearch.root").exists());

    let demo = std::fs::read_to_string(temp_dir.path().join("demo.html"))
        .expect("Failed to read generated demo");
    assert!(demo.contains("addEventListener('input', performSearch)"));
    assert!(demo.contains("Results update as you type"));

    let shard_count = std::fs::read_dir(temp_dir)
        .expect("Failed to inspect generated shards")
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("tinysearch-shard")
        })
        .count();
    assert!(shard_count > 1, "Expected multiple lazy-loadable shards");
}

fn assert_sharded_loader(temp_dir: &TempDir) {
    let output = Command::new("node")
        .args([
            "tests/wasm_loader_test.mjs",
            temp_dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute Node.js loader integration test");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("Loader test failed. Stdout: {stdout}");
        eprintln!("Loader test failed. Stderr: {stderr}");
    }
    assert!(
        output.status.success(),
        "Generated JS/WASM loader integration test failed"
    );
}

fn assert_release_wasm_output(
    temp_dir: &TempDir,
    generated_crate: &std::path::Path,
    current_dir: &std::path::Path,
) {
    let retained_shard = temp_dir
        .path()
        .join("0000000000000000000000000000000000000000000000000000000000000000.tinysearch-shard");
    std::fs::write(&retained_shard, b"previous immutable shard")
        .expect("Failed to write retained shard fixture");

    let release_output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "--release",
            "--shard-size",
            "4096",
            "-m",
            "wasm",
            "--crate-path",
            generated_crate.to_str().unwrap(),
            "-p",
            temp_dir.path().to_str().unwrap(),
            "--engine-version",
            &format!("path=\"{}\"", current_dir.display()),
            "fixtures/index.json",
        ])
        .output()
        .expect("Failed to execute release WASM command");
    if !release_output.status.success() {
        let stderr = String::from_utf8_lossy(&release_output.stderr);
        let stdout = String::from_utf8_lossy(&release_output.stdout);
        eprintln!("Release WASM build failed. Stdout: {stdout}");
        eprintln!("Release WASM build failed. Stderr: {stderr}");
    }
    assert!(release_output.status.success());
    assert!(temp_dir.path().join("tinysearch_engine.wasm").exists());
    assert!(temp_dir.path().join("tinysearch_engine.js").exists());
    assert!(temp_dir.path().join("tinysearch.root").exists());
    assert_eq!(
        std::fs::read(&retained_shard).expect("Release removed the retained shard"),
        b"previous immutable shard"
    );
    assert!(
        !temp_dir.path().join("demo.html").exists(),
        "Release output must omit the demo"
    );
    assert!(
        std::fs::read_dir(temp_dir)
            .expect("Failed to inspect release shards")
            .filter_map(std::result::Result::ok)
            .any(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    == Some("tinysearch-shard")
            }),
        "Release output must retain sharded artifacts"
    );
}

fn assert_xor8_wasm_output(generated_crate: &std::path::Path, current_dir: &std::path::Path) {
    let output_dir = TempDir::new().expect("Failed to create Xor8 output directory");
    let output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "--release",
            "--indexer",
            "xor8",
            "-m",
            "wasm",
            "--crate-path",
            generated_crate.to_str().unwrap(),
            "-p",
            output_dir.path().to_str().unwrap(),
            "--engine-version",
            &format!("path=\"{}\"", current_dir.display()),
            "fixtures/index.json",
        ])
        .output()
        .expect("Failed to generate Xor8 WASM output");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("Xor8 WASM build failed. Stdout: {stdout}");
        eprintln!("Xor8 WASM build failed. Stderr: {stderr}");
    }
    assert!(output.status.success());

    let loader_test = Command::new("node")
        .args([
            "tests/legacy_wasm_loader_test.mjs",
            output_dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute Xor8 loader integration test");
    if !loader_test.status.success() {
        let stderr = String::from_utf8_lossy(&loader_test.stderr);
        let stdout = String::from_utf8_lossy(&loader_test.stdout);
        eprintln!("Xor8 loader test failed. Stdout: {stdout}");
        eprintln!("Xor8 loader test failed. Stderr: {stderr}");
    }
    assert!(loader_test.status.success());
}

#[test]
fn test_cli_exact_crate_non_top_level_keeps_embedded_search_api() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let current_dir = std::env::current_dir().expect("Failed to resolve repository directory");
    let output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "crate",
            "--indexer",
            "exact",
            "--non-top-level-crate",
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
        .expect("Failed to generate exact embedded crate");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("Exact crate generation failed. Stdout: {stdout}");
        eprintln!("Exact crate generation failed. Stderr: {stderr}");
    }
    assert!(output.status.success());

    let storage_path = temp_dir.path().join("src/storage");
    assert!(storage_path.exists(), "Exact crate must embed src/storage");
    assert!(
        !temp_dir.path().join("index").exists(),
        "Crate mode must not emit external sharded artifacts"
    );
    let library = std::fs::read_to_string(temp_dir.path().join("src/lib.rs"))
        .expect("Failed to read generated exact crate library");
    assert!(library.contains("pub fn search_local"));
    assert!(library.contains("include_bytes!(\"storage\")"));
    assert!(!library.contains("ShardedIndex"));

    let cargo_toml = std::fs::read_to_string(temp_dir.path().join("Cargo.toml"))
        .expect("Failed to read generated Cargo.toml");
    assert!(!cargo_toml.contains("[workspace]"));
    assert!(cargo_toml.contains("[lib]"));

    let smoke_bin_dir = temp_dir.path().join("src/bin");
    std::fs::create_dir_all(&smoke_bin_dir).expect("Failed to create smoke binary directory");
    std::fs::write(
        smoke_bin_dir.join("search_smoke.rs"),
        r#"fn main() {
    let results = tinysearch_engine::search_local("decades".to_owned(), 5);
    assert!(!results.is_empty());
}
"#,
    )
    .expect("Failed to write search_local smoke binary");

    let smoke_output = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            temp_dir.path().join("Cargo.toml").to_str().unwrap(),
            "--bin",
            "search_smoke",
        ])
        .output()
        .expect("Failed to compile and run generated exact crate");
    if !smoke_output.status.success() {
        let stderr = String::from_utf8_lossy(&smoke_output.stderr);
        let stdout = String::from_utf8_lossy(&smoke_output.stdout);
        eprintln!("Generated exact crate smoke test failed. Stdout: {stdout}");
        eprintln!("Generated exact crate smoke test failed. Stderr: {stderr}");
    }
    assert!(smoke_output.status.success());
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
        eprintln!("Command failed: {stderr}");
    }

    assert!(output.status.success());

    // Check that storage file was created
    let storage_path = temp_dir.path().join("storage");
    assert!(storage_path.exists(), "Storage file should be created");

    let xor_dir = temp_dir.path().join("xor8");
    let xor_output = Command::new("cargo")
        .args([
            "run",
            "--features=bin",
            "--",
            "-m",
            "storage",
            "--indexer",
            "xor8",
            "-p",
            xor_dir.to_str().unwrap(),
            "fixtures/index.json",
        ])
        .output()
        .expect("Failed to build Xor8 storage");
    assert!(xor_output.status.success());

    let xor_bytes = std::fs::read(xor_dir.join("storage")).expect("Failed to read Xor8 storage");
    let xor_index = tinysearch::Storage::from_bytes(&xor_bytes)
        .expect("Failed to decode Xor8 storage")
        .filters;
    assert!(!tinysearch::search(&xor_index, "decades", 5).is_empty());
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
        eprintln!("Custom schema build failed. Stdout: {stdout}");
        eprintln!("Stderr: {stderr}");
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
        eprintln!("Flexible fields build failed. Stdout: {stdout}");
        eprintln!("Stderr: {stderr}");
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
            "waterp",
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
        search_stdout.contains("Bluetooth Speaker"),
        "Should find a body-only prefix match"
    );
    assert!(
        search_stdout.contains("https://store.example.com/speaker"),
        "Should contain the product URL"
    );
}
