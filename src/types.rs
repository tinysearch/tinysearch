use bincode::{self, Error as BincodeError};
use cuckoofilter::{self, CuckooFilter, ExportedCuckooFilter};
use serde_derive::{Deserialize, Serialize};
use std::convert::From;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

type Filters = HashMap<String, CuckooFilter<DefaultHasher>>;
type ExportedFilters = HashMap<String, ExportedCuckooFilter>;

pub struct Storage {
    filters: Filters,
}

impl From<Filters> for Storage {
    fn from(filters: Filters) -> Self {
        Storage { filters }
    }
}

impl Storage {
    pub fn to_bytes(&self) -> Result<Vec<u8>, BincodeError> {
        let encoded: Vec<u8> = bincode::serialize(&self.dehydrate())?;
        Ok(encoded)
    }

    fn dehydrate(&self) -> ExportedFilters {
        self
            .filters
            .iter()
            .map(|(key, filter)| (key.clone(), filter.export()))
            .collect()
    }
}
