//! tinysearch - A tiny search engine for static websites
//!
//! This crate provides a fast, memory-efficient search engine that can be compiled
//! to WebAssembly for client-side search functionality on static websites.
//!
//! # Library Usage
//!
//! This crate can be used both as a command-line tool and as a library for programmatic
//! access to search index generation and search functionality.
//!
//! ## Basic Usage
//!
//! ```rust
//! use tinysearch::{BasicPost, TinySearch, SearchIndex};
//! use std::collections::HashMap;
//!
//! // Create posts
//! let posts = vec![
//!     BasicPost {
//!         title: "First Post".to_string(),
//!         url: "/first".to_string(),
//!         body: Some("This is the first post content".to_string()),
//!         meta: HashMap::new(),
//!     },
//!     BasicPost {
//!         title: "Second Post".to_string(),
//!         url: "/second".to_string(),
//!         body: Some("This is the second post about rust programming".to_string()),
//!         meta: HashMap::new(),
//!     }
//! ];
//!
//! // Build search index
//! let search = TinySearch::new();
//! let index: SearchIndex = search.build_index(&posts).expect("Failed to build index");
//!
//! // Search
//! let results = search.search(&index, "rust", 10);
//! ```

pub mod api;
pub mod sharded;

use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::convert::From;
use xorf::{Filter as XorfFilter, HashProxy, Xor8};

#[cfg(feature = "bin")]
use std::path::Path;

/// Represents a post with its title, URL, and metadata
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PostId {
    /// Post title
    pub title: String,
    /// Post URL
    pub url: String,
    /// Serialized metadata string
    pub meta: String,
}

/// Type of the probabilistic filter used by the Xor8 backend.
pub type Filter = HashProxy<String, DefaultHasher, Xor8>;

/// A post and its associated Xor8 filter.
#[derive(Serialize, Deserialize)]
pub struct PostFilter {
    /// Indexed post metadata.
    pub post: PostId,
    /// Probabilistic filter containing the post's normalized terms.
    pub filter: Filter,
}

/// A search index containing either exact postings or per-post Xor8 filters.
///
/// Exact indexes use sorted terms for binary-search prefix lookup. Xor8 indexes
/// retain the compact probabilistic representation used by earlier releases.
///
/// # Example
///
/// ```rust
/// use tinysearch::{BasicPost, TinySearch, SearchIndex};
/// use std::collections::HashMap;
///
/// let posts = vec![
///     BasicPost {
///         title: "My Post".to_string(),
///         url: "/my-post".to_string(),
///         body: Some("Post content here".to_string()),
///         meta: HashMap::new(),
///     }
/// ];
///
/// let search = TinySearch::new();
/// let index: SearchIndex = search.build_index(&posts).unwrap();
/// let results = search.search(&index, "cont", 10);
/// ```
#[derive(Serialize, Deserialize)]
pub struct SearchIndex {
    data: SearchIndexData,
}

#[derive(Serialize, Deserialize)]
enum SearchIndexData {
    Exact(ExactIndex),
    Xor8(Xor8Index),
}

#[derive(Serialize, Deserialize)]
struct ExactIndex {
    posts: Vec<PostId>,
    terms: Vec<String>,
    postings: Vec<Vec<usize>>,
}

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct Xor8Index {
    entries: Vec<PostFilter>,
}

/// A post and all of its normalized title, body, and metadata terms.
#[derive(Debug, Clone)]
pub struct IndexedDocument {
    /// Indexed post metadata.
    pub post: PostId,
    /// Normalized searchable terms from the title, body, and metadata.
    pub terms: HashSet<String>,
}

impl IndexedDocument {
    /// Creates a document from post metadata and normalized terms.
    pub fn new<T>(post: PostId, terms: T) -> Self
    where
        T: IntoIterator<Item = String>,
    {
        Self {
            post,
            terms: terms.into_iter().collect(),
        }
    }
}

/// Common interface for building a supported index representation.
pub trait IndexBackend {
    /// Builds an index from normalized documents.
    fn build(&self, documents: Vec<IndexedDocument>) -> SearchIndex;
}

/// Exact inverted-index backend.
///
/// [`Storage::to_bytes`] delta- and varint-encodes its posting lists.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExactIndexBackend;

/// Xor8 backend with compact probabilistic per-document filters.
#[derive(Debug, Clone, Copy, Default)]
pub struct Xor8IndexBackend;

/// Built-in index backend selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexKind {
    /// Exact body, metadata, and title prefix matching.
    #[default]
    Exact,
    /// Probabilistic Xor8 membership with exact body and metadata matching.
    Xor8,
}

impl IndexKind {
    /// Returns the selected built-in backend.
    #[must_use]
    pub fn backend(self) -> &'static dyn IndexBackend {
        static EXACT: ExactIndexBackend = ExactIndexBackend;
        static XOR8: Xor8IndexBackend = Xor8IndexBackend;

        match self {
            Self::Exact => &EXACT,
            Self::Xor8 => &XOR8,
        }
    }
}

impl std::str::FromStr for IndexKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "exact" => Ok(Self::Exact),
            "xor" | "xor8" => Ok(Self::Xor8),
            _ => Err(format!("unknown indexer {value:?}; expected exact or xor8")),
        }
    }
}

impl std::fmt::Display for IndexKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => formatter.write_str("exact"),
            Self::Xor8 => formatter.write_str("xor8"),
        }
    }
}

impl ExactIndex {
    /// Builds exact term-to-document postings from normalized documents.
    ///
    /// Title terms are omitted from postings because titles are scored directly.
    fn from_documents(documents: Vec<IndexedDocument>) -> Self {
        let mut posts = Vec::new();
        let mut postings = BTreeMap::<String, Vec<usize>>::new();
        for (document_id, document) in documents.into_iter().enumerate() {
            let IndexedDocument { post, terms } = document;
            let title_terms: BTreeSet<String> = tokenize(&post.title).into_iter().collect();
            let terms: BTreeSet<String> = terms
                .into_iter()
                .filter(|term| !term.is_empty() && !title_terms.contains(term))
                .collect();
            posts.push(post);
            for term in terms {
                postings.entry(term).or_default().push(document_id);
            }
        }
        let (terms, postings) = postings.into_iter().unzip();
        Self {
            posts,
            terms,
            postings,
        }
    }
}

impl Xor8Index {
    /// Builds per-document Xor8 filters from all normalized document terms.
    fn from_documents(documents: Vec<IndexedDocument>) -> Self {
        let entries = documents
            .into_iter()
            .map(|document| {
                let terms: Vec<String> = document.terms.into_iter().collect();
                PostFilter {
                    post: document.post,
                    filter: HashProxy::from(&terms),
                }
            })
            .collect();
        Self { entries }
    }
}

impl IndexBackend for ExactIndexBackend {
    fn build(&self, documents: Vec<IndexedDocument>) -> SearchIndex {
        SearchIndex {
            data: SearchIndexData::Exact(ExactIndex::from_documents(documents)),
        }
    }
}

impl IndexBackend for Xor8IndexBackend {
    fn build(&self, documents: Vec<IndexedDocument>) -> SearchIndex {
        SearchIndex {
            data: SearchIndexData::Xor8(Xor8Index::from_documents(documents)),
        }
    }
}

impl SearchIndex {
    /// Returns the number of indexed posts.
    #[must_use]
    pub fn len(&self) -> usize {
        match &self.data {
            SearchIndexData::Exact(index) => index.posts.len(),
            SearchIndexData::Xor8(index) => index.entries.len(),
        }
    }

    /// Returns whether the index contains no posts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Converts an exact index into a root and content-addressed lexical shards.
    ///
    /// Existing monolithic serialization is unchanged. Shard posting lists keep
    /// the exact index's global dense document identifiers.
    ///
    /// # Errors
    ///
    /// Returns [`ShardError::UnsupportedBackend`] for Xor8 indexes, or a
    /// validation or encoding error if the exact index cannot be sharded.
    pub fn to_sharded_bundle(&self, config: ShardConfig) -> Result<ShardedIndexBundle, ShardError> {
        sharded::build_bundle(self, config)
    }
}

impl From<Vec<PostFilter>> for SearchIndex {
    fn from(filters: Vec<PostFilter>) -> Self {
        Self {
            data: SearchIndexData::Xor8(Xor8Index { entries: filters }),
        }
    }
}

impl FromIterator<PostFilter> for SearchIndex {
    fn from_iter<T: IntoIterator<Item = PostFilter>>(iter: T) -> Self {
        Self::from(iter.into_iter().collect::<Vec<_>>())
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        ExactIndexBackend.build(Vec::new())
    }
}

// Re-export public API types from the API module
pub use api::{BasicPost, Post, TinySearch};
pub use sharded::{
    ShardArtifact, ShardConfig, ShardDescriptor, ShardError, ShardId, ShardedIndex,
    ShardedIndexBundle,
};

/// Configuration schema for tinysearch.toml
#[cfg(feature = "bin")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSchemaConfig {
    /// Schema configuration section
    pub schema: SearchSchema,
}

/// Schema configuration details
#[cfg(feature = "bin")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSchema {
    /// Fields that should be indexed for searching
    pub indexed_fields: Vec<String>,
    /// Fields that should be stored as metadata but not indexed
    pub metadata_fields: Vec<String>,
    /// Field that contains the URL for each document
    pub url_field: String,
}

#[cfg(feature = "bin")]
impl Default for SearchSchema {
    /// Default schema configuration matching current JSON structure
    fn default() -> Self {
        Self {
            indexed_fields: vec!["title".to_string(), "body".to_string()],
            metadata_fields: vec![],
            url_field: "url".to_string(),
        }
    }
}

#[cfg(feature = "bin")]
impl SearchSchema {
    /// Load schema from tinysearch.toml file, falling back to defaults if not found.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be read, parsed, or validated.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let toml_path = path.as_ref().join("tinysearch.toml");

        if !toml_path.exists() {
            return Ok(Self::default());
        }

        let toml_content = std::fs::read_to_string(&toml_path)
            .map_err(|e| format!("Failed to read tinysearch.toml: {e}"))?;
        let config: SearchSchemaConfig = toml::from_str(&toml_content)
            .map_err(|e| format!("Failed to parse tinysearch.toml: {e}"))?;

        config.schema.validate()?;

        Ok(config.schema)
    }

    /// Validate the schema configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are empty or fields are duplicated.
    pub fn validate(&self) -> Result<(), String> {
        if self.indexed_fields.is_empty() {
            return Err("indexed_fields cannot be empty".to_string());
        }

        if self.url_field.is_empty() {
            return Err("url_field cannot be empty".to_string());
        }

        // Check for overlap between indexed and metadata fields
        let all_fields: Vec<_> = self
            .indexed_fields
            .iter()
            .chain(self.metadata_fields.iter())
            .chain(std::iter::once(&self.url_field))
            .collect();

        let mut unique_fields = std::collections::HashSet::new();
        for field in &all_fields {
            if !unique_fields.insert(field) {
                return Err(format!("Duplicate field definition: {field}"));
            }
        }

        Ok(())
    }

    /// Get all fields that should be processed from JSON (indexed + metadata + URL).
    #[must_use]
    pub fn all_fields(&self) -> Vec<String> {
        let mut fields = self.indexed_fields.clone();
        fields.extend(self.metadata_fields.clone());
        if !fields.contains(&self.url_field) {
            fields.push(self.url_field.clone());
        }
        fields
    }
}

/// Storage container for a serialized search index.
#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// Search index data. The field name is retained for source compatibility.
    pub filters: SearchIndex,
}

/// Error returned while encoding or decoding search index storage.
#[derive(Debug)]
#[non_exhaustive]
pub enum StorageError {
    /// The search index could not be encoded.
    Encode(bincode::error::EncodeError),
    /// The search index could not be decoded.
    Decode(bincode::error::DecodeError),
    /// The compact index data is malformed.
    MalformedIndex(&'static str),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encode(error) => write!(formatter, "failed to encode search index: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode search index: {error}"),
            Self::MalformedIndex(reason) => write!(formatter, "malformed search index: {reason}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encode(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::MalformedIndex(_) => None,
        }
    }
}

impl From<SearchIndex> for Storage {
    fn from(index: SearchIndex) -> Self {
        Self { filters: index }
    }
}

impl From<Vec<PostFilter>> for Storage {
    fn from(filters: Vec<PostFilter>) -> Self {
        Self {
            filters: filters.into(),
        }
    }
}

/// Trait for scoring search terms against an Xor8 filter.
pub trait Score {
    /// Returns the number of search terms that match this filter
    fn score(&self, terms: &[String]) -> usize;
}

/// Scores an Xor8 filter by the number of contained query terms.
impl Score for HashProxy<String, DefaultHasher, Xor8> {
    fn score(&self, terms: &[String]) -> usize {
        terms.iter().filter(|term| self.contains(term)).count()
    }
}

pub(crate) fn encode_search_index(index: &SearchIndex) -> Result<Vec<u8>, StorageError> {
    match &index.data {
        SearchIndexData::Exact(exact) => encode_exact_index(exact),
        SearchIndexData::Xor8(xor8) => encode_xor8_index(xor8),
    }
}

fn encode_xor8_index(index: &Xor8Index) -> Result<Vec<u8>, StorageError> {
    bincode::serde::encode_to_vec(&index.entries, bincode::config::legacy())
        .map_err(StorageError::Encode)
}

impl Storage {
    /// Serializes exact indexes with compact delta postings and uses the
    /// bincode 1-compatible format for Xor8 indexes.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails or the index violates its storage
    /// invariants.
    pub fn to_bytes(&self) -> Result<Vec<u8>, StorageError> {
        encode_search_index(&self.filters)
    }

    /// Deserializes exact and Xor8 indexes.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes cannot be decoded or contain a malformed
    /// compact index.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StorageError> {
        if bytes.starts_with(STORAGE_MAGIC) {
            return decode_exact_index(bytes).map(|index| Self {
                filters: SearchIndex {
                    data: SearchIndexData::Exact(index),
                },
            });
        }

        let (filters, _) = bincode::serde::decode_from_slice::<Vec<PostFilter>, _>(
            bytes,
            bincode::config::legacy(),
        )
        .map_err(StorageError::Decode)?;
        Ok(Self {
            filters: SearchIndex::from(filters),
        })
    }
}

fn encode_exact_index(index: &ExactIndex) -> Result<Vec<u8>, StorageError> {
    if index.terms.len() != index.postings.len()
        || index.terms.windows(2).any(|pair| pair[0] >= pair[1])
        || index.terms.iter().any(|term| term.contains('\n'))
    {
        return Err(StorageError::MalformedIndex(
            "terms and postings must be aligned, sorted, and newline-free",
        ));
    }

    let posts = bincode::serde::encode_to_vec(&index.posts, bincode::config::standard())
        .map_err(StorageError::Encode)?;
    let mut vocabulary = index.terms.join("\n").into_bytes();
    if !vocabulary.is_empty() {
        vocabulary.push(b'\n');
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(STORAGE_MAGIC);
    write_varint(&mut bytes, posts.len());
    bytes.extend_from_slice(&posts);
    write_varint(&mut bytes, index.terms.len());
    write_varint(&mut bytes, vocabulary.len());
    bytes.extend_from_slice(&vocabulary);

    for postings in &index.postings {
        if postings.is_empty() {
            return Err(StorageError::MalformedIndex(
                "posting lists must not be empty",
            ));
        }
        write_varint(&mut bytes, postings.len());
        let mut previous = 0_usize;
        for (position, &document) in postings.iter().enumerate() {
            if document >= index.posts.len() {
                return Err(StorageError::MalformedIndex(
                    "posting document is out of range",
                ));
            }
            let delta = if position == 0 {
                document
            } else {
                document
                    .checked_sub(previous)
                    .filter(|delta| *delta > 0)
                    .ok_or(StorageError::MalformedIndex(
                        "posting lists must be strictly increasing",
                    ))?
            };
            write_varint(&mut bytes, delta);
            previous = document;
        }
    }
    Ok(bytes)
}

fn decode_exact_index(bytes: &[u8]) -> Result<ExactIndex, StorageError> {
    let payload = decode_exact_header(bytes)?;
    let mut cursor = 0;
    let posts = decode_posts(payload, &mut cursor)?;
    let terms = decode_vocabulary(payload, &mut cursor)?;
    let postings = decode_postings(payload, &mut cursor, terms.len(), posts.len())?;

    if cursor != payload.len() {
        return Err(StorageError::MalformedIndex("index has trailing bytes"));
    }

    Ok(ExactIndex {
        posts,
        terms,
        postings,
    })
}

fn decode_exact_header(bytes: &[u8]) -> Result<&[u8], StorageError> {
    bytes
        .strip_prefix(STORAGE_MAGIC)
        .ok_or(StorageError::MalformedIndex(
            "compact index header is missing",
        ))
}

fn decode_posts(input: &[u8], cursor: &mut usize) -> Result<Vec<PostId>, StorageError> {
    let section_len = read_varint(input, cursor)?;
    let section = take_section(
        input,
        cursor,
        section_len,
        "post payload length overflowed",
        "post payload extends beyond the index",
    )?;
    let (posts, consumed) =
        bincode::serde::decode_from_slice::<Vec<PostId>, _>(section, bincode::config::standard())
            .map_err(StorageError::Decode)?;
    if consumed != section.len() {
        return Err(StorageError::MalformedIndex(
            "post payload has trailing bytes",
        ));
    }
    Ok(posts)
}

fn decode_vocabulary(input: &[u8], cursor: &mut usize) -> Result<Vec<String>, StorageError> {
    let term_count = read_varint(input, cursor)?;
    let section_len = read_varint(input, cursor)?;
    let section = take_section(
        input,
        cursor,
        section_len,
        "vocabulary length overflowed",
        "vocabulary extends beyond the index",
    )?;
    let vocabulary = std::str::from_utf8(section)
        .map_err(|_error| StorageError::MalformedIndex("vocabulary is not valid UTF-8"))?;
    let terms: Vec<String> = vocabulary.lines().map(String::from).collect();
    if terms.len() != term_count || !terms.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(StorageError::MalformedIndex(
            "vocabulary must be sorted, unique, and match its term count",
        ));
    }
    Ok(terms)
}

fn decode_postings(
    input: &[u8],
    cursor: &mut usize,
    term_count: usize,
    document_count: usize,
) -> Result<Vec<Vec<usize>>, StorageError> {
    (0..term_count)
        .map(|_| decode_posting_list(input, cursor, document_count))
        .collect()
}

fn decode_posting_list(
    input: &[u8],
    cursor: &mut usize,
    document_count: usize,
) -> Result<Vec<usize>, StorageError> {
    let posting_count = read_varint(input, cursor)?;
    let remaining_bytes = input.len().saturating_sub(*cursor);
    if posting_count == 0 || posting_count > document_count || posting_count > remaining_bytes {
        return Err(StorageError::MalformedIndex(
            "posting count is invalid for this index",
        ));
    }

    let mut documents = Vec::with_capacity(posting_count);
    let mut previous = 0_usize;
    for position in 0..posting_count {
        let delta = read_varint(input, cursor)?;
        if position > 0 && delta == 0 {
            return Err(StorageError::MalformedIndex(
                "posting documents must be strictly increasing",
            ));
        }
        let document = previous
            .checked_add(delta)
            .ok_or(StorageError::MalformedIndex("posting document overflowed"))?;
        if document >= document_count {
            return Err(StorageError::MalformedIndex(
                "posting document is out of range",
            ));
        }
        documents.push(document);
        previous = document;
    }
    Ok(documents)
}

fn take_section<'input>(
    input: &'input [u8],
    cursor: &mut usize,
    length: usize,
    overflow_message: &'static str,
    bounds_message: &'static str,
) -> Result<&'input [u8], StorageError> {
    let end = (*cursor)
        .checked_add(length)
        .ok_or(StorageError::MalformedIndex(overflow_message))?;
    let section = input
        .get(*cursor..end)
        .ok_or(StorageError::MalformedIndex(bounds_message))?;
    *cursor = end;
    Ok(section)
}

fn write_varint(output: &mut Vec<u8>, mut value: usize) {
    while value >= 0x80 {
        output.push((value.to_le_bytes()[0] & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value.to_le_bytes()[0]);
}

fn read_varint(input: &[u8], cursor: &mut usize) -> Result<usize, StorageError> {
    let mut value = 0_usize;
    for shift in (0..usize::BITS).step_by(7) {
        let byte = input
            .get(*cursor)
            .copied()
            .ok_or(StorageError::MalformedIndex(
                "index contains a truncated varint",
            ))?;
        *cursor = (*cursor)
            .checked_add(1)
            .ok_or(StorageError::MalformedIndex("varint cursor overflowed"))?;
        let payload = usize::from(byte & 0x7f);
        if payload > (usize::MAX >> shift) {
            return Err(StorageError::MalformedIndex(
                "varint exceeds the platform range",
            ));
        }
        value |= payload << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(StorageError::MalformedIndex(
        "index contains an unterminated varint",
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod storage_tests {
    use super::{
        ExactIndexBackend, IndexBackend, IndexedDocument, PostId, STORAGE_MAGIC, SearchIndexData,
        Storage, StorageError, search, write_varint,
    };
    use xorf::Filter;

    #[test]
    fn malformed_index_error_has_no_source() {
        let error = StorageError::MalformedIndex("bad section");
        assert_eq!(error.to_string(), "malformed search index: bad section");
        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn empty_index_keeps_bincode_one_wire_format() -> Result<(), StorageError> {
        let bytes = Storage::from(Vec::new()).to_bytes()?;
        assert_eq!(bytes, [0_u8; 8]);
        assert!(Storage::from_bytes(&bytes)?.filters.is_empty());
        Ok(())
    }

    #[test]
    fn reads_index_written_by_bincode_one_and_xorf_0_11() -> Result<(), StorageError> {
        let bytes = include_bytes!("testdata/xor8-storage-v0.10.bin");
        let storage = Storage::from_bytes(bytes)?;
        assert_eq!(storage.to_bytes()?, bytes);
        assert_eq!(storage.filters.len(), 1);
        let SearchIndexData::Xor8(index) = &storage.filters.data else {
            return Err(StorageError::MalformedIndex(
                "v0.10 Xor8 fixture decoded as an exact index",
            ));
        };
        let entry = &index.entries[0];
        assert!(!entry.post.title.is_empty());
        assert!(!entry.post.url.is_empty());
        assert!(
            entry
                .filter
                .contains(&entry.post.url.trim_start_matches('/').to_owned())
        );
        Ok(())
    }

    #[test]
    fn round_trips_exact_prefix_index() -> Result<(), StorageError> {
        let post = PostId {
            title: "Other title".to_string(),
            url: "/other".to_string(),
            meta: String::new(),
        };
        let index = ExactIndexBackend.build(vec![IndexedDocument::new(
            post,
            ["programming".to_string(), "program".to_string()],
        )]);
        let bytes = Storage::from(index).to_bytes()?;

        assert!(bytes.starts_with(STORAGE_MAGIC));
        let storage = Storage::from_bytes(&bytes)?;
        assert_eq!(search(&storage.filters, "prog", 5).len(), 1);
        assert_eq!(search(&storage.filters, "programming", 5).len(), 1);
        Ok(())
    }

    #[test]
    fn rejects_impossible_posting_counts() -> Result<(), StorageError> {
        let post = PostId {
            title: "Title".to_string(),
            url: "/title".to_string(),
            meta: String::new(),
        };
        let posts = bincode::serde::encode_to_vec(vec![post], bincode::config::standard())
            .map_err(StorageError::Encode)?;
        let mut bytes = STORAGE_MAGIC.to_vec();
        write_varint(&mut bytes, posts.len());
        bytes.extend_from_slice(&posts);
        write_varint(&mut bytes, 1);
        write_varint(&mut bytes, 5);
        bytes.extend_from_slice(b"word\n");
        write_varint(&mut bytes, usize::MAX);

        assert!(matches!(
            Storage::from_bytes(&bytes),
            Err(StorageError::MalformedIndex(_))
        ));
        Ok(())
    }
}

/// Prefix for the compact exact inverted-index storage format.
const STORAGE_MAGIC: &[u8] = b"tinysearch\x02";

/// Weight assigned to an exact title-term match.
const TITLE_EXACT_WEIGHT: usize = 3;

/// Weight assigned to a title-prefix match.
const TITLE_PREFIX_WEIGHT: usize = 2;

/// Minimum query length for prefix matching.
pub const MIN_PREFIX_LEN: usize = 3;

/// Calculates the score for one query term against the title.
fn title_term_score(title_terms: &[String], search_term: &str) -> usize {
    if title_terms
        .iter()
        .any(|title_term| title_term == search_term)
    {
        TITLE_EXACT_WEIGHT
    } else if search_term.chars().count() >= MIN_PREFIX_LEN
        && title_terms
            .iter()
            .any(|title_term| title_term.starts_with(search_term))
    {
        TITLE_PREFIX_WEIGHT
    } else {
        0
    }
}

fn title_scores(posts: &[PostId], search_terms: &[String]) -> Vec<usize> {
    let title_terms: Vec<Vec<String>> = posts.iter().map(|post| tokenize(&post.title)).collect();
    let mut scores = vec![0_usize; posts.len()];
    for search_term in search_terms {
        for (score, terms) in scores.iter_mut().zip(&title_terms) {
            *score = score.saturating_add(title_term_score(terms, search_term));
        }
    }
    scores
}

fn matching_term_range(terms: &[String], search_term: &str) -> std::ops::Range<usize> {
    let start = terms.partition_point(|term| term.as_str() < search_term);
    let Some(candidates) = terms.get(start..) else {
        return start..start;
    };
    let matching_count = if search_term.chars().count() >= MIN_PREFIX_LEN {
        candidates
            .iter()
            .take_while(|term| term.starts_with(search_term))
            .count()
    } else {
        usize::from(candidates.first().is_some_and(|term| term == search_term))
    };
    start..start.saturating_add(matching_count)
}

fn search_exact<'index>(
    index: &'index ExactIndex,
    search_terms: &[String],
    num_results: usize,
) -> Vec<&'index PostId> {
    let mut scores = title_scores(&index.posts, search_terms);
    let mut content_matches = vec![false; index.posts.len()];

    for search_term in search_terms {
        content_matches.fill(false);
        let matching_terms = matching_term_range(&index.terms, search_term);
        for postings in index
            .postings
            .iter()
            .skip(matching_terms.start)
            .take(matching_terms.len())
        {
            for &document in postings {
                if let Some(content_match) = content_matches.get_mut(document) {
                    *content_match = true;
                }
            }
        }
        for (score, &content_match) in scores.iter_mut().zip(&content_matches) {
            *score = score.saturating_add(usize::from(content_match));
        }
    }

    ranked_posts(&index.posts, scores, num_results)
}

fn search_xor8<'index>(
    index: &'index Xor8Index,
    search_terms: &[String],
    num_results: usize,
) -> Vec<&'index PostId> {
    let mut matches: Vec<(&PostId, usize)> = index
        .entries
        .iter()
        .map(|entry| {
            let title_terms = tokenize(&entry.post.title);
            let title_score = search_terms.iter().fold(0_usize, |score, term| {
                score.saturating_add(title_term_score(&title_terms, term))
            });
            (
                &entry.post,
                title_score.saturating_add(entry.filter.score(search_terms)),
            )
        })
        .filter(|(_post, score)| *score > 0)
        .collect();
    matches.sort_by_key(|(_post, score)| Reverse(*score));
    matches
        .into_iter()
        .take(num_results)
        .map(|(post, _score)| post)
        .collect()
}

fn ranked_posts(posts: &[PostId], scores: Vec<usize>, num_results: usize) -> Vec<&PostId> {
    let mut matches: Vec<(&PostId, usize)> = posts
        .iter()
        .zip(scores)
        .filter(|(_post, score)| *score > 0)
        .collect();
    matches.sort_by_key(|(_post, score)| Reverse(*score));
    matches
        .into_iter()
        .take(num_results)
        .map(|(post, _score)| post)
        .collect()
}

/// Tokenizes query and title text with lightweight punctuation cleanup.
fn tokenize(text: &str) -> Vec<String> {
    text.replace(
        |character: char| !(character.is_alphabetic() || character == '\''),
        " ",
    )
    .split_whitespace()
    .map(str::to_lowercase)
    .collect()
}

/// Performs a search query against the provided index.
///
/// Query terms of at least three characters support prefix matching in titles,
/// body text, and metadata for exact indexes. Shorter terms require exact
/// matches. Xor8 indexes keep exact body and metadata matching.
///
/// # Arguments
/// * `index` - The search index built by an exact or Xor8 backend
/// * `query` - The search query string
/// * `num_results` - Maximum number of results to return
///
/// # Returns
/// Vector of `PostId` references, sorted by relevance score (highest first)
#[must_use]
pub fn search<'index>(
    index: &'index SearchIndex,
    query: &str,
    num_results: usize,
) -> Vec<&'index PostId> {
    let search_terms: Vec<String> = tokenize(query);
    match &index.data {
        SearchIndexData::Exact(index) => search_exact(index, &search_terms, num_results),
        SearchIndexData::Xor8(index) => search_xor8(index, &search_terms, num_results),
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod search_tests {
    use super::*;

    fn document(title: &str, indexed_terms: &[&str]) -> IndexedDocument {
        IndexedDocument::new(
            PostId {
                title: title.to_string(),
                url: format!("/{title}"),
                meta: String::new(),
            },
            indexed_terms.iter().map(ToString::to_string),
        )
    }

    fn exact_index(documents: impl IntoIterator<Item = IndexedDocument>) -> SearchIndex {
        ExactIndexBackend.build(documents.into_iter().collect())
    }

    #[test]
    fn built_in_index_backends_are_selectable() {
        let post = PostId {
            title: "Other title".to_string(),
            url: "/other".to_string(),
            meta: String::new(),
        };
        let document = IndexedDocument::new(post, ["programming".to_string()]);

        let exact = IndexKind::Exact.backend().build(vec![document.clone()]);
        assert!(matches!(&exact.data, SearchIndexData::Exact(_)));
        assert_eq!(search(&exact, "prog", 5).len(), 1);

        let xor8 = IndexKind::Xor8.backend().build(vec![document]);
        assert!(matches!(&xor8.data, SearchIndexData::Xor8(_)));
        assert_eq!(search(&xor8, "programming", 5).len(), 1);

        assert_eq!("exact".parse(), Ok(IndexKind::Exact));
        assert_eq!("xor8".parse(), Ok(IndexKind::Xor8));
        assert_eq!(IndexKind::default(), IndexKind::Exact);
        assert!(matches!(
            SearchIndex::default().data,
            SearchIndexData::Exact(_)
        ));
    }

    #[test]
    fn matches_title_prefixes() {
        let index = exact_index([document("Rust Programming", &[])]);
        let results = search(&index, "prog", 10);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Programming");
    }

    #[test]
    fn ignores_short_title_prefixes() {
        let index = exact_index([document("Rust Programming", &[])]);

        assert!(search(&index, "ru", 10).is_empty());
    }

    #[test]
    fn normalizes_title_and_query_punctuation() {
        let index = exact_index([document("Go!", &["go"])]);

        assert_eq!(search(&index, "go", 10).len(), 1);
        assert_eq!(search(&index, "go!", 10).len(), 1);
    }

    #[test]
    fn ranks_exact_title_matches_above_prefixes() {
        let index = exact_index([
            document("Rustacean Guide", &[]),
            document("Rust Guide", &[]),
        ]);
        let results = search(&index, "rust", 10);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust Guide");
        assert_eq!(results[1].title, "Rustacean Guide");
    }

    #[test]
    fn matches_body_prefixes() {
        let index = exact_index([document("Other Title", &["programming"])]);

        assert_eq!(search(&index, "prog", 10).len(), 1);
        assert_eq!(search(&index, "programming", 10).len(), 1);
    }

    #[test]
    fn one_prefix_only_scores_once_when_multiple_words_match() {
        let index = exact_index([
            document("First", &["program", "programming"]),
            document("Second", &["programming"]),
        ]);

        let results = search(&index, "prog", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn searches_all_prefix_completions() {
        let documents = (0..40).map(|number| {
            let title = format!("Document {number}");
            let term = format!("prefix{number:02}");
            document(&title, &[&term])
        });
        let index = exact_index(documents);

        assert_eq!(search(&index, "pre", 100).len(), 40);
    }
}

#[cfg(test)]
#[cfg(feature = "bin")]
#[allow(clippy::panic, clippy::unwrap_used)]
mod schema_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_schema() {
        let schema = SearchSchema::default();
        assert_eq!(schema.indexed_fields, vec!["title", "body"]);
        assert_eq!(schema.metadata_fields, Vec::<String>::new());
        assert_eq!(schema.url_field, "url");
        if let Err(e) = schema.validate() {
            panic!("Default schema validation failed: {e}");
        }
    }

    #[test]
    fn test_load_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let schema = SearchSchema::load_from_file(temp_dir.path()).unwrap();
        assert_eq!(schema.indexed_fields, vec!["title", "body"]);
    }

    #[test]
    fn test_load_valid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let toml_content = r#"
[schema]
indexed_fields = ["title", "description"]
metadata_fields = ["author", "date", "image_url"]
url_field = "permalink"
"#;
        std::fs::write(temp_dir.path().join("tinysearch.toml"), toml_content).unwrap();

        let schema = SearchSchema::load_from_file(temp_dir.path()).unwrap();
        assert_eq!(schema.indexed_fields, vec!["title", "description"]);
        assert_eq!(schema.metadata_fields, vec!["author", "date", "image_url"]);
        assert_eq!(schema.url_field, "permalink");
    }

    #[test]
    fn test_validation_empty_indexed_fields() {
        let schema = SearchSchema {
            indexed_fields: vec![],
            metadata_fields: vec!["url".to_string()],
            url_field: "url".to_string(),
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_validation_empty_url_field() {
        let schema = SearchSchema {
            indexed_fields: vec!["title".to_string()],
            metadata_fields: vec![],
            url_field: String::new(),
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_validation_duplicate_fields() {
        let schema = SearchSchema {
            indexed_fields: vec!["title".to_string(), "body".to_string()],
            metadata_fields: vec!["title".to_string()], // Duplicate!
            url_field: "url".to_string(),
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_all_fields_method() {
        let schema = SearchSchema {
            indexed_fields: vec!["title".to_string(), "body".to_string()],
            metadata_fields: vec!["author".to_string(), "date".to_string()],
            url_field: "permalink".to_string(),
        };

        let all_fields = schema.all_fields();
        assert!(all_fields.contains(&"title".to_string()));
        assert!(all_fields.contains(&"body".to_string()));
        assert!(all_fields.contains(&"author".to_string()));
        assert!(all_fields.contains(&"date".to_string()));
        assert!(all_fields.contains(&"permalink".to_string()));
    }

    #[test]
    fn test_invalid_toml_format() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_toml = "this is not valid toml [";
        std::fs::write(temp_dir.path().join("tinysearch.toml"), invalid_toml).unwrap();

        let result = SearchSchema::load_from_file(temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse"));
    }

    #[test]
    fn test_missing_schema_section() {
        let temp_dir = TempDir::new().unwrap();
        let toml_content = r#"
[other]
value = "test"
"#;
        std::fs::write(temp_dir.path().join("tinysearch.toml"), toml_content).unwrap();

        let result = SearchSchema::load_from_file(temp_dir.path());
        assert!(result.is_err());
    }
}
