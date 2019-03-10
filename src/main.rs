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
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process;

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(help = "Search terms")]
    search_terms: String,
}

fn load_filters() -> CuckooFilter<DefaultHasher> {
    let raw = fs::read("store").unwrap();
    let decoded: ExportedCuckooFilter = deserialize(&raw[..]).unwrap();
    let recovered_filter = CuckooFilter::<DefaultHasher>::from(decoded);
    recovered_filter
}

lazy_static! {
        // static ref FILTERS: HashMap<OsString, CuckooFilter<std::collections::hash_map::DefaultHasher>>> =
        static ref FILTERS: CuckooFilter<std::collections::hash_map::DefaultHasher> = load_filters();
}

fn main() {
    let opt = Opt::from_args();
    if let Err(err) = run(&opt.search_terms) {
        eprintln!("Command failed:\n{}\n", err);
        process::exit(1);
    }
}

fn run(search_terms: &str) -> Result<(), Box<Error>> {
    let matches = search(search_terms, FILTERS);
    println!("Found the following matches: {:#?}", matches);
    Ok(())
}

#[no_mangle]
pub fn search(
    query: &str,
    filters: HashMap<OsString, CuckooFilter<std::collections::hash_map::DefaultHasher>>,
) -> Vec<OsString> {
    let search_terms: HashSet<String> =
        query.split_whitespace().map(|s| s.to_lowercase()).collect();

    filters
        .into_iter()
        .filter(|&(_, ref filter)| search_terms.iter().all(|term| filter.contains(term)))
        .map(|(name, _)| name)
        .collect()
}
