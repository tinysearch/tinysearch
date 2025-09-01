//! Example showing how to use the `SearchIndex` type to store and work with
//! search indexes without needing to import the xorf library directly.

#![allow(clippy::print_stdout, clippy::missing_docs_in_private_items)]

use std::collections::HashMap;
use tinysearch::{BasicPost, SearchIndex, TinySearch};

/// Example showing how to use the `SearchIndex` type to store and work with
/// search indexes without needing to import the xorf library directly.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("SearchIndex type example\n");

    // Create some posts
    let mut meta = HashMap::new();
    meta.insert("category".to_string(), "tutorial".to_string());

    let posts = vec![
        BasicPost {
            title: "Getting Started with Rust".to_string(),
            url: "/rust-tutorial".to_string(),
            body: Some("Learn Rust programming language basics".to_string()),
            meta: meta.clone(),
        },
        BasicPost {
            title: "Advanced Rust Concepts".to_string(),
            url: "/rust-advanced".to_string(),
            body: Some("Deep dive into advanced Rust features".to_string()),
            meta,
        },
    ];

    let search = TinySearch::new();

    // Build index with explicit type annotation
    let search_index: SearchIndex = search.build_index(&posts)?;

    println!("Built search index with {} entries", search_index.len());

    // Store the index (could be in a struct field, etc.)
    let stored_index = search_index;

    // Use the stored index for searching
    let results = search.search(&stored_index, "rust programming", 10);

    println!("Search results for 'rust programming':");
    for result in results {
        println!("  - {}: {}", result.title, result.url);
        if !result.meta.is_empty() {
            println!("    Meta: {}", result.meta);
        }
    }

    // Demonstrate serialization/deserialization with SearchIndex
    println!("\nTesting serialization...");
    let index_bytes = search.build_and_serialize_index(&posts)?;
    let loaded_index: SearchIndex = search.load_index_from_bytes(&index_bytes)?;

    let results = search.search(&loaded_index, "advanced", 5);
    println!("Results from deserialized index:");
    for result in results {
        println!("  - {}: {}", result.title, result.url);
    }

    println!("\nSearchIndex type example completed!");
    Ok(())
}
