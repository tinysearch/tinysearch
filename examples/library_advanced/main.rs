use tinysearch::{Post, TinySearch};

/// Example of implementing the Post trait on your own custom type
#[derive(Debug)]
struct BlogPost {
    title: String,
    slug: String,
    content: String,
    tags: Vec<String>,
    author: String,
}

impl Post for BlogPost {
    fn title(&self) -> &str {
        &self.title
    }

    fn url(&self) -> &str {
        &self.slug
    }

    fn body(&self) -> Option<&str> {
        Some(&self.content)
    }

    fn meta(&self) -> Option<&str> {
        // Include author and first tag in searchable metadata
        if self.tags.is_empty() {
            Some(&self.author)
        } else {
            Some(&self.tags[0])
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Custom Post trait implementation example\n");

    // Create blog posts using your own struct
    let blog_posts = vec![
        BlogPost {
            title: "Getting Started with Rust".to_string(),
            slug: "/blog/rust-getting-started".to_string(),
            content: "Rust is a systems programming language focused on safety and performance"
                .to_string(),
            tags: vec!["rust".to_string(), "programming".to_string()],
            author: "Alice".to_string(),
        },
        BlogPost {
            title: "WebAssembly Performance Tips".to_string(),
            slug: "/blog/wasm-performance".to_string(),
            content: "Optimizing WebAssembly modules for better performance in browsers"
                .to_string(),
            tags: vec!["wasm".to_string(), "performance".to_string()],
            author: "Bob".to_string(),
        },
        BlogPost {
            title: "Building Search Engines".to_string(),
            slug: "/blog/search-engines".to_string(),
            content: "How to build efficient search engines using modern techniques".to_string(),
            tags: vec!["search".to_string(), "algorithms".to_string()],
            author: "Alice".to_string(),
        },
    ];

    // Create search engine with custom stopwords
    let search = TinySearch::new().with_stopwords(vec!["the".to_string(), "with".to_string()]);

    // Build index from custom post types
    println!("Building index from {} blog posts...", blog_posts.len());
    let index = search.build_index(blog_posts)?;
    println!("Index built with {} filters\n", index.len());

    // Search examples
    let queries = vec!["rust", "alice", "performance", "wasm"];

    for query in queries {
        println!("Searching for: '{}'", query);
        let results = search.search(&index, query, 3);

        if results.is_empty() {
            println!("  No results found");
        } else {
            for result in results {
                println!("  - {} ({})", result.0, result.1);
                if let Some(meta) = &result.2 {
                    println!("    Meta: {}", meta);
                }
            }
        }
        println!();
    }

    println!("Custom Post implementation example completed!");
    Ok(())
}
