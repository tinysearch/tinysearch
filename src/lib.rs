//! tinysearch - A tiny search engine for static websites
//!
//! This crate provides a fast, memory-efficient search engine that can be compiled
//! to WebAssembly for client-side search functionality on static websites.

use bincode::Error as BincodeError;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::hash_map::DefaultHasher;
use std::convert::From;
use xorf::{Filter as XorfFilter, HashProxy, Xor8};

/// Title of a post
type Title = String;
/// URL of a post
type Url = String;
/// Optional metadata for a post
type Meta = Option<String>;

/// Represents a post with its title, URL, and optional metadata
pub type PostId = (Title, Url, Meta);

/// A post with its associated Xor filter for fast lookups
pub type PostFilter = (PostId, HashProxy<String, DefaultHasher, Xor8>);

/// Collection of all post filters
pub type Filters = Vec<PostFilter>;

/// Storage container for serialized search index
#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// Vector of post filters for search functionality
    pub filters: Filters,
}

impl From<Filters> for Storage {
    fn from(filters: Filters) -> Self {
        Storage { filters }
    }
}

/// Trait for scoring search terms against a filter
pub trait Score {
    /// Returns the number of search terms that match this filter
    fn score(&self, terms: &[String]) -> usize;
}

/// Implementation of scoring for Xor filters
/// The score denotes the number of terms from the query that are contained in the current filter
impl Score for HashProxy<String, DefaultHasher, Xor8> {
    fn score(&self, terms: &[String]) -> usize {
        terms.iter().filter(|term| self.contains(term)).count()
    }
}

impl Storage {
    /// Serializes the storage to bytes using bincode
    pub fn to_bytes(&self) -> Result<Vec<u8>, BincodeError> {
        let encoded: Vec<u8> = bincode::serialize(&self)?;
        Ok(encoded)
    }

    /// Deserializes storage from bytes using bincode
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BincodeError> {
        let decoded: Filters = bincode::deserialize(bytes)?;
        Ok(Storage { filters: decoded })
    }
}

/// Type alias for the filter used in search
pub type Filter = HashProxy<String, DefaultHasher, Xor8>;

/// Weight multiplier for title matches vs body matches
const TITLE_WEIGHT: usize = 3;

/// Calculates a combined score for a post based on title and body matches
/// Post title matches are weighted higher than body matches
fn score(title: &str, search_terms: &[String], filter: &Filter) -> usize {
    let title_terms: Vec<String> = tokenize(title);
    let title_score: usize = search_terms
        .iter()
        .filter(|term| title_terms.contains(term))
        .count();
    TITLE_WEIGHT * title_score + filter.score(search_terms)
}

/// Tokenizes a string into lowercase words, removing empty tokens
fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split_whitespace()
        .filter(|&t| !t.trim().is_empty())
        .map(String::from)
        .collect()
}

/// Performs a search query against the provided filters
///
/// # Arguments
/// * `filters` - The search index containing all posts and their filters
/// * `query` - The search query string
/// * `num_results` - Maximum number of results to return
///
/// # Returns
/// Vector of `PostId` references, sorted by relevance score (highest first)
pub fn search(filters: &'_ Filters, query: String, num_results: usize) -> Vec<&'_ PostId> {
    let search_terms: Vec<String> = tokenize(&query);
    let mut matches: Vec<(&PostId, usize)> = filters
        .iter()
        .map(|(post_id, filter)| (post_id, score(&post_id.0, &search_terms, filter)))
        .filter(|(_post_id, score)| *score > 0)
        .collect();

    matches.sort_by_key(|k| Reverse(k.1));

    matches.into_iter().take(num_results).map(|p| p.0).collect()
}
