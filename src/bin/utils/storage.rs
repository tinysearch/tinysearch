use anyhow::Error;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path;

use super::assets::STOP_WORDS;
use super::index::Posts;
use strip_markdown::strip_markdown;
use tinysearch::{Filters, PostId, SearchSchema, Storage};
use xorf::HashProxy;

pub fn write(posts: Posts, path: &path::PathBuf, schema: &SearchSchema) -> Result<(), Error> {
    let filters = build(posts, schema)?;
    trace!("Storage::from");
    let storage = Storage::from(filters);
    trace!("Write");
    fs::write(path, storage.to_bytes()?)?;
    trace!("ok");
    Ok(())
}

fn build(posts: Posts, schema: &SearchSchema) -> Result<Filters, Error> {
    let posts = prepare_posts(posts, schema);
    generate_filters(posts)
}

/// Remove non-ascii characters from string
/// Keep apostrophe (e.g. for words like "don't")
fn cleanup(s: String) -> String {
    s.replace(|c: char| !(c.is_alphabetic() || c == '\''), " ")
}

fn tokenize(words: &str, stopwords: &HashSet<String>) -> HashSet<String> {
    cleanup(strip_markdown(words))
        .split_whitespace()
        .filter(|&word| !word.trim().is_empty())
        .map(str::to_lowercase)
        .filter(|word| !stopwords.contains(word))
        .collect()
}

// Read all posts and generate Bloomfilters from them.
#[unsafe(no_mangle)]
pub fn generate_filters(posts: HashMap<PostId, Option<String>>) -> Result<Filters, Error> {
    // Create a dictionary of {"post name": "lowercase word set"}. split_posts =
    // {name: set(re.split("\W+", contents.lower())) for name, contents in
    // posts.items()}
    debug!("Generate filters");

    let stopwords: HashSet<String> = STOP_WORDS.split_whitespace().map(String::from).collect();

    let split_posts: HashMap<PostId, Option<HashSet<String>>> = posts
        .into_iter()
        .map(|(post, content)| {
            debug!("Generating {:?}", post);
            (post, content.map(|content| tokenize(&content, &stopwords)))
        })
        .collect();

    // At this point, we have a dictionary of posts and a normalized set of
    // words in each. We could do more things, like stemming, removing common
    // words (a, the, etc), but we're going for naive, so let's just create the
    // filters for now:
    let filters = split_posts
        .into_iter()
        .map(|(post_id, body)| {
            // Also add title to filter
            let title: HashSet<String> = tokenize(&post_id.0, &stopwords);
            let content: Vec<String> = body.map_or_else(
                || title.clone().into_iter().collect(),
                |body| body.union(&title).cloned().collect(),
            );
            let filter = HashProxy::from(&content);
            (post_id, filter)
        })
        .collect();
    trace!("Done");
    Ok(filters)
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
                    debug!("Field '{}' not found in post for indexing", field);
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
                    debug!("Field '{}' not found in post for metadata", field);
                }
            }

            // Handle URL field
            let url_value = if let Some(value) = post.fields.get(&schema.url_field) {
                extract_string_value(value)
            } else {
                debug!(
                    "URL field '{}' not found in post, using empty string",
                    schema.url_field
                );
                String::new()
            };

            // Extract title for PostId - use first indexed field as title or URL field as fallback
            let title = if let Some(title_field) = schema.indexed_fields.first() {
                if let Some(value) = post.fields.get(title_field) {
                    extract_string_value(value)
                } else {
                    url_value.clone()
                }
            } else {
                url_value.clone()
            };

            // Create PostId with title, URL, and metadata
            let post_id = (
                title,
                url_value,
                if metadata_content.trim().is_empty() {
                    None
                } else {
                    Some(metadata_content.trim().to_string())
                },
            );

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
mod tests {
    use xorf::Filter;

    use super::*;

    #[test]
    fn test_generate_filters() {
        let mut posts = HashMap::new();
        posts.insert(
            (
                "Maybe You Don't Need Kubernetes, Or Excel - You Know".to_string(), //title
                "".to_string(),                                                     //url
                None,                                                               //meta
            ),
            None, //body
        );
        let filters = generate_filters(posts).unwrap();
        assert_eq!(filters.len(), 1);
        let (_post_id, filter) = filters.first().unwrap();

        assert!(!filter.contains(&" ".to_owned()));
        assert!(!filter.contains(&"    ".to_owned()));
        assert!(!filter.contains(&"foo".to_owned()));
        assert!(!filter.contains(&"-".to_owned()));
        assert!(!filter.contains(&",".to_owned()));
        assert!(!filter.contains(&"'".to_owned()));

        // "you", "don't", and "need" get stripped out because they are stopwords
        assert!(!filter.contains(&"you".to_owned()));
        assert!(!filter.contains(&"don't".to_owned()));
        assert!(!filter.contains(&"need".to_owned()));

        assert!(filter.contains(&"maybe".to_owned()));
        assert!(filter.contains(&"kubernetes".to_owned()));
        assert!(filter.contains(&"excel".to_owned()));
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

        assert_eq!(post_id.0, "Test Title");
        assert_eq!(post_id.1, "https://example.com");
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
        assert_eq!(post_id.0, "Gaming Laptop"); // Title should be first indexed field
        assert_eq!(post_id.1, "https://example.com/laptop"); // URL from product_url field
        assert!(post_id.2.is_some()); // Should have metadata
        let metadata = post_id.2.as_ref().unwrap();
        assert!(metadata.contains("$1999.99"));
        assert!(metadata.contains("TechCorp"));

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
