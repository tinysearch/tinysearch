use once_cell::sync::Lazy;

#[cfg(feature = "bind")]
use serde_wasm_bindgen;
#[cfg(feature = "bind")]
use wasm_bindgen::prelude::*;

use tinysearch::{search as base_search, Filters, PostId, Storage};

#[cfg(feature = "bind")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

static FILTERS: Lazy<Filters> = Lazy::new(|| {
    let bytes = include_bytes!("storage");
    Storage::from_bytes(bytes).unwrap().filters
});

pub fn search_local(query: String, num_results: usize) -> Vec<&'static PostId> {
    base_search(&FILTERS, query, num_results)
}

#[cfg(feature = "bind")]
#[wasm_bindgen]
pub fn search(query: String, num_results: usize) -> JsValue {
    serde_wasm_bindgen::to_value(&search_local(query, num_results))
        .expect("failed to serialize search result")
}
