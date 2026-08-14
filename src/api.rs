//! Public API for tinysearch library
//!
//! This module contains the main public API types and functions for using tinysearch
//! as a library. The API is designed around the [`Post`] trait and [`TinySearch`] struct
//! which provide flexible and ergonomic access to search index generation and querying.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::From;
use strip_markdown::strip_markdown;

use crate::{IndexBackend, IndexKind, IndexedDocument, PostId, SearchIndex, Storage, StorageError};

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
    /// Return an empty `HashMap` if no metadata should be indexed.
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
    /// Custom stopwords to use instead of built-in ones.
    custom_stopwords: Option<HashSet<String>>,
    /// Built-in index backend used by [`build_index`](Self::build_index).
    index_kind: IndexKind,
}

impl TinySearch {
    /// Create a new `TinySearch` instance with default settings
    ///
    /// The default configuration uses the built-in English stopwords list and
    /// the exact inverted-index backend.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tinysearch::TinySearch;
    ///
    /// let search = TinySearch::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            custom_stopwords: None,
            index_kind: IndexKind::Exact,
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
    #[must_use]
    pub fn with_stopwords<I>(mut self, stopwords: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        self.custom_stopwords = Some(stopwords.into_iter().collect());
        self
    }

    /// Select the built-in index backend.
    ///
    /// Exact indexing is the default. [`IndexKind::Xor8`] produces a smaller
    /// probabilistic index with exact-word body and metadata matching.
    ///
    /// ```rust
    /// use tinysearch::{IndexKind, TinySearch};
    ///
    /// let search = TinySearch::new().with_index_kind(IndexKind::Xor8);
    /// ```
    #[must_use]
    pub const fn with_index_kind(mut self, index_kind: IndexKind) -> Self {
        self.index_kind = index_kind;
        self
    }

    /// Parse a JSON string containing posts into a `Vec<BasicPost>`.
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
    /// # Errors
    ///
    /// Returns an error if `json_str` is not a valid array of posts.
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
    /// This method takes posts implementing the [`Post`] trait and generates an
    /// index with the selected backend. It handles tokenization and stop word
    /// removal.
    ///
    /// The process involves:
    /// 1. Extracting text content from each post (title, body, meta)
    /// 2. Tokenizing and cleaning the text (lowercase, remove punctuation)
    /// 3. Filtering out stopwords
    /// 4. Building the selected exact or Xor8 representation
    ///
    /// # Arguments
    /// * `posts` - Vector of posts implementing the [`Post`] trait
    ///
    /// # Returns
    /// * `Ok(SearchIndex)` - Successfully generated search index
    /// * `Err(Box<dyn std::error::Error>)` - Index generation error
    ///
    /// # Errors
    ///
    /// The built-in backends are currently infallible. The `Result` is retained
    /// for API compatibility and future fallible indexing backends.
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
    /// let index = search.build_index(&posts).unwrap();
    /// ```
    pub fn build_index<P: Post>(
        &self,
        posts: &[P],
    ) -> Result<SearchIndex, Box<dyn std::error::Error>> {
        self.build_index_with(posts, self.index_kind.backend())
    }

    /// Build an index using a custom backend.
    ///
    /// # Errors
    ///
    /// Index preparation is currently infallible. The `Result` is retained for
    /// API compatibility and future fallible indexing backends.
    pub fn build_index_with<P: Post>(
        &self,
        posts: &[P],
        backend: &dyn IndexBackend,
    ) -> Result<SearchIndex, Box<dyn std::error::Error>> {
        let prepared_posts = Self::prepare_posts(posts);
        let stopwords = self.get_stopwords();
        let documents = Self::prepare_documents(prepared_posts, &stopwords);
        Ok(backend.build(documents))
    }

    /// Search using a pre-built index
    ///
    /// This method performs a search query against a pre-built search index,
    /// returning results sorted by relevance score. Exact title matches rank
    /// above prefix matches. Prefix matching in titles, body text, and metadata
    /// starts at three characters for exact indexes. Xor8 indexes retain
    /// exact-word body and metadata matching.
    ///
    /// # Arguments
    /// * `index` - Pre-built search index from [`build_index`](Self::build_index)
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
    #[must_use]
    pub fn search<'index>(
        &self,
        index: &'index SearchIndex,
        query: &str,
        num_results: usize,
    ) -> Vec<&'index PostId> {
        crate::search(index, query, num_results)
    }

    /// Serialize a built search index to bytes.
    ///
    /// ```rust
    /// use tinysearch::{BasicPost, TinySearch};
    /// use std::collections::HashMap;
    ///
    /// let posts = vec![BasicPost {
    ///     title: "My Post".to_string(),
    ///     url: "/post".to_string(),
    ///     body: Some("Post content".to_string()),
    ///     meta: HashMap::new(),
    /// }];
    /// let search = TinySearch::new();
    /// let index = search.build_index(&posts).unwrap();
    /// let index_bytes = search.serialize_index(&index).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the selected index representation cannot be encoded
    /// or violates its storage invariants.
    pub fn serialize_index(&self, index: &SearchIndex) -> Result<Vec<u8>, StorageError> {
        crate::encode_search_index(index)
    }

    /// Build and serialize an index in one step.
    ///
    /// # Errors
    ///
    /// Returns an error if index construction or serialization fails.
    #[deprecated(note = "call build_index, then serialize_index")]
    pub fn build_and_serialize_index<P: Post>(
        &self,
        posts: &[P],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let index = self.build_index(posts)?;
        self.serialize_index(&index)
            .map_err(std::convert::Into::into)
    }

    /// Load a search index from serialized bytes
    ///
    /// This method deserializes bytes produced by
    /// [`serialize_index`](Self::serialize_index) or compatible serialization.
    ///
    /// # Arguments
    /// * `bytes` - Serialized index bytes
    ///
    /// # Returns
    /// * `Ok(SearchIndex)` - Successfully loaded search index
    /// * `Err(StorageError)` - Deserialization error
    ///
    /// # Errors
    ///
    /// Returns an error if `bytes` contain malformed or unsupported index data.
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
    /// let built_index = search.build_index(&posts).unwrap();
    /// let index_bytes = search.serialize_index(&built_index).unwrap();
    ///
    /// // Then load it back
    /// let index = search.load_index_from_bytes(&index_bytes).unwrap();
    /// let results = search.search(&index, "content", 10);
    /// ```
    pub fn load_index_from_bytes(&self, bytes: &[u8]) -> Result<SearchIndex, StorageError> {
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
    fn cleanup(s: &str) -> String {
        s.replace(|c: char| !(c.is_alphabetic() || c == '\''), " ")
    }

    /// Tokenize input text, removing stopwords and normalizing to lowercase
    fn tokenize_with_stopwords(words: &str, stopwords: &HashSet<String>) -> HashSet<String> {
        Self::cleanup(&strip_markdown(words))
            .split_whitespace()
            .filter(|&word| !word.trim().is_empty())
            .map(str::to_lowercase)
            .filter(|word| !stopwords.contains(word))
            .collect()
    }

    /// Tokenize prepared posts for an index backend.
    fn prepare_documents(
        posts: HashMap<PostId, Option<String>>,
        stopwords: &HashSet<String>,
    ) -> Vec<IndexedDocument> {
        let split_posts: HashMap<PostId, Option<HashSet<String>>> = posts
            .into_iter()
            .map(|(post, content)| {
                (
                    post,
                    content.map(|content| Self::tokenize_with_stopwords(&content, stopwords)),
                )
            })
            .collect();

        split_posts
            .into_iter()
            .map(|(post_id, body)| {
                let mut content = Self::tokenize_with_stopwords(&post_id.title, stopwords);
                if !post_id.meta.is_empty() {
                    content.extend(Self::tokenize_with_stopwords(&post_id.meta, stopwords));
                }
                if let Some(body) = body {
                    content.extend(body);
                }
                IndexedDocument::new(post_id, content)
            })
            .collect()
    }

    /// Prepare posts for index generation (internal implementation)
    fn prepare_posts<P: Post>(posts: &[P]) -> HashMap<PostId, Option<String>> {
        posts
            .iter()
            .map(|post| {
                let metadata = post.meta();
                let meta_str = if metadata.is_empty() {
                    String::new()
                } else {
                    serde_json::to_string(&metadata).unwrap_or_default()
                };
                let post_id = PostId {
                    title: post.title().to_string(),
                    url: post.url().to_string(),
                    meta: meta_str,
                };
                let body = post.body().map(std::string::ToString::to_string);
                (post_id, body)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::{ExactIndexBackend, IndexBackend, IndexKind};

    #[derive(Default)]
    struct RecordingBackend {
        documents: RefCell<Vec<IndexedDocument>>,
    }

    impl IndexBackend for RecordingBackend {
        fn build(&self, documents: Vec<IndexedDocument>) -> SearchIndex {
            self.documents.replace(documents.clone());
            ExactIndexBackend.build(documents)
        }
    }

    fn post() -> BasicPost {
        BasicPost {
            title: "Rust Guide".to_string(),
            url: "/rust".to_string(),
            body: Some("Observability details".to_string()),
            meta: HashMap::from([("author".to_string(), "Ferris Crab".to_string())]),
        }
    }

    #[test]
    fn builds_with_both_builtin_backends() -> Result<(), Box<dyn std::error::Error>> {
        let posts = [post()];
        let exact = TinySearch::new().build_index(&posts)?;
        assert_eq!(TinySearch::new().search(&exact, "rus", 5).len(), 1);
        assert_eq!(TinySearch::new().search(&exact, "obser", 5).len(), 1);
        assert_eq!(TinySearch::new().search(&exact, "ferr", 5).len(), 1);

        let xor_search = TinySearch::new().with_index_kind(IndexKind::Xor8);
        let xor8 = xor_search.build_index(&posts)?;
        assert_eq!(xor_search.search(&xor8, "rus", 5).len(), 1);
        assert_eq!(xor_search.search(&xor8, "observability", 5).len(), 1);
        assert_eq!(xor_search.search(&xor8, "ferris", 5).len(), 1);

        let bytes = xor_search.serialize_index(&xor8)?;
        let round_tripped = xor_search.load_index_from_bytes(&bytes)?;
        assert_eq!(
            xor_search.search(&round_tripped, "observability", 5).len(),
            1
        );
        Ok(())
    }

    #[test]
    fn custom_backend_receives_all_normalized_terms() -> Result<(), Box<dyn std::error::Error>> {
        let backend = RecordingBackend::default();
        TinySearch::new().build_index_with(&[post()], &backend)?;

        let documents = backend.documents.borrow();
        let document = &documents[0];
        for term in [
            "rust",
            "guide",
            "observability",
            "details",
            "ferris",
            "crab",
        ] {
            assert!(
                document.terms.contains(term),
                "missing normalized term {term:?}"
            );
        }
        Ok(())
    }
}
