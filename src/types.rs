use bincode::Error as BincodeError;
use bloomfilter::Bloom;
use serde::{Deserialize, Serialize};

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::convert::From;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct ExportedBloomFilter {
    bitmap: Vec<u8>,
    bitmap_bits: u64,
    k_num: u32,
    sip_keys: [(u64, u64); 2],
}


type ExportedBloomFilters = HashMap<PathBuf, ExportedBloomFilter>;
pub type Filters = HashMap<PathBuf, Bloom>;

pub struct Storage {
    pub filters: Filters,
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

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BincodeError> {
        let decoded = bincode::deserialize(bytes)?;
        Ok(Storage {
            filters: Storage::hydrate(decoded),
        })
    }

    fn dehydrate(&self) -> ExportedBloomFilters {
        self.filters
            .iter()
            .map(|(key, filter)| {
                (
                    key.clone(),
                    ExportedBloomFilter {
                        bitmap: filter.bitmap(),
                        bitmap_bits: filter.number_of_bits(),
                        k_num: filter.number_of_hash_functions(),
                        sip_keys: filter.sip_keys(),
                    },
                )
            })
            .collect()
    }

    fn hydrate(exported_filters: ExportedBloomFilters) -> Filters {
        exported_filters
            .into_iter()
            .map(|(key, exported)| {
                (
                    key.clone(),
                    Bloom::from_existing(
                        &exported.bitmap,
                        exported.bitmap_bits,
                        exported.k_num,
                        exported.sip_keys,
                    ),
                )
            })
            .collect()
    }
}
