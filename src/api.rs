//! Public API for tinysearch library
//!
//! This module contains the main public API types and functions for using tinysearch
//! as a library. The API is designed around the [`Post`] trait and [`TinySearch`] struct
//! which provide flexible and ergonomic access to search index generation and querying.

use bincode::Error as BincodeError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::From;
use strip_markdown::strip_markdown;
use xorf::{HashProxy, Xor8};

use crate::{Filters, PostId, Storage};

/// Trait that types must implement to be used as posts in tinysearch
///
/// This trait allows users to use their own post types without needing to convert
/// to a specific struct, as long as they can provide the required fields through
/// these methods.
///
/// # Example
///
/// ```rust
/// use tinysearch::Post;
/// use std::collections::HashMap;
///
/// #[derive(Debug)]
/// struct BlogPost {
///     title: String,
///     permalink: String,
///     content: String,
///     author: String,
/// }
///
/// impl Post for BlogPost {
///     fn title(&self) -> &str {
///         &self.title
///     }
///
///     fn url(&self) -> &str {
///         &self.permalink
///     }
///
///     fn body(&self) -> Option<&str> {
///         Some(&self.content)
///     }
///
///     fn meta(&self) -> HashMap<String, String> {
///         let mut meta = HashMap::new();
///         meta.insert("author".to_string(), self.author.clone());
///         meta
///     }
/// }
/// ```
pub trait Post {
    /// Get the post title
    ///
    /// The title is used both for display in search results and as part of the
    /// searchable content. Title matches are weighted higher than body matches.
    fn title(&self) -> &str;

    /// Get the post URL or identifier
    ///
    /// This should be a unique identifier for the post, typically a URL path
    /// or permalink that can be used to navigate to the post.
    fn url(&self) -> &str;

    /// Get the post body content, if any
    ///
    /// The body content is tokenized and indexed for full-text search.
    /// Return `None` if the post has no body content (e.g., for title-only posts).
    fn body(&self) -> Option<&str>;

    /// Get metadata for the post as key-value pairs
    ///
    /// Metadata is also indexed and searchable, useful for things like author names,
    /// tags, categories, or other structured data you want to be findable.
    /// Return an empty HashMap if no metadata should be indexed.
    fn meta(&self) -> HashMap<String, String>;
}

/// Basic implementation of the [`Post`] trait for simple use cases
///
/// This struct provides a straightforward way to create posts without needing
/// to implement the [`Post`] trait yourself. All fields are public for easy construction.
///
/// # Example
///
/// ```rust
/// use tinysearch::BasicPost;
/// use std::collections::HashMap;
///
/// let mut meta = HashMap::new();
/// meta.insert("category".to_string(), "programming".to_string());
/// meta.insert("author".to_string(), "John Doe".to_string());
///
/// let post = BasicPost {
///     title: "My First Post".to_string(),
///     url: "/posts/my-first-post".to_string(),
///     body: Some("This is the content of my post".to_string()),
///     meta,
/// };
/// ```
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BasicPost {
    /// Post title
    pub title: String,
    /// Post URL or permalink
    pub url: String,
    /// Optional post body content
    pub body: Option<String>,
    /// Metadata as key-value pairs (e.g., author, category, tags)
    #[serde(default)]
    pub meta: HashMap<String, String>,
}

impl Post for BasicPost {
    fn title(&self) -> &str {
        &self.title
    }

    fn url(&self) -> &str {
        &self.url
    }

    fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    fn meta(&self) -> HashMap<String, String> {
        self.meta.clone()
    }
}

/// Main API struct for tinysearch operations
///
/// This struct provides the primary interface for building search indexes and
/// performing searches. It supports a builder pattern for configuration and
/// provides methods for common operations like JSON parsing and serialization.
///
/// # Example
///
/// ```rust
/// use tinysearch::{BasicPost, TinySearch};
/// use std::collections::HashMap;
///
/// // Create posts
/// let posts = vec![
///     BasicPost {
///         title: "First Post".to_string(),
///         url: "/first".to_string(),
///         body: Some("Content about Rust programming".to_string()),
///         meta: HashMap::new(),
///     }
/// ];
///
/// // Build and search index
/// let search = TinySearch::new();
/// let index = search.build_index(&posts).unwrap();
/// let results = search.search(&index, "rust", 10);
/// ```
#[derive(Debug, Clone)]
pub struct TinySearch {
    /// Custom stopwords to use instead of built-in ones
    custom_stopwords: Option<HashSet<String>>,
}

impl TinySearch {
    /// Create a new TinySearch instance with default settings
    ///
    /// The default configuration uses the built-in English stopwords list.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tinysearch::TinySearch;
    ///
    /// let search = TinySearch::new();
    /// ```
    pub fn new() -> Self {
        Self {
            custom_stopwords: None,
        }
    }

    /// Configure custom stopwords to filter out during indexing (builder pattern)
    ///
    /// Stopwords are common words that are typically filtered out during indexing
    /// to improve search quality and reduce index size. By default, tinysearch uses
    /// a built-in English stopwords list.
    ///
    /// # Arguments
    /// * `stopwords` - Collection of words to exclude from the index
    ///
    /// # Example
    ///
    /// ```rust
    /// use tinysearch::TinySearch;
    ///
    /// let search = TinySearch::new()
    ///     .with_stopwords(vec!["the".to_string(), "and".to_string(), "or".to_string()]);
    /// ```
    pub fn with_stopwords<I>(mut self, stopwords: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        self.custom_stopwords = Some(stopwords.into_iter().collect());
        self
    }

    /// Parse JSON string containing posts into a Vec<BasicPost>
    ///
    /// This method parses JSON in the format expected by tinysearch, where each
    /// post is an object with `title`, `url`, and optionally `body` and `meta` fields.
    ///
    /// # Arguments
    /// * `json_str` - JSON string containing an array of post objects
    ///
    /// # Returns
    /// * `Ok(Vec<BasicPost>)` - Successfully parsed posts
    /// * `Err(serde_json::Error)` - JSON parsing error
    ///
    /// # Example
    ///
    /// ```rust
    /// use tinysearch::TinySearch;
    ///
    /// let json = r#"[
    ///   {
    ///     "title": "My Post",
    ///     "url": "/my-post",
    ///     "body": "Post content goes here",
    ///     "meta": {"category": "programming", "author": "John"}
    ///   }
    /// ]"#;
    ///
    /// let search = TinySearch::new();
    /// let posts = search.parse_posts_from_json(json).unwrap();
    /// ```
    pub fn parse_posts_from_json(
        &self,
        json_str: &str,
    ) -> Result<Vec<BasicPost>, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Build a search index from a collection of posts
    ///
    /// This method takes posts implementing the [`Post`] trait and generates the filters
    /// needed for fast search. It handles tokenization, stop word removal, and filter generation.
    ///
    /// The process involves:
    /// 1. Extracting text content from each post (title, body, meta)
    /// 2. Tokenizing and cleaning the text (lowercase, remove punctuation)
    /// 3. Filtering out stopwords
    /// 4. Creating Xor filters for efficient membership testing
    ///
    /// # Arguments
    /// * `posts` - Vector of posts implementing the [`Post`] trait
    ///
    /// # Returns
    /// * `Ok(Filters)` - Successfully generated search index
    /// * `Err(Box<dyn std::error::Error>)` - Index generation error
    ///
    /// # Example
    ///
    /// ```rust
    /// use tinysearch::{BasicPost, TinySearch};
    /// use std::collections::HashMap;
    ///
    /// let posts = vec![
    ///     BasicPost {
    ///         title: "Hello World".to_string(),
    ///         url: "/hello".to_string(),
    ///         body: Some("This is my first post".to_string()),
    ///         meta: HashMap::new(),
    ///     }
    /// ];
    ///
    /// let search = TinySearch::new();
    /// let filters = search.build_index(&posts).unwrap();
    /// ```
    pub fn build_index<P: Post>(&self, posts: &[P]) -> Result<Filters, Box<dyn std::error::Error>> {
        let prepared_posts = self.prepare_posts(posts);
        self.generate_filters(prepared_posts)
    }

    /// Search using a pre-built index
    ///
    /// This method performs a search query against a pre-built search index,
    /// returning results sorted by relevance score. Title matches are weighted
    /// higher than body matches to prioritize more relevant results.
    ///
    /// # Arguments
    /// * `filters` - Pre-built search index from [`build_index`](Self::build_index)
    /// * `query` - Search query string
    /// * `num_results` - Maximum number of results to return
    ///
    /// # Returns
    /// Vector of matching [`PostId`] references, sorted by relevance (highest first)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tinysearch::{BasicPost, TinySearch};
    /// use std::collections::HashMap;
    ///
    /// let posts = vec![
    ///     BasicPost {
    ///         title: "Rust Guide".to_string(),
    ///         url: "/rust".to_string(),
    ///         body: Some("Learn Rust programming".to_string()),
    ///         meta: HashMap::new(),
    ///     }
    /// ];
    /// let search = TinySearch::new();
    /// let index = search.build_index(&posts).unwrap();
    ///
    /// let results = search.search(&index, "rust programming", 5);
    /// for result in results {
    ///     println!("Found: {} at {}", result.title, result.url);
    /// }
    /// ```
    pub fn search<'a>(
        &self,
        filters: &'a Filters,
        query: &str,
        num_results: usize,
    ) -> Vec<&'a PostId> {
        crate::search(filters, query.to_string(), num_results)
    }

    /// Build a search index and serialize it to bytes
    ///
    /// This is a convenience method that combines index building and serialization
    /// for easy storage to files or databases. The serialized format uses bincode
    /// for efficient binary encoding.
    ///
    /// # Arguments
    /// * `posts` - Vector of posts implementing the [`Post`] trait
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - Serialized index as bytes
    /// * `Err(Box<dyn std::error::Error>)` - Index generation or serialization error
    ///
    /// # Example
    ///
    /// ```rust
    /// use tinysearch::{BasicPost, TinySearch};
    /// use std::collections::HashMap;
    ///
    /// let posts = vec![
    ///     BasicPost {
    ///         title: "My Post".to_string(),
    ///         url: "/post".to_string(),
    ///         body: Some("Post content".to_string()),
    ///         meta: HashMap::new(),
    ///     }
    /// ];
    /// let search = TinySearch::new();
    ///
    /// // Build and serialize index
    /// let index_bytes = search.build_and_serialize_index(&posts).unwrap();
    ///
    /// // You can save to file: std::fs::write("search_index.bin", index_bytes).unwrap();
    /// ```
    pub fn build_and_serialize_index<P: Post>(
        &self,
        posts: &[P],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let filters = self.build_index(posts)?;
        let storage = Storage::from(filters);
        storage.to_bytes().map_err(|e| e.into())
    }

    /// Load a search index from serialized bytes
    ///
    /// This method deserializes a previously saved search index from bytes.
    /// The index must have been created using [`build_and_serialize_index`](Self::build_and_serialize_index)
    /// or compatible serialization.
    ///
    /// # Arguments
    /// * `bytes` - Serialized index bytes
    ///
    /// # Returns
    /// * `Ok(Filters)` - Successfully loaded search index
    /// * `Err(BincodeError)` - Deserialization error
    ///
    /// # Example
    ///
    /// ```rust
    /// use tinysearch::{BasicPost, TinySearch};
    /// use std::collections::HashMap;
    ///
    /// let search = TinySearch::new();
    ///
    /// // First create and serialize an index
    /// let posts = vec![
    ///     BasicPost {
    ///         title: "Test".to_string(),
    ///         url: "/test".to_string(),
    ///         body: Some("content".to_string()),
    ///         meta: HashMap::new(),
    ///     }
    /// ];
    /// let index_bytes = search.build_and_serialize_index(&posts).unwrap();
    ///
    /// // Then load it back
    /// let index = search.load_index_from_bytes(&index_bytes).unwrap();
    /// let results = search.search(&index, "content", 10);
    /// ```
    pub fn load_index_from_bytes(&self, bytes: &[u8]) -> Result<Filters, BincodeError> {
        let storage = Storage::from_bytes(bytes)?;
        Ok(storage.filters)
    }
}

impl Default for TinySearch {
    fn default() -> Self {
        Self::new()
    }
}

impl TinySearch {
    /// Get the stopwords set to use for this instance
    fn get_stopwords(&self) -> HashSet<String> {
        self.custom_stopwords.clone().unwrap_or_else(|| {
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/stopwords"))
                .split_whitespace()
                .map(String::from)
                .collect()
        })
    }

    /// Remove non-ascii characters from string
    /// Keep apostrophe (e.g. for words like "don't")
    fn cleanup(&self, s: String) -> String {
        s.replace(|c: char| !(c.is_alphabetic() || c == '\''), " ")
    }

    fn tokenize_with_stopwords(&self, words: &str, stopwords: &HashSet<String>) -> HashSet<String> {
        self.cleanup(strip_markdown(words))
            .split_whitespace()
            .filter(|&word| !word.trim().is_empty())
            .map(str::to_lowercase)
            .filter(|word| !stopwords.contains(word))
            .collect()
    }

    /// Generate filters from prepared posts (internal implementation)
    fn generate_filters(
        &self,
        posts: HashMap<PostId, Option<String>>,
    ) -> Result<Filters, Box<dyn std::error::Error>> {
        let stopwords = self.get_stopwords();

        let split_posts: HashMap<PostId, Option<HashSet<String>>> = posts
            .into_iter()
            .map(|(post, content)| {
                (
                    post,
                    content.map(|content| self.tokenize_with_stopwords(&content, &stopwords)),
                )
            })
            .collect();

        let filters = split_posts
            .into_iter()
            .map(|(post_id, body)| {
                // Add title to filter
                let title: HashSet<String> = self.tokenize_with_stopwords(&post_id.title, &stopwords);
                
                // Add metadata to filter
                let metadata: HashSet<String> = if post_id.meta.is_empty() {
                    HashSet::new()
                } else {
                    self.tokenize_with_stopwords(&post_id.meta, &stopwords)
                };
                
                let mut content: HashSet<String> = title;
                content.extend(metadata);
                if let Some(body) = body {
                    content.extend(body);
                }
                
                let content_vec: Vec<String> = content.into_iter().collect();
                let filter =
                    HashProxy::<String, std::collections::hash_map::DefaultHasher, Xor8>::from(
                        &content_vec,
                    );
                (post_id, filter)
            })
            .collect();
        Ok(filters)
    }

    /// Prepare posts for filter generation (internal implementation)
    fn prepare_posts<P: Post>(&self, posts: &[P]) -> HashMap<PostId, Option<String>> {
        posts
            .iter()
            .map(|post| {
                let meta_str = if post.meta().is_empty() {
                    String::new()
                } else {
                    serde_json::to_string(&post.meta()).unwrap_or_default()
                };
                let post_id = PostId {
                    title: post.title().to_string(),
                    url: post.url().to_string(),
                    meta: meta_str,
                };
                let body = post.body().map(|s| s.to_string());
                (post_id, body)
            })
            .collect()
    }
}
