# Using tinysearch as a Library

tinysearch can now be used as a library for programmatic search index generation and searching, addressing [issue #183](https://github.com/tinysearch/tinysearch/issues/183).

The library uses a flexible trait-based approach where you can either use the built-in `BasicPost` struct or implement the `Post` trait on your own types.

## Quick Start

Add tinysearch to your `Cargo.toml`:

```toml
[dependencies]
tinysearch = "0.9.0"
```

## Basic Usage

```rust
use tinysearch::{BasicPost, TinySearch};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a search engine instance
    let search = TinySearch::new();
    
    // Create posts using BasicPost
    let posts = vec![
        BasicPost {
            title: "First Post".to_string(),
            url: "/first".to_string(),
            body: Some("This is the first post content".to_string()),
            meta: None,
        },
        BasicPost {
            title: "Second Post".to_string(),
            url: "/second".to_string(),
            body: Some("This is about Rust programming".to_string()),
            meta: None,
        },
    ];

    // Build search index
    let index = search.build_index(posts)?;

    // Search
    let results = search.search(&index, "rust", 10);
    
    for result in results {
        println!("Title: {}, URL: {}", result.0, result.1);
    }
    
    Ok(())
}
```

## Using Custom Post Types

You can implement the `Post` trait on your own structs:

```rust
use tinysearch::{Post, TinySearch};

#[derive(Debug)]
struct MyPost {
    page_title: String,
    page_url: String,
    content: Option<String>,
    tags: Vec<String>,
}

impl Post for MyPost {
    fn title(&self) -> &str {
        &self.page_title
    }

    fn url(&self) -> &str {
        &self.page_url
    }

    fn body(&self) -> Option<&str> {
        self.content.as_deref()
    }

    fn meta(&self) -> Option<&str> {
        // You can customize how metadata is handled
        if self.tags.is_empty() {
            None
        } else {
            Some(&self.page_title) // Or serialize tags, etc.
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let search = TinySearch::new();
    
    let posts = vec![
        MyPost {
            page_title: "Custom Post".to_string(),
            page_url: "/custom".to_string(),
            content: Some("This uses a custom post type".to_string()),
            tags: vec!["custom".to_string(), "demo".to_string()],
        }
    ];

    let index = search.build_index(posts)?;
    let results = search.search(&index, "custom", 5);
    
    Ok(())
}
```

## Configuration with Builder Pattern

TinySearch supports configuration through a builder pattern:

```rust
use tinysearch::TinySearch;

let search = TinySearch::new()
    .with_stopwords(vec!["the".to_string(), "and".to_string()]);

// Now 'the' and 'and' will be excluded from indexing
let index = search.build_index(posts)?;
```

## API Reference

### Core Types

- **`Post`** - Trait that types must implement to be used as posts in tinysearch
- **`BasicPost`** - Basic implementation of the Post trait for simple use cases  
- **`TinySearch`** - Main API struct for configuring and performing search operations
- **`Filters`** - The search index containing all post filters
- **`PostId`** - A tuple of `(title, url, metadata)`

### TinySearch Methods

#### `TinySearch::new() -> Self`

Create a new TinySearch instance with default settings.

#### `with_stopwords<I>(self, stopwords: I) -> Self`

Configure custom stopwords to filter out during indexing (builder pattern).

#### `parse_posts_from_json(&self, json_str: &str) -> Result<Vec<BasicPost>, serde_json::Error>`

Parse a JSON string into a vector of BasicPost. The JSON format should match tinysearch's expected structure:

```rust
let json = r#"[
  {
    "title": "My Post",
    "url": "/my-post",
    "body": "Post content goes here",
    "meta": "optional metadata"
  }
]"#;

let search = TinySearch::new();
let posts = search.parse_posts_from_json(json)?;
```

#### `build_index<P: Post>(&self, posts: Vec<P>) -> Result<Filters, Box<dyn std::error::Error>>`

Build a search index from a collection of posts implementing the Post trait. This handles:
- Text tokenization and cleanup
- Stop word removal (default or custom)
- Xor filter generation for fast lookups

#### `search<'a>(&self, filters: &'a Filters, query: &str, num_results: usize) -> Vec<&'a PostId>`

Search the index with a query string. Returns results sorted by relevance score.

#### `build_and_serialize_index<P: Post>(&self, posts: Vec<P>) -> Result<Vec<u8>, Box<dyn std::error::Error>>`

Convenience method that builds an index and serializes it to bytes for storage.

#### `load_index_from_bytes(&self, bytes: &[u8]) -> Result<Filters, BincodeError>`

Load a previously serialized search index from bytes.

## Integration with Static Site Generators

### Zola Integration Example

```rust
use std::fs;
use tinysearch::TinySearch;

fn generate_search_index() -> Result<(), Box<dyn std::error::Error>> {
    let search = TinySearch::new();
    
    // Read your Zola-generated JSON
    let json_content = fs::read_to_string("public/tinysearch.json/index.html")?;
    
    // Parse posts
    let posts = search.parse_posts_from_json(&json_content)?;
    
    // Build and serialize index
    let index_bytes = search.build_and_serialize_index(posts)?;
    
    // Save to file that your frontend can load
    fs::write("public/search_index.bin", index_bytes)?;
    
    Ok(())
}
```

### Build Script Integration

You can integrate this into a build script (`build.rs`):

```rust
// build.rs
use tinysearch::TinySearch;
use std::fs;

fn main() {
    // Check if posts JSON exists
    if let Ok(json_content) = fs::read_to_string("posts.json") {
        let search = TinySearch::new();
        let posts = search.parse_posts_from_json(&json_content).unwrap();
        let index_bytes = search.build_and_serialize_index(posts).unwrap();
        
        // Write to output directory
        fs::write("out/search_index.bin", index_bytes).unwrap();
        
        println!("cargo:rerun-if-changed=posts.json");
    }
}
```

## Performance Considerations

- **Index Generation**: Building an index is CPU-intensive and should be done at build time, not runtime
- **Memory Usage**: Expect approximately ~2kB uncompressed per article (~1kB compressed)
- **Search Performance**: Searches are very fast (microseconds) once the index is loaded
- **Index Size**: The serialized index size scales linearly with the number of posts

## Error Handling

The library functions return proper `Result` types:

```rust
match build_index(posts) {
    Ok(index) => {
        // Use the index
        let results = search_with_index(&index, query, 10);
    }
    Err(e) => {
        eprintln!("Failed to build index: {}", e);
    }
}
```

## Compared to CLI Usage

| Approach | Use Case | Pros | Cons |
|----------|----------|------|------|
| **CLI** | Build pipelines, existing workflows | No code changes needed | Less flexible, subprocess overhead |
| **Library** | Custom integrations, build scripts | Full control, better error handling | Requires Rust code |

## Examples

See the [`examples/`](examples/) directory for complete working examples:

- [`library_basic/`](examples/library_basic/) - Basic API usage with BasicPost
- [`library_advanced/`](examples/library_advanced/) - Advanced usage with custom Post trait implementation
- More examples coming soon for specific static site generators

## Migration from CLI

If you're currently using the CLI tool and want to switch to the library:

**Before (CLI):**
```bash
tinysearch --mode wasm --path output posts.json
```

**After (Library in build.rs):**
```rust
let search = TinySearch::new();
let posts = search.parse_posts_from_json(&fs::read_to_string("posts.json")?)?;
let index_bytes = search.build_and_serialize_index(posts)?;
fs::write("output/search_index.bin", index_bytes)?;
```

**Benefits of the new approach:**
- **Type safety**: Full Rust type checking and IDE support
- **Flexibility**: Use your own post types by implementing the `Post` trait
- **Configuration**: Customize stopwords and other behavior
- **Better error handling**: Proper Rust `Result` types instead of exit codes  
- **Performance**: No subprocess overhead
- **Integration**: Works seamlessly in build scripts and custom tools

This gives you the same functionality with much more control and better integration into Rust-based build systems.