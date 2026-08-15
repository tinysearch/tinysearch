use anyhow::Error;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use super::assets::STOP_WORDS;
use super::index::Posts;
use strip_markdown::strip_markdown;
use tinysearch::{
    IndexKind, IndexedDocument, PostId, SearchIndex, SearchSchema, ShardConfig, Storage,
};

pub const ROOT_FILENAME: &str = "tinysearch.root";
pub const SHARD_FILE_SUFFIX: &str = ".tinysearch-shard";

pub struct ShardedArtifacts {
    pub root_bytes: usize,
    pub shard_files: Vec<String>,
    pub total_shard_bytes: usize,
    pub max_shard_bytes: usize,
}

pub fn write(index: SearchIndex, path: &Path) -> Result<(), Error> {
    trace!("Storage::from");
    let storage = Storage::from(index);
    trace!("Write");
    fs::write(path, storage.to_bytes()?)?;
    trace!("ok");
    Ok(())
}

pub fn write_sharded(
    index: &SearchIndex,
    directory: &Path,
    config: ShardConfig,
) -> Result<ShardedArtifacts, Error> {
    if directory.try_exists()? {
        fs::remove_dir_all(directory)?;
    }
    fs::create_dir_all(directory)?;

    let bundle = index.to_sharded_bundle(config)?;
    let (root_bytes, shards) = bundle.into_parts();
    let root_size = root_bytes.len();
    fs::write(directory.join(ROOT_FILENAME), root_bytes)?;

    let mut shard_files = Vec::with_capacity(shards.len());
    let mut total_shard_bytes = 0_usize;
    let mut max_shard_bytes = 0_usize;
    for artifact in shards {
        let (descriptor, bytes) = artifact.into_parts();
        let shard_size = bytes.len();
        fs::write(directory.join(&descriptor.filename), bytes)?;
        total_shard_bytes = total_shard_bytes.saturating_add(shard_size);
        max_shard_bytes = max_shard_bytes.max(shard_size);
        shard_files.push(descriptor.filename);
    }

    Ok(ShardedArtifacts {
        root_bytes: root_size,
        shard_files,
        total_shard_bytes,
        max_shard_bytes,
    })
}

pub fn build(posts: Posts, schema: &SearchSchema, index_kind: IndexKind) -> SearchIndex {
    let posts = prepare_posts(posts, schema);
    generate_index(posts, index_kind)
}

/// Replaces non-letter punctuation with spaces while preserving apostrophes.
fn cleanup(s: &str) -> String {
    s.replace(|c: char| !(c.is_alphabetic() || c == '\''), " ")
}

fn tokenize(words: &str, stopwords: &HashSet<String>) -> HashSet<String> {
    cleanup(&strip_markdown(words))
        .split_whitespace()
        .filter(|&word| !word.trim().is_empty())
        .map(str::to_lowercase)
        .filter(|word| !stopwords.contains(word))
        .collect()
}

// Read all posts and generate the selected index representation.
pub fn generate_index(
    posts: HashMap<PostId, Option<String>>,
    index_kind: IndexKind,
) -> SearchIndex {
    debug!("Generate index");

    let stopwords: HashSet<String> = STOP_WORDS.split_whitespace().map(String::from).collect();

    let split_posts: HashMap<PostId, Option<HashSet<String>>> = posts
        .into_iter()
        .map(|(post, content)| {
            debug!("Generating {post:?}");
            (post, content.map(|content| tokenize(&content, &stopwords)))
        })
        .collect();

    let mut documents: Vec<IndexedDocument> = split_posts
        .into_iter()
        .map(|(post_id, body)| {
            let mut content = tokenize(&post_id.title, &stopwords);
            if !post_id.meta.is_empty() {
                content.extend(tokenize(&post_id.meta, &stopwords));
            }
            if let Some(body) = body {
                content.extend(body);
            }
            IndexedDocument::new(post_id, content)
        })
        .collect();
    documents.sort_by(|left, right| {
        (&left.post.url, &left.post.title, &left.post.meta).cmp(&(
            &right.post.url,
            &right.post.title,
            &right.post.meta,
        ))
    });
    trace!("Done");
    index_kind.backend().build(documents)
}

// prepares posts with arbitrary field mappings based on schema
pub fn prepare_posts(posts: Posts, schema: &SearchSchema) -> HashMap<PostId, Option<String>> {
    posts
        .into_iter()
        .inspect(|post| {
            if let Some(url) = post.fields.get(&schema.url_field) {
                debug!("Analyzing {}", extract_string_value(url));
            }
        })
        .map(|post| {
            let mut indexed_content = String::new();
            let mut metadata_content = String::new();

            // Handle indexed fields
            for field in &schema.indexed_fields {
                if let Some(value) = post.fields.get(field) {
                    let field_content = extract_string_value(value);
                    if !field_content.is_empty() {
                        indexed_content.push_str(&field_content);
                        indexed_content.push(' ');
                    }
                } else {
                    debug!("Field '{field}' not found in post for indexing");
                }
            }

            // Handle metadata fields
            for field in &schema.metadata_fields {
                if let Some(value) = post.fields.get(field) {
                    let field_content = extract_string_value(value);
                    if !field_content.is_empty() {
                        metadata_content.push_str(&field_content);
                        metadata_content.push(' ');
                    }
                } else {
                    debug!("Field '{field}' not found in post for metadata");
                }
            }

            // Handle URL field
            let url_value = post.fields.get(&schema.url_field).map_or_else(
                || {
                    debug!(
                        "URL field '{}' not found in post, using empty string",
                        schema.url_field
                    );
                    String::new()
                },
                extract_string_value,
            );

            // Extract title for PostId - use first indexed field as title or URL field as fallback
            let title = if let Some(title_field) = schema.indexed_fields.first() {
                post.fields
                    .get(title_field)
                    .map_or_else(|| url_value.clone(), extract_string_value)
            } else {
                url_value.clone()
            };

            // Create PostId with title, URL, and metadata string
            let meta_str = if metadata_content.trim().is_empty() {
                String::new()
            } else {
                metadata_content.trim().to_string()
            };

            let post_id = PostId {
                title,
                url: url_value,
                meta: meta_str,
            };

            (
                post_id,
                if indexed_content.trim().is_empty() {
                    None
                } else {
                    Some(indexed_content.trim().to_string())
                },
            )
        })
        .collect()
}

// Helper function to extract string value from JSON value
fn extract_string_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_indexes() {
        let mut posts = HashMap::new();
        posts.insert(
            PostId {
                title: "Maybe You Don't Need Kubernetes, Or Excel - You Know".to_string(),
                url: String::new(),
                meta: String::new(),
            },
            Some("Observability requires instrumentation".to_string()),
        );
        let exact = generate_index(posts.clone(), IndexKind::Exact);
        assert_eq!(exact.len(), 1);
        assert!(tinysearch::search(&exact, "foo", 5).is_empty());
        assert_eq!(tinysearch::search(&exact, "obser", 5).len(), 1);
        assert_eq!(tinysearch::search(&exact, "excel", 5).len(), 1);

        let xor8 = generate_index(posts, IndexKind::Xor8);
        assert_eq!(xor8.len(), 1);
        assert_eq!(tinysearch::search(&xor8, "observability", 5).len(), 1);
    }

    #[test]
    fn test_prepare_posts_with_schema() {
        use super::super::index::Post;
        use std::collections::HashMap;

        let mut post_fields = HashMap::new();
        post_fields.insert(
            "title".to_string(),
            serde_json::Value::String("Test Title".to_string()),
        );
        post_fields.insert(
            "url".to_string(),
            serde_json::Value::String("https://example.com".to_string()),
        );
        post_fields.insert(
            "body".to_string(),
            serde_json::Value::String("Test body content".to_string()),
        );

        let posts = vec![Post {
            fields: post_fields,
        }];

        let schema = SearchSchema::default();
        let prepared = prepare_posts(posts, &schema);

        assert_eq!(prepared.len(), 1);
        let (post_id, body) = prepared.iter().next().unwrap();

        assert_eq!(post_id.title, "Test Title");
        assert_eq!(post_id.url, "https://example.com");
        assert!(body.is_some());
        assert!(body.as_ref().unwrap().contains("Test Title"));
        assert!(body.as_ref().unwrap().contains("Test body content"));
    }

    #[test]
    fn test_prepare_posts_custom_fields() {
        use super::super::index::Post;
        use std::collections::HashMap;

        let mut post_fields = HashMap::new();
        post_fields.insert(
            "product_name".to_string(),
            serde_json::Value::String("Gaming Laptop".to_string()),
        );
        post_fields.insert(
            "description".to_string(),
            serde_json::Value::String("High-performance gaming laptop".to_string()),
        );
        post_fields.insert(
            "product_url".to_string(),
            serde_json::Value::String("https://example.com/laptop".to_string()),
        );
        post_fields.insert(
            "price".to_string(),
            serde_json::Value::String("$1999.99".to_string()),
        );
        post_fields.insert(
            "brand".to_string(),
            serde_json::Value::String("TechCorp".to_string()),
        );

        let posts = vec![Post {
            fields: post_fields,
        }];

        let schema = SearchSchema {
            indexed_fields: vec!["product_name".to_string(), "description".to_string()],
            metadata_fields: vec!["price".to_string(), "brand".to_string()],
            url_field: "product_url".to_string(),
        };

        let prepared = prepare_posts(posts, &schema);

        assert_eq!(prepared.len(), 1);
        let (post_id, indexed_content) = prepared.iter().next().unwrap();

        // Check PostId structure
        assert_eq!(post_id.title, "Gaming Laptop"); // Title should be first indexed field
        assert_eq!(post_id.url, "https://example.com/laptop"); // URL from product_url field
        assert!(!post_id.meta.is_empty()); // Should have metadata
        assert!(post_id.meta.contains("$1999.99"));
        assert!(post_id.meta.contains("TechCorp"));

        // Check indexed content
        assert!(indexed_content.is_some());
        let content = indexed_content.as_ref().unwrap();
        assert!(content.contains("Gaming Laptop"));
        assert!(content.contains("High-performance gaming laptop"));
    }

    #[test]
    fn test_extract_string_value() {
        use serde_json::Value;

        assert_eq!(
            extract_string_value(&Value::String("test".to_string())),
            "test"
        );
        assert_eq!(
            extract_string_value(&Value::Number(serde_json::Number::from(42))),
            "42"
        );
        assert_eq!(extract_string_value(&Value::Bool(true)), "true");

        let array = Value::Array(vec![
            Value::String("hello".to_string()),
            Value::String("world".to_string()),
        ]);
        assert_eq!(extract_string_value(&array), "hello world");

        assert_eq!(extract_string_value(&Value::Null), "");
    }
}
