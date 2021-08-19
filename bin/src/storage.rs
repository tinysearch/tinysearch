use anyhow::Error;
use std::collections::{HashMap, HashSet};
use std::fs;

use crate::index::Posts;
use strip_markdown::strip_markdown;
use tinysearch_shared::{Filters, PostId, Storage};
use xorf::HashProxy;

pub fn write(posts: Posts) -> Result<(), Error> {
    let filters = build(posts)?;
    trace!("Storage::from");
    let storage = Storage::from(filters);
    trace!("Write");
    fs::write("storage", storage.to_bytes()?)?;
    trace!("ok");
    Ok(())
}

fn build(posts: Posts) -> Result<Filters, Error> {
    let posts = prepare_posts(posts);
    generate_filters(posts)
}

/// Remove non-ascii characters from string
/// Keep apostrophe (e.g. for words like "don't")
fn cleanup(s: String) -> String {
    s.replace(|c: char| !(c.is_alphabetic() || c == '\''), " ")
}

fn tokenize(words: &str, stopwords: &HashSet<String>) -> HashSet<String> {
    cleanup(strip_markdown(&words))
        .split_whitespace()
        .filter(|&word| !word.trim().is_empty())
        .map(str::to_lowercase)
        .filter(|word| !stopwords.contains(word))
        .collect()
}

// Read all posts and generate Bloomfilters from them.
#[no_mangle]
pub fn generate_filters(posts: HashMap<PostId, Option<String>>) -> Result<Filters, Error> {
    // Create a dictionary of {"post name": "lowercase word set"}. split_posts =
    // {name: set(re.split("\W+", contents.lower())) for name, contents in
    // posts.items()}
    debug!("Generate filters");

    let stopwords: &str = include_str!("../assets/stopwords");
    let stopwords: HashSet<String> = stopwords.split_whitespace().map(String::from).collect();

    let split_posts: HashMap<PostId, Option<HashSet<String>>> = posts
        .into_iter()
        .map(|(post, content)| {
            debug!("Generating {:?}", post);
            (post, content.map(|content| tokenize(&content, &stopwords)))
        })
        .collect();

    // At this point, we have a dictionary of posts and a normalized set of
    // words in each. We could do more things, like stemming, removing common
    // words (a, the, etc), but we’re going for naive, so let’s just create the
    // filters for now:
    let mut filters = Vec::new();
    for (post_id, body) in split_posts {
        // Also add title to filter
        let title: HashSet<String> = tokenize(&post_id.0, &stopwords);
        let content: Vec<String> = if let Some(body) = body {
            body.union(&title).cloned().collect()
        } else {
            title.into_iter().collect()
        };
        let filter = HashProxy::from(&content);
        filters.push((post_id, filter));
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
    use xorf::Filter;

    use super::*;

    #[test]
    fn test_generate_filters() {
        let mut posts = HashMap::new();
        posts.insert(
            (
                "Maybe You Don't Need Kubernetes, Or Excel - You Know".to_string(),
                "".to_string(),
            ),
            None,
        );
        let filters = generate_filters(posts).unwrap();
        assert_eq!(filters.len(), 1);
        let (_post_id, filter) = filters.first().unwrap();

        assert!(!filter.contains(&" ".to_owned()));
        assert!(!filter.contains(&"    ".to_owned()));
        assert!(!filter.contains(&"foo".to_owned()));
        assert!(!filter.contains(&"-".to_owned()));
        assert!(!filter.contains(&",".to_owned()));
        assert!(!filter.contains(&"'".to_owned()));

        // "you", "don't", and "need" get stripped out because they are stopwords
        assert!(!filter.contains(&"you".to_owned()));
        assert!(!filter.contains(&"don't".to_owned()));
        assert!(!filter.contains(&"need".to_owned()));

        assert!(filter.contains(&"maybe".to_owned()));
        assert!(filter.contains(&"kubernetes".to_owned()));
        assert!(filter.contains(&"excel".to_owned()));
    }
}
