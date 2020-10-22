use anyhow::Error;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;

use crate::index::Posts;
use crate::strip_markdown::strip_markdown;
use cuckoofilter::{self, CuckooFilter};
use tinysearch_shared::{PostId, Storage};

pub fn gen(posts: Posts) -> Result<(), Error> {
    let filters = build(posts)?;
    trace!("Storage::from");
    let storage = Storage::from(filters);
    trace!("Write");
    fs::write("storage", storage.to_bytes()?)?;
    trace!("ok");
    Ok(())
}

fn build(posts: Posts) -> Result<Vec<(PostId, CuckooFilter<DefaultHasher>)>, Error> {
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
    posts: HashMap<PostId, Option<String>>,
) -> Result<Vec<(PostId, CuckooFilter<DefaultHasher>)>, Error> {
    // Create a dictionary of {"post name": "lowercase word set"}. split_posts =
    // {name: set(re.split("\W+", contents.lower())) for name, contents in
    // posts.items()}
    debug!("Generate filters");

    let bytes = include_bytes!("../assets/stopwords");
    let stopwords = String::from_utf8(bytes.to_vec())?;
    let stopwords: HashSet<String> = stopwords.split_whitespace().map(String::from).collect();

    let split_posts: HashMap<PostId, Option<HashSet<String>>> = posts
        .into_iter()
        .map(|(post, content)| {
            debug!("Generating {:?}", post);
            (
                post,
                content.map(|content| {
                    cleanup(strip_markdown(&content))
                        .split_whitespace()
                        .map(str::to_lowercase)
                        .filter(|word| !stopwords.contains(word))
                        .collect::<HashSet<String>>()
                }),
            )
        })
        .collect();

    // At this point, we have a dictionary of posts and a normalized set of
    // words in each. We could do more things, like stemming, removing common
    // words (a, the, etc), but we’re going for naive, so let’s just create the
    // filters for now:
    let mut filters = Vec::new();
    for (name, words) in split_posts {
        let capacity = words.as_ref().map(|words| words.len()).unwrap_or(0);
        // Adding some more padding to the capacity because sometimes there is an error
        // about not having enough space. Not sure why that happens, though.
        let mut filter = CuckooFilter::with_capacity(capacity + 64);
        if let Some(words) = words {
            for word in words {
                trace!("{}", word);
                filter.add(&word)?;
            }
        }
        for word in name.0.split_whitespace().map(str::to_lowercase) {
            let word = &cleanup(strip_markdown(&word));
            trace!("{}", word);
            filter.add(word)?;
        }
        filters.push((name, filter));
    }
    trace!("Done");
    Ok(filters)
}

// prepares the files in the given directory to be consumed by the generator
pub fn prepare_posts(posts: Posts) -> HashMap<PostId, Option<String>> {
    let mut prepared: HashMap<PostId, Option<String>> = HashMap::new();
    for post in posts {
        debug!("Analyzing {}", post.url);
        prepared.insert((post.title, post.url), post.body);
    }
    prepared
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_filters() {
        let mut posts = HashMap::new();
        posts.insert(
            (
                "Maybe You Don't Need Kubernetes".to_string(),
                "".to_string(),
            ),
            Some("Excel Unreasonable".to_string()),
        );
        let filters = generate_filters(posts).unwrap();
        assert_eq!(filters.len(), 1);
        let (_, filter) = filters.iter().nth(0).unwrap();

        // "you", "don't", and "need" get stripped out because they are stopwords
        assert!(filter.contains("maybe"));
        assert!(filter.contains("kubernetes"));
        assert!(filter.contains("excel"));
        assert!(filter.contains("unreasonable"));
    }

    #[test]
    fn test_generate_filters_empty_body() {
        let mut posts = HashMap::new();
        posts.insert(
            (
                "Maybe You Don't Need Kubernetes Excel Unreasonable".to_string(),
                "".to_string(),
            ),
            None,
        );
        let filters = generate_filters(posts).unwrap();
        assert_eq!(filters.len(), 1);
        let (_, filter) = filters.iter().nth(0).unwrap();

        // "you", "don't", and "need" get stripped out because they are stopwords
        assert!(filter.contains("maybe"));
        assert!(filter.contains("kubernetes"));
        assert!(filter.contains("excel"));
        assert!(filter.contains("unreasonable"));
    }
}
