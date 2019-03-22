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
    let bytes = include_bytes!("../storage");
    Ok(Storage::from_bytes(bytes)?.filters)
}

lazy_static! {
    static ref FILTERS: Filters = load_filters().unwrap();
}

use wasm_bindgen::prelude::*;

// Called when the wasm module is instantiated
#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    // Use `web_sys`'s global `window` function to get a handle on the global
    // window object.
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let body = document.body().expect("document should have a body");

    // Manufacture the element we're gonna append
    let val = document.create_element("p")?;
    val.set_inner_html("Hello from Rust!");

    body.append_child(&val)?;

    Ok(())
}

#[wasm_bindgen]
pub fn search(query: String) -> String {
    let search_terms: HashSet<String> =
        query.split_whitespace().map(|s| s.to_lowercase()).collect();

    let matches: Vec<PathBuf> = FILTERS
        .iter()
        .filter(|&(_, ref filter)| search_terms.iter().all(|term| filter.contains(term)))
        .map(|(name, _)| name.to_owned())
        .collect();
    // String::from(matches.len())
    // 3.to_string()
    // String::from("hello")
    serde_json::to_string(&matches).unwrap_or_else(|_| "{}".to_string())
}
