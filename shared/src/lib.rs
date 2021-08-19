use bincode::Error as BincodeError;
use serde::{Deserialize, Serialize};
use std::convert::From;
use xorf::{Filter, HashProxy, Xor8};

use std::collections::hash_map::DefaultHasher;

type Title = String;
type Url = String;
pub type PostId = (Title, Url);
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
