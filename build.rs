use cuckoofilter::{self, CuckooFilter, ExportedCuckooFilter};
use bincode::{serialize, deserialize};

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::error::Error;

fn main() -> Result<(), Box<Error>> {
    let filters = build(".".to_string())?;
    // fs::write("filters", filters)?;
    // Ok(())

    // let words = vec!["foo", "bar", "xylophone", "milagro"];
    // let mut filter = cuckoofilter::CuckooFilter::new();

    // let mut insertions = 0;
    // for s in &words {
    //     filter.add(s).unwrap();
    //     insertions += 1;
    // }

    // Export the fingerprint data stored in the filter,
    // along with the filter's current length.
    // let store: ExportedCuckooFilter = filter.export();
    // let encoded: Vec<u8> = serialize(&store).unwrap();
    let encoded: Vec<u8> = serialize(&filters).unwrap();
    fs::write("store", encoded);
    Ok(())
}

fn build(
    corpus_path: String,
) -> Result<HashMap<OsString, CuckooFilter<std::collections::hash_map::DefaultHasher>>, Box<Error>>
{
    let posts = prepare_posts(corpus_path)?;
    generate_filters(posts)
}

// Read all posts and generate Bloomfilters from them.
#[no_mangle]
pub fn generate_filters(
    posts: HashMap<OsString, String>,
) -> Result<HashMap<OsString, CuckooFilter<std::collections::hash_map::DefaultHasher>>, Box<Error>>
{
    // Create a dictionary of {"post name": "lowercase word set"}. split_posts =
    // {name: set(re.split("\W+", contents.lower())) for name, contents in
    // posts.items()}
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

    // At this point, we have a dictionary of posts and a normalized set of
    // words in each. We could do more things, like stemming, removing common
    // words (a, the, etc), but we’re going for naive, so let’s just create the
    // filters for now:
    let mut filters = HashMap::new();
    for (name, words) in split_posts {
        // let mut filter = Cuckoofilter::with_capacity(words.len() as u32);
        let mut filter = cuckoofilter::CuckooFilter::new();
        for word in words {
            filter.add(&word);
        }
        filters.insert(name, filter);
    }
    Ok(filters)
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
