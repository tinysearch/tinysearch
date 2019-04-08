use cuckoofilter::{self, CuckooFilter};
use walkdir::{DirEntry, WalkDir};

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use strip_markdown::strip_markdown;

#[path = "src/types.rs"]
mod types;

use crate::types::Storage;

fn main() -> Result<(), Box<Error>> {
    let input_dir = env::var("INPUT_DIR")?;
    let filters = build(input_dir)?;
    println!("Storage::from");
    let storage = Storage::from(filters);
    println!("Write");
    fs::write("storage", storage.to_bytes()?)?;
    println!("ok");
    Ok(())
}

fn build(corpus_path: String) -> Result<HashMap<PathBuf, CuckooFilter<DefaultHasher>>, Box<Error>> {
    println!("{}", corpus_path);
    let posts = prepare_posts(corpus_path)?;
    generate_filters(posts)
}

/// Remove non-ascii characters from string
fn cleanup(s: String) -> String {
    s.replace(|c: char| !c.is_alphabetic(), " ")
}

// Read all posts and generate Bloomfilters from them.
#[no_mangle]
pub fn generate_filters(
    posts: HashMap<PathBuf, String>,
) -> Result<HashMap<PathBuf, CuckooFilter<DefaultHasher>>, Box<Error>> {
    // Create a dictionary of {"post name": "lowercase word set"}. split_posts =
    // {name: set(re.split("\W+", contents.lower())) for name, contents in
    // posts.items()}
    println!("Generate filters");

    let bytes = include_bytes!("stopwords");
    let stopwords = String::from_utf8(bytes.to_vec())?;
    let STOPWORDS: HashSet<String> = stopwords.split_whitespace().map(String::from).collect();

    let split_posts: HashMap<PathBuf, HashSet<String>> = posts
        .into_iter()
        .map(|(post, content)| {
            println!("Generating {:?}", post);
            (
                post,
                cleanup(strip_markdown(&content))
                    .split_whitespace()
                    .map(str::to_lowercase)
                    .filter(|word| !STOPWORDS.contains(word))
                    .collect::<HashSet<String>>(),
            )
        })
        .collect();

    // At this point, we have a dictionary of posts and a normalized set of
    // words in each. We could do more things, like stemming, removing common
    // words (a, the, etc), but we’re going for naive, so let’s just create the
    // filters for now:
    let mut filters = HashMap::new();
    for (name, words) in split_posts {
        // Adding some more padding to the capacity because sometimes there is an error
        // about not having enough space. Not sure why that happens, though.
        let mut filter = cuckoofilter::CuckooFilter::with_capacity(words.len() + 4);
        for word in words {
            println!("{}", word);
            filter.add(&word)?;
        }
        filters.insert(name, filter);
    }
    println!("Done");
    Ok(filters)
}

fn is_markdown(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.ends_with(".md"))
        .unwrap_or(false)
}

// prepares the files in the given directory to be consumed by the generator
pub fn prepare_posts(dir: String) -> Result<HashMap<PathBuf, String>, Box<Error>> {
    let mut posts: HashMap<PathBuf, String> = HashMap::new();
    let walker = WalkDir::new(dir).into_iter();
    for entry in walker.filter_map(Result::ok).filter(|e| is_markdown(e)) {
        println!("Analyzing {}", entry.path().display());
        let mut post = File::open(entry.path())?;
        let mut contents = String::new();
        post.read_to_string(&mut contents)?;
        posts.insert(entry.into_path(), contents);
    }
    Ok(posts)
}
