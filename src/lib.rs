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

#[cfg(feature = "bin")]
use std::path::Path;

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
        SearchSchema {
            indexed_fields: vec!["title".to_string(), "body".to_string()],
            metadata_fields: vec![],
            url_field: "url".to_string(),
        }
    }
}

#[cfg(feature = "bin")]
impl SearchSchema {
    /// Load schema from tinysearch.toml file, falling back to defaults if not found
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let toml_path = path.as_ref().join("tinysearch.toml");

        if !toml_path.exists() {
            return Ok(SearchSchema::default());
        }

        let toml_content = std::fs::read_to_string(&toml_path)
            .map_err(|e| format!("Failed to read tinysearch.toml: {e}"))?;
        let config: SearchSchemaConfig = toml::from_str(&toml_content)
            .map_err(|e| format!("Failed to parse tinysearch.toml: {e}"))?;

        // Validate schema
        config.schema.validate()?;

        Ok(config.schema)
    }

    /// Validate the schema configuration
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

    /// Get all fields that should be processed from JSON (indexed + metadata + url)
    pub fn all_fields(&self) -> Vec<String> {
        let mut fields = self.indexed_fields.clone();
        fields.extend(self.metadata_fields.clone());
        if !fields.contains(&self.url_field) {
            fields.push(self.url_field.clone());
        }
        fields
    }
}

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

#[cfg(test)]
#[cfg(feature = "bin")]
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
            panic!("Default schema validation failed: {}", e);
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
