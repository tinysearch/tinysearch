#[macro_use]
extern crate serde_derive;

extern crate structopt;
#[macro_use]
extern crate structopt_derive;
#[macro_use]
extern crate lazy_static;

use bincode::{deserialize, serialize};
use cuckoofilter::{self, CuckooFilter, ExportedCuckooFilter};
use structopt::StructOpt;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process;

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

fn main() {
    let opt = Opt::from_args();
    if let Err(err) = run(&opt.search_terms) {
        eprintln!("Command failed:\n{}\n", err);
        process::exit(1);
    }
}

fn run(search_terms: &str) -> Result<(), Box<Error>> {
    let matches = search(search_terms);
    println!("Found the following matches: {:#?}", matches);
    Ok(())
}

#[no_mangle]
pub fn search(query: &str) -> Vec<PathBuf> {
    let search_terms: HashSet<String> =
        query.split_whitespace().map(|s| s.to_lowercase()).collect();

    FILTERS
        .into_iter()
        .filter(|&(_, ref filter)| search_terms.iter().all(|term| filter.contains(term)))
        .map(|(name, _)| name)
        .collect()
}
