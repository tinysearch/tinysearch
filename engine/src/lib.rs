#[macro_use]
extern crate lazy_static;

use std::cmp::Reverse;
use std::error::Error;
use tinysearch_shared::{Filters, PostId, Score, Storage};
use wasm_bindgen::prelude::*;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

fn load_filters() -> Result<Filters, Box<dyn Error>> {
    let bytes = include_bytes!("../storage");
    Ok(Storage::from_bytes(bytes)?.filters)
}

lazy_static! {
    static ref FILTERS: Filters = load_filters().unwrap();
}

#[wasm_bindgen]
pub fn search(query: String, num_results: usize) -> JsValue {
    let search_terms: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

    let mut matches: Vec<(&PostId, u32)> = FILTERS
        .iter()
        .map(|(name, filter)| (name, filter.score(&search_terms)))
        .filter(|(_, score)| *score > 0)
        .collect();

    matches.sort_by_key(|k| Reverse(k.1));

    let results: Vec<&PostId> = matches
        .iter()
        .map(|(name, _)| name.to_owned())
        .take(num_results)
        .collect();

    JsValue::from_serde(&results).unwrap()
}
