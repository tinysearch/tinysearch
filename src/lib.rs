#[macro_use]
extern crate lazy_static;

use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

mod types;
use types::{Filters, Storage};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

fn load_filters() -> Result<Filters, Box<Error>> {
    let bytes = include_bytes!("../storage");
    Ok(Storage::from_bytes(bytes)?.filters)
}

lazy_static! {
    static ref FILTERS: Filters = load_filters().unwrap();
}

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn search(query: String) -> String {
    let search_terms: HashSet<String> =
        query.split_whitespace().map(|s| s.to_lowercase()).collect();

    let matches: Vec<PathBuf> = FILTERS
        .iter()
        .filter(|&(_, ref filter)| search_terms.iter().all(|term| filter.contains(term)))
        .map(|(name, _)| name.to_owned())
        .collect();
    serde_json::to_string(&matches).unwrap_or_else(|_| "{}".to_string())
}
