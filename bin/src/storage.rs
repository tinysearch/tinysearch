use strip_markdown::strip_markdown;

use failure::Error;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;

use crate::index::Posts;
use tinysearch_cuckoofilter::{self, CuckooFilter};
use tinysearch_shared::{PostId, Storage};

pub fn gen(posts: Posts) -> Result<(), Error> {
    let filters = build(posts)?;
    println!("Storage::from");
    let storage = Storage::from(filters);
    println!("Write");
    fs::write("storage", storage.to_bytes()?)?;
    println!("ok");
    Ok(())
}

fn build(posts: Posts) -> Result<HashMap<PostId, CuckooFilter<DefaultHasher>>, Error> {
    let posts = prepare_posts(posts);
    generate_filters(posts)
}

/// Remove non-ascii characters from string
fn cleanup(s: String) -> String {
    s.replace(|c: char| !c.is_alphabetic(), " ")
}

// Read all posts and generate Bloomfilters from them.
#[no_mangle]
pub fn generate_filters(
    posts: HashMap<PostId, String>,
) -> Result<HashMap<PostId, CuckooFilter<DefaultHasher>>, Error> {
    // Create a dictionary of {"post name": "lowercase word set"}. split_posts =
    // {name: set(re.split("\W+", contents.lower())) for name, contents in
    // posts.items()}
    println!("Generate filters");

    let bytes = include_bytes!("../stopwords");
    let stopwords = String::from_utf8(bytes.to_vec())?;
    let stopwords: HashSet<String> = stopwords.split_whitespace().map(String::from).collect();

    let split_posts: HashMap<PostId, HashSet<String>> = posts
        .into_iter()
        .map(|(post, content)| {
            println!("Generating {:?}", post);
            (
                post,
                cleanup(strip_markdown(&content))
                    .split_whitespace()
                    .map(str::to_lowercase)
                    .filter(|word| !stopwords.contains(word))
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
        let mut filter = CuckooFilter::with_capacity(words.len() + 4);
        for word in words {
            println!("{}", word);
            filter.add(&word)?;
        }
        for word in name.0.split_whitespace() {
            filter.add(&cleanup(strip_markdown(word)))?;
        }
        filters.insert(name, filter);
    }
    println!("Done");
    Ok(filters)
}

// prepares the files in the given directory to be consumed by the generator
pub fn prepare_posts(posts: Posts) -> HashMap<PostId, String> {
    let mut prepared: HashMap<PostId, String> = HashMap::new();
    for post in posts {
        println!("Analyzing {}", post.url);
        prepared.insert((post.title, post.url), post.body);
    }
    prepared
}
