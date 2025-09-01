//! Basic library usage example demonstrating search functionality

#![allow(clippy::print_stdout, clippy::missing_docs_in_private_items)]

use std::collections::HashMap;
use tinysearch::{BasicPost, TinySearch};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing tinysearch library API...\n");

    // Example 1: Create posts manually using BasicPost
    let posts = vec![
        BasicPost {
            title: "Introduction to Rust".to_string(),
            url: "/rust-intro".to_string(),
            body: Some(
                "Rust is a systems programming language that is fast, safe, and concurrent"
                    .to_string(),
            ),
            meta: HashMap::new(),
        },
        BasicPost {
            title: "WebAssembly Tutorial".to_string(),
            url: "/wasm-tutorial".to_string(),
            body: Some(
                "WebAssembly (WASM) allows you to run code at near-native speed in web browsers"
                    .to_string(),
            ),
            meta: HashMap::new(),
        },
        BasicPost {
            title: "Building Search Engines".to_string(),
            url: "/search-engines".to_string(),
            body: Some(
                "Search engines use various algorithms to index and retrieve relevant documents"
                    .to_string(),
            ),
            meta: HashMap::new(),
        },
    ];

    // Create a TinySearch instance
    let tinysearch = TinySearch::new();

    // Build search index
    println!("Building search index from {} posts...", posts.len());
    let index = tinysearch.build_index(&posts)?;
    println!("Index built successfully with {} filters\n", index.len());

    // Search the index
    let queries = vec!["rust", "wasm", "search", "algorithms"];
    for query in queries {
        println!("Searching for: '{query}'");
        let results = tinysearch.search(&index, query, 5);
        for result in results {
            println!("  - {}: {}", result.title, result.url);
        }
        println!();
    }

    // Example 2: Parse from JSON
    let json_data = r#"[
        {
            "title": "JSON Parsing Example",
            "url": "/json-example",
            "body": "This post demonstrates JSON parsing functionality in tinysearch"
        }
    ]"#;

    println!("Testing JSON parsing...");
    let json_posts = tinysearch.parse_posts_from_json(json_data)?;
    println!("Parsed {} posts from JSON\n", json_posts.len());

    // Example 3: Serialize and deserialize index
    println!("Testing serialization...");
    let serialized = tinysearch.build_and_serialize_index(&json_posts)?;
    println!("Serialized index size: {} bytes", serialized.len());

    let deserialized_index = tinysearch.load_index_from_bytes(&serialized)?;
    println!(
        "Deserialized index with {} filters",
        deserialized_index.len()
    );

    let search_results = tinysearch.search(&deserialized_index, "json", 5);
    println!(
        "Search results for 'json': {} matches",
        search_results.len()
    );

    // Example 4: Using builder pattern with custom stopwords
    println!("\nTesting builder pattern with custom stopwords...");
    let tinysearch = TinySearch::new().with_stopwords(vec!["the".to_string(), "is".to_string()]);

    let test_posts = vec![BasicPost {
        title: "The Ultimate Guide".to_string(),
        url: "/ultimate-guide".to_string(),
        body: Some("This is the ultimate guide to everything".to_string()),
        meta: HashMap::new(),
    }];

    let custom_stopwords_index = tinysearch.build_index(&test_posts)?;
    let stopword_results = tinysearch.search(&custom_stopwords_index, "ultimate", 5);
    println!(
        "Results with custom stopwords: {} matches",
        stopword_results.len()
    );
    for result in stopword_results {
        println!("  - {}: {}", result.title, result.url);
    }

    println!("\nLibrary API test completed successfully!");
    Ok(())
}
