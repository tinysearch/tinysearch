use bincode::Error as BincodeError;
use std::convert::From;
use tinysearch_cuckoofilter::{self, CuckooFilter, ExportedCuckooFilter};

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};

pub type PostId = (String, String);
pub type Filters = HashMap<PostId, CuckooFilter<DefaultHasher>>;
type ExportedFilters = HashMap<PostId, ExportedCuckooFilter>;

pub struct Storage {
    pub filters: Filters,
}

impl From<Filters> for Storage {
    fn from(filters: Filters) -> Self {
        Storage { filters }
    }
}

pub trait Score {
    fn score(&self, terms: &HashSet<String>) -> u32;
}

// the score is the number of terms from the query that contained in the current
// filter
impl Score for CuckooFilter<DefaultHasher> {
    fn score(&self, terms: &HashSet<String>) -> u32 {
        terms
            .iter()
            .filter(|term| self.contains(&term.to_lowercase()))
            .count() as u32
    }
}

impl Storage {
    pub fn to_bytes(&self) -> Result<Vec<u8>, BincodeError> {
        let encoded: Vec<u8> = bincode::serialize(&self.dehydrate())?;
        Ok(encoded)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BincodeError> {
        let decoded: ExportedFilters = bincode::deserialize(bytes)?;
        Ok(Storage {
            filters: Storage::hydrate(decoded),
        })
    }

    fn dehydrate(&self) -> ExportedFilters {
        self.filters
            .iter()
            .map(|(key, filter)| (key.clone(), filter.export()))
            .collect()
    }

    fn hydrate(exported_filters: ExportedFilters) -> Filters {
        exported_filters
            .into_iter()
            .map(|(key, exported)| (key.clone(), CuckooFilter::<DefaultHasher>::from(exported)))
            .collect()
    }
}
