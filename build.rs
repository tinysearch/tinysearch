use walkdir::{DirEntry, WalkDir};
use bloomfilter::Bloom;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[path = "src/types.rs"]
mod types;

use crate::types::Storage;

fn main() -> Result<(), Box<Error>> {
    let input_dir = env::var("INPUT_DIR")?;
    let filters = build(input_dir)?;
    let storage = Storage::from(filters);
    fs::write("storage", storage.to_bytes()?)?;
    Ok(())
}

fn build(corpus_path: String) -> Result<HashMap<PathBuf, Bloom>, Box<Error>> {
    println!("{}", corpus_path);
    let posts = prepare_posts(corpus_path)?;
    generate_filters(posts)
}

// Read all posts and generate Bloomfilters from them.
// Create a dictionary of {"post name": "lowercase word set"}. 
// split_posts = {name: set(re.split("\W+", contents.lower())) for name, contents in posts.items()}
#[no_mangle]
pub fn generate_filters(
    posts: HashMap<PathBuf, String>,
) -> Result<HashMap<PathBuf, Bloom>, Box<Error>> {
    println!("Generate filters");
    let split_posts: HashMap<PathBuf, HashSet<String>> = posts
        .into_iter()
        .map(|(post, content)| {
            println!("Generating {:?}", post);
            (
                post,
                content
                    .split_whitespace()
                    .map(str::to_lowercase)
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
        let mut filter = Bloom::new_for_fp_rate(1000, 0.05);
        for word in words {
            println!("{}", word);
            filter.set(&word);
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
