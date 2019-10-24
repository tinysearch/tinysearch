use js_sys::Uint8Array;
use std::cmp::Reverse;
use tinysearch_shared::{Filters, PostId, Score, Storage};
use wasm_bindgen::prelude::*;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
#[derive(Default)]
pub struct TinySearch {
    filters: Option<Filters>,
}

#[wasm_bindgen]
impl TinySearch {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        TinySearch::default()
    }

    #[wasm_bindgen(js_name = "loadFilters")]
    pub fn load_filters(&mut self, data: Uint8Array) -> Result<(), JsValue> {
        let mut buffer: Vec<u8> = vec![0; data.byte_length() as usize];
        data.copy_to(&mut buffer);
        self.filters = Some(
            Storage::from_bytes(&buffer)
                .map_err(|e| e.to_string())?
                .filters,
        );
        Ok(())
    }

    #[wasm_bindgen]
    pub fn search(&self, query: String, num_results: usize) -> JsValue {
        if self.filters.is_none() {
            return JsValue::from_serde(&Vec::<Vec<&PostId>>::new()).unwrap();
        }

        let filters = self.filters.as_ref().unwrap();

        let search_terms: Vec<String> =
            query.split_whitespace().map(|s| s.to_lowercase()).collect();

        let mut matches: Vec<(&PostId, u32)> = filters
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
}
