use bincode::Error as BincodeError;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::hash_map::DefaultHasher;
use std::convert::From;
use xorf::{Filter as XorfFilter, HashProxy, Xor8};

type Title = String;
type Url = String;
type Meta = Option<String>;
pub type PostId = (Title, Url, Meta);
pub type PostFilter = (PostId, HashProxy<String, DefaultHasher, Xor8>);
pub type Filters = Vec<PostFilter>;

#[derive(Serialize, Deserialize)]
pub struct Storage {
    pub filters: Filters,
}

impl From<Filters> for Storage {
    fn from(filters: Filters) -> Self {
        Storage { filters }
    }
}

pub trait Score {
    fn score(&self, terms: &[String]) -> usize;
}

// the score denotes the number of terms from the query that are contained in the
// current filter
impl Score for HashProxy<String, DefaultHasher, Xor8> {
    fn score(&self, terms: &[String]) -> usize {
        terms.iter().filter(|term| self.contains(term)).count()
    }
}

impl Storage {
    pub fn to_bytes(&self) -> Result<Vec<u8>, BincodeError> {
        let encoded: Vec<u8> = bincode::serialize(&self)?;
        Ok(encoded)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BincodeError> {
        let decoded: Filters = bincode::deserialize(bytes)?;
        Ok(Storage { filters: decoded })
    }
}

pub type Filter = HashProxy<String, DefaultHasher, Xor8>;

const TITLE_WEIGHT: usize = 3;

// Wrapper around filter score, that also scores the post title
// Post title score has a higher weight than post body
fn score(title: &str, search_terms: &[String], filter: &Filter) -> usize {
    let title_terms: Vec<String> = tokenize(title);
    let title_score: usize = search_terms
        .iter()
        .filter(|term| title_terms.contains(term))
        .count();
    TITLE_WEIGHT * title_score + filter.score(search_terms)
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split_whitespace()
        .filter(|&t| !t.trim().is_empty())
        .map(String::from)
        .collect()
}
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
