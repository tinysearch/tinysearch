#[macro_use]
extern crate lazy_static;

use wasm_bindgen::prelude::*;

use structopt::StructOpt;

use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

mod types;
use types::{Filters, Storage};

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(help = "Search terms")]
    search_terms: String,
}

fn load_filters() -> Result<Filters, Box<Error>> {
    let bytes = fs::read("storage").unwrap();
    Ok(Storage::from_bytes(&bytes)?.filters)
}

lazy_static! {
        // static ref FILTERS: HashMap<PathBuf, CuckooFilter<std::collections::hash_map::DefaultHasher>>> =
        static ref FILTERS: Filters = load_filters().unwrap();
}

#[wasm_bindgen]
pub fn search(query: &str) -> String {
    let search_terms: HashSet<String> =
        query.split_whitespace().map(|s| s.to_lowercase()).collect();

    let matches: Vec<PathBuf> = FILTERS
        .iter()
        .filter(|&(_, ref filter)| search_terms.iter().all(|term| filter.contains(term)))
        .map(|(name, _)| name.to_owned())
        .collect();
    serde_json::to_string(&matches).unwrap_or_else(|_| "{}".to_string())
}
