use anyhow::Error;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path;

use super::assets::STOP_WORDS;
use super::index::Posts;
use strip_markdown::strip_markdown;
use tinysearch::{Filters, PostId, Storage, SearchSchema};
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

// prepares the files in the given directory to be consumed by the generator
pub fn prepare_posts(posts: Posts, schema: &SearchSchema) -> HashMap<PostId, Option<String>> {
    posts
        .into_iter()
        .inspect(|post| debug!("Analyzing {}", post.url))
        .filter_map(|post| {
            // Extract values from post fields based on schema
            let mut indexed_content = String::new();
            let mut metadata_content = String::new();
            let url_value;
            
            // For the current JSON structure, we need to handle the mapping
            // TODO: In the future, this should parse arbitrary JSON based on schema fields
            
            // Handle indexed fields
            for field in &schema.indexed_fields {
                match field.as_str() {
                    "title" => {
                        indexed_content.push_str(&post.title);
                        indexed_content.push(' ');
                    }
                    "body" => {
                        if let Some(body) = &post.body {
                            indexed_content.push_str(body);
                            indexed_content.push(' ');
                        }
                    }
                    _ => {
                        // For now, ignore unknown fields
                        debug!("Skipping unknown indexed field: {}", field);
                    }
                }
            }
            
            // Handle metadata fields  
            for field in &schema.metadata_fields {
                match field.as_str() {
                    "title" => {
                        metadata_content.push_str(&post.title);
                        metadata_content.push(' ');
                    }
                    "body" => {
                        if let Some(body) = &post.body {
                            metadata_content.push_str(body);
                            metadata_content.push(' ');
                        }
                    }
                    "meta" => {
                        if let Some(meta) = &post.meta {
                            metadata_content.push_str(meta);
                            metadata_content.push(' ');
                        }
                    }
                    _ => {
                        debug!("Skipping unknown metadata field: {}", field);
                    }
                }
            }
            
            // Handle URL field
            url_value = match schema.url_field.as_str() {
                "url" => post.url.clone(),
                "title" => post.title.clone(),
                _ => {
                    debug!("Unknown URL field: {}, using post.url", schema.url_field);
                    post.url.clone()
                }
            };
            
            // Create PostId with title, URL, and metadata
            let post_id = (
                post.title, 
                url_value, 
                if metadata_content.trim().is_empty() { None } else { Some(metadata_content.trim().to_string()) }
            );
            
            Some((post_id, if indexed_content.trim().is_empty() { None } else { Some(indexed_content.trim().to_string()) }))
        })
        .collect()
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
        
        let posts = vec![
            Post {
                title: "Test Title".to_string(),
                url: "https://example.com".to_string(),
                meta: Some("test metadata".to_string()),
                body: Some("Test body content".to_string()),
            }
        ];
        
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
}
