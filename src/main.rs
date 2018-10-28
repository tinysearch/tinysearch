extern crate bloom;

extern crate structopt;
#[macro_use]
extern crate structopt_derive;

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use bloom::{BloomFilter, ASMS};
use std::error::Error;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(help = "Path to input files (search corpus)")]
    corpus_path: String,

    #[structopt(help = "Search terms")]
    search_terms: String,
}

fn main() {
    let opt = Opt::from_args();
    // TODO: Proper error handling
    run(opt.corpus_path, &opt.search_terms).unwrap();
}

fn run(corpus_path: String, search_terms: &str) -> Result<(), Box<Error>> {
    let posts = prepare_posts(corpus_path)?;
    let filters = generate(posts)?;
    let matches = search(search_terms, filters);
    println!("Found the following matches: {:#?}", matches);
    Ok(())
}

// def search(search_string):
//    search_terms = re.split("\W+", search_string)
//    return [name for name, filter in filters.items() if all(term in filter for term in search_terms)]
#[no_mangle]
pub fn search(query: &str, filters: HashMap<OsString, BloomFilter>) -> Vec<OsString> {
    let search_terms: HashSet<String> =
        query.split_whitespace().map(|s| s.to_lowercase()).collect();

    filters
        .into_iter()
        .filter(|&(_, ref filter)| search_terms.iter().all(|term| filter.contains(term)))
        .map(|(name, _)| name)
        .collect()
}

// prepares the files in the given directory to be consumed by the generator
pub fn prepare_posts(dir: String) -> Result<HashMap<OsString, String>, Box<Error>> {
    let paths: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|f| f.path())
        .collect();

    let mut posts: HashMap<OsString, String> = HashMap::new();
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let mut post = File::open(&path)?;
        let mut contents = String::new();
        post.read_to_string(&mut contents)?;
        posts.insert(
            path.file_name().ok_or("Not a file")?.to_os_string(),
            contents,
        );
    }

    Ok(posts)
}

// Read all my posts.
// # posts = {post_name: open(POST_DIR + post_name).read() for post_name in os.listdir(POST_DIR)}
#[no_mangle]
pub fn generate(posts: HashMap<OsString, String>) -> Result<HashMap<OsString, BloomFilter>, Box<Error>> {
    // Create a dictionary of {"post name": "lowercase word set"}.
    // split_posts = {name: set(re.split("\W+", contents.lower())) for name, contents in posts.items()}
    let split_posts: HashMap<OsString, HashSet<String>> = posts
        .into_iter()
        .map(|(post, content)| {
            (
                post,
                content
                    .split_whitespace()
                    .map(str::to_lowercase)
                    .collect::<HashSet<String>>(),
            )
        })
        .collect();

    // At this point, we have a dictionary of posts and a normalized set of words in each.
    // We could do more things, like stemming, removing common words (a, the, etc), but
    // we’re going for naive, so let’s just create the filters for now:
    let mut filters = HashMap::new();
    for (name, words) in split_posts {
        let mut filter: BloomFilter = BloomFilter::with_rate(0.01, words.len() as u32);
        for word in words {
            filter.insert(&word);
        }
        filters.insert(name, filter);
    }
    Ok(filters)
}
