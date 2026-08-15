//! Differential and wire-validation tests for the sharded exact index.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::missing_docs_in_private_items,
    clippy::panic,
    clippy::unwrap_used
)]

use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use tinysearch::{
    BasicPost, ExactIndexBackend, IndexBackend, IndexedDocument, PostId, ShardConfig, ShardError,
    ShardId, ShardedIndex, TinySearch, Xor8IndexBackend, search,
};

const TARGET_BYTES: usize = 48;
const ROOT_MAGIC: &[u8] = b"tinysearch-sharded-root";
const SHARD_MAGIC: &[u8] = b"tinysearch-shard";
const ROOT_VERSION: u8 = 1;
const SHARD_VERSION: u8 = 1;
const SHARD_SUFFIX: &str = ".tinysearch-shard";

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn tokenize(text: &str) -> Vec<String> {
    text.replace(
        |character: char| !(character.is_alphabetic() || character == '\''),
        " ",
    )
    .split_whitespace()
    .map(str::to_lowercase)
    .collect()
}

fn skewed_corpus() -> (tinysearch::SearchIndex, Vec<String>) {
    let titles = [
        "Rust Résumé",
        "Rustacean Notes",
        "Go Handbook",
        "Café Culture",
        "Über Guide",
        "Tokyo 東京",
        "A Small Post",
        "Éclair Notes",
        "Ångström Scale",
        "Don't Panic",
        "Punctuation & Unicode",
        "Oversized Entry",
    ];
    let special_terms: [&[&str]; 12] = [
        &["program", "programmer", "programming", "a"],
        &["programming", "an"],
        &["go", "rust"],
        &["café", "co", "don't"],
        &["über", "éclair"],
        &["東京", "ångström"],
        &["café", "résumé"],
        &["über", "東京"],
        &["don't", "go"],
        &["a", "an", "co"],
        &["punctuation", "unicode"],
        &[],
    ];

    let mut all_terms = BTreeSet::new();
    let mut documents = Vec::new();
    for (number, (title, extras)) in titles.iter().zip(special_terms).enumerate() {
        let mut terms = vec!["common".to_string(), format!("prefix{number:02}")];
        terms.extend(extras.iter().map(ToString::to_string));
        if number == titles.len() - 1 {
            terms.push("z".repeat(180));
        }
        all_terms.extend(terms.iter().cloned());
        all_terms.extend(tokenize(title));
        documents.push(IndexedDocument::new(
            PostId {
                title: (*title).to_string(),
                url: format!("/doc-{number:02}"),
                meta: format!("{{\"ordinal\":{number}}}"),
            },
            terms,
        ));
    }
    (
        ExactIndexBackend.build(documents),
        all_terms.into_iter().collect(),
    )
}

fn config() -> ShardConfig {
    ShardConfig::new(TARGET_BYTES).expect("nonzero target")
}

fn urls<'post>(posts: &[&'post PostId]) -> Vec<&'post str> {
    posts.iter().map(|post| post.url.as_str()).collect()
}

fn fully_loaded(index: &tinysearch::SearchIndex) -> ShardedIndex {
    let bundle = index
        .to_sharded_bundle(config())
        .expect("exact index should shard");
    let mut sharded =
        ShardedIndex::from_root_bytes(bundle.root_bytes()).expect("root should decode");
    for artifact in bundle.shards() {
        sharded
            .load_shard(artifact.bytes())
            .expect("generated shard should load");
    }
    sharded
}

fn unicode_prefixes(term: &str) -> Vec<&str> {
    term.char_indices()
        .map(|(position, _character)| position)
        .skip(1)
        .chain(std::iter::once(term.len()))
        .map(|end| &term[..end])
        .collect()
}

fn assert_same_results(
    monolithic: &tinysearch::SearchIndex,
    sharded: &ShardedIndex,
    query: &str,
    limit: usize,
) {
    let expected = search(monolithic, query, limit);
    let actual = sharded
        .search(query, limit)
        .unwrap_or_else(|error| panic!("sharded search failed for {query:?}: {error}"));
    assert_eq!(
        urls(&actual),
        urls(&expected),
        "ordered URL mismatch for query {query:?}, limit {limit}"
    );
}

#[test]
fn defaults_validation_and_backend_support_are_explicit() {
    assert_eq!(ShardConfig::default().target_bytes(), 64 * 1024);
    assert!(matches!(
        ShardConfig::new(0),
        Err(ShardError::InvalidTargetBytes)
    ));

    let xor = Xor8IndexBackend.build(vec![IndexedDocument::new(
        PostId {
            title: "Xor".to_string(),
            url: "/xor".to_string(),
            meta: String::new(),
        },
        ["term".to_string()],
    )]);
    assert!(matches!(
        xor.to_sharded_bundle(ShardConfig::default()),
        Err(ShardError::UnsupportedBackend)
    ));

    let title_only = ExactIndexBackend.build(vec![IndexedDocument::new(
        PostId {
            title: "Headline Only".to_string(),
            url: "/headline".to_string(),
            meta: String::new(),
        },
        ["headline".to_string(), "only".to_string()],
    )]);
    let title_bundle = title_only
        .to_sharded_bundle(config())
        .expect("title-only index should shard");
    assert!(title_bundle.shards().is_empty());
    let title_index =
        ShardedIndex::from_root_bytes(title_bundle.root_bytes()).expect("title root should decode");
    assert!(title_index.required_shards("headline").is_empty());
    assert_eq!(
        urls(&title_index.search("headline", 5).expect("root-only search")),
        ["/headline"]
    );

    let empty = ExactIndexBackend.build(Vec::new());
    let empty_bundle = empty
        .to_sharded_bundle(config())
        .expect("empty exact index should shard");
    assert!(empty_bundle.shards().is_empty());
    let empty_index =
        ShardedIndex::from_root_bytes(empty_bundle.root_bytes()).expect("empty root should decode");
    assert!(
        empty_index
            .search("anything", 5)
            .expect("empty search")
            .is_empty()
    );
}

#[test]
fn sharded_search_matches_monolithic_for_terms_prefixes_and_limits() {
    let (monolithic, terms) = skewed_corpus();
    let sharded = fully_loaded(&monolithic);
    let limits = [0, 1, 2, 5, 50];
    let mut queries = BTreeSet::new();

    for term in &terms {
        queries.insert(term.clone());
        for prefix in unicode_prefixes(term) {
            queries.insert(prefix.to_string());
        }
    }
    queries.extend([
        "café!!!".to_string(),
        "RUST, programming".to_string(),
        "don't?".to_string(),
        "東京。".to_string(),
        "go".to_string(),
        "a".to_string(),
        "an".to_string(),
        "pro pro".to_string(),
        "program café 東京".to_string(),
        "prefix00 prefix11 prefix00".to_string(),
        "!!!".to_string(),
    ]);

    for query in queries {
        for limit in limits {
            assert_same_results(&monolithic, &sharded, &query, limit);
        }
    }
}

#[test]
fn plans_fanout_and_supports_incremental_loading() -> TestResult {
    let (monolithic, _terms) = skewed_corpus();
    let bundle = monolithic.to_sharded_bundle(config())?;
    let mut sharded = ShardedIndex::from_root_bytes(bundle.root_bytes())?;
    let required = sharded.required_shards("pre prefix00 pre");
    assert!(
        required.len() > 2,
        "expected prefix fanout, got {required:?}"
    );
    assert!(required.windows(2).all(|pair| pair[0] < pair[1]));

    let error = sharded.search("pre prefix00 pre", 10).unwrap_err();
    let ShardError::NeedsShards(initial_missing) = error else {
        panic!("expected NeedsShards, got {error}");
    };
    assert_eq!(initial_missing, required);

    let first = required[0];
    let first_artifact = bundle
        .shards()
        .iter()
        .find(|artifact| artifact.descriptor().id == first)
        .expect("required artifact");
    sharded.load_shard(first_artifact.bytes())?;
    let ShardError::NeedsShards(after_one) = sharded.search("pre prefix00 pre", 10).unwrap_err()
    else {
        panic!("query unexpectedly succeeded with incomplete fanout");
    };
    assert_eq!(after_one, required[1..]);

    for id in &required[1..] {
        let artifact = bundle
            .shards()
            .iter()
            .find(|artifact| artifact.descriptor().id == *id)
            .expect("required artifact");
        sharded.load_shard(artifact.bytes())?;
    }
    assert_same_results(&monolithic, &sharded, "pre prefix00 pre", 10);
    assert_eq!(sharded.loaded_shard_count(), required.len());
    let expected_bytes: usize = bundle
        .shards()
        .iter()
        .filter(|artifact| required.contains(&artifact.descriptor().id))
        .map(|artifact| artifact.bytes().len())
        .sum();
    assert_eq!(sharded.loaded_shard_bytes(), expected_bytes);
    Ok(())
}

#[test]
fn prefix_crosses_boundaries_and_content_scores_once_per_query_token() -> TestResult {
    let (monolithic, _terms) = skewed_corpus();
    let bundle = monolithic.to_sharded_bundle(config())?;
    let required = ShardedIndex::from_root_bytes(bundle.root_bytes())?.required_shards("pro");
    assert!(
        required.len() >= 2,
        "program terms should cross shard boundaries: {required:?}"
    );

    let sharded = fully_loaded(&monolithic);
    assert_same_results(&monolithic, &sharded, "pro", 50);
    let results = sharded.search("pro", 50)?;
    assert_eq!(urls(&results), ["/doc-00", "/doc-01"]);

    let repeated = sharded.search("pro pro", 50)?;
    assert_eq!(urls(&repeated), ["/doc-00", "/doc-01"]);
    Ok(())
}

#[test]
fn public_build_order_does_not_change_content_addressed_artifacts() -> TestResult {
    let posts = vec![
        BasicPost {
            title: "Second".to_string(),
            url: "/second".to_string(),
            body: Some("beta shared".to_string()),
            meta: HashMap::new(),
        },
        BasicPost {
            title: "First".to_string(),
            url: "/first".to_string(),
            body: Some("alpha shared".to_string()),
            meta: HashMap::new(),
        },
    ];
    let search = TinySearch::new();
    let forward = search.build_index(&posts)?.to_sharded_bundle(config())?;
    let reverse_posts: Vec<BasicPost> = posts.into_iter().rev().collect();
    let reverse = search
        .build_index(&reverse_posts)?
        .to_sharded_bundle(config())?;

    assert_eq!(forward, reverse);
    Ok(())
}

#[test]
fn direct_exact_backend_preserves_caller_order_for_equal_score_ties() -> TestResult {
    let documents = vec![
        IndexedDocument::new(
            PostId {
                title: "Second".to_string(),
                url: "/second".to_string(),
                meta: String::new(),
            },
            ["shared".to_string()],
        ),
        IndexedDocument::new(
            PostId {
                title: "First".to_string(),
                url: "/first".to_string(),
                meta: String::new(),
            },
            ["shared".to_string()],
        ),
    ];
    let index = ExactIndexBackend.build(documents);
    assert_eq!(urls(&search(&index, "shared", 10)), ["/second", "/first"]);

    let sharded = fully_loaded(&index);
    assert_eq!(urls(&sharded.search("shared", 10)?), ["/second", "/first"]);
    Ok(())
}

#[test]
fn bundles_are_deterministic_content_addressed_and_targeted() -> TestResult {
    let (monolithic, _terms) = skewed_corpus();
    let first = monolithic.to_sharded_bundle(config())?;
    let second = monolithic.to_sharded_bundle(config())?;
    assert_eq!(first, second);

    let root_size = first.root_bytes().len();
    let total_size: usize = first.shards().iter().map(|shard| shard.bytes().len()).sum();
    let max_size = first
        .shards()
        .iter()
        .map(|shard| shard.bytes().len())
        .max()
        .unwrap_or(0);
    let context = format!(
        "root={root_size} total_shards={total_size} max_shard={max_size} target={TARGET_BYTES}"
    );
    assert!(!first.shards().is_empty(), "{context}");
    assert!(
        max_size > TARGET_BYTES,
        "oversized fixture missing; {context}"
    );

    for artifact in first.shards() {
        let descriptor = artifact.descriptor();
        let digest: [u8; 32] = Sha256::digest(artifact.bytes()).into();
        assert_eq!(descriptor.digest, digest, "{context}");
        assert_eq!(descriptor.encoded_len, artifact.bytes().len(), "{context}");
        assert!(descriptor.filename.ends_with(SHARD_SUFFIX), "{context}");
        assert!(
            descriptor.filename.starts_with(&descriptor.digest_hex()),
            "{context}"
        );
        assert!(
            artifact.bytes().len() <= TARGET_BYTES || descriptor.first_term == descriptor.last_term,
            "non-singleton shard exceeded target; {context}; descriptor={descriptor:?}"
        );
    }
    Ok(())
}

#[test]
fn generated_root_and_shards_reject_corruption_and_duplicates() -> TestResult {
    let (monolithic, _terms) = skewed_corpus();
    let bundle = monolithic.to_sharded_bundle(config())?;

    let mut bad_root = bundle.root_bytes().to_vec();
    bad_root.push(0);
    assert!(matches!(
        ShardedIndex::from_root_bytes(&bad_root),
        Err(ShardError::MalformedRoot(_))
    ));
    let mut bad_magic = bundle.root_bytes().to_vec();
    bad_magic[0] ^= 1;
    assert!(matches!(
        ShardedIndex::from_root_bytes(&bad_magic),
        Err(ShardError::MalformedRoot(_))
    ));
    assert!(matches!(
        ShardedIndex::from_root_bytes(&bundle.root_bytes()[..ROOT_MAGIC.len()]),
        Err(ShardError::MalformedRoot(_))
    ));

    let artifact = &bundle.shards()[0];
    let mut corrupt = artifact.bytes().to_vec();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    let mut sharded = ShardedIndex::from_root_bytes(bundle.root_bytes())?;
    assert!(matches!(
        sharded.load_shard(&corrupt),
        Err(ShardError::DigestMismatch { id }) if id == artifact.descriptor().id
    ));

    assert_eq!(
        sharded.load_shard(artifact.bytes())?,
        artifact.descriptor().id
    );
    let count = sharded.loaded_shard_count();
    let bytes = sharded.loaded_shard_bytes();
    assert_eq!(
        sharded.load_shard(artifact.bytes())?,
        artifact.descriptor().id
    );
    assert_eq!(sharded.loaded_shard_count(), count);
    assert_eq!(sharded.loaded_shard_bytes(), bytes);
    assert!(matches!(
        sharded.load_shard(&corrupt),
        Err(ShardError::ConflictingShard(id)) if id == artifact.descriptor().id
    ));

    let unknown = encode_test_shard(ShardId::new(999), "unknown", &[0], false);
    assert!(matches!(
        sharded.load_shard(&unknown),
        Err(ShardError::UnknownShard(id)) if id == ShardId::new(999)
    ));
    Ok(())
}

#[test]
fn handcrafted_envelopes_validate_length_digest_id_counts_and_trailing_bytes() -> TestResult {
    let posts = [PostId {
        title: "Root Post".to_string(),
        url: "/root".to_string(),
        meta: String::new(),
    }];

    let wrong_id_shard = encode_test_shard(ShardId::new(1), "alpha", &[0], false);
    let wrong_id_root = encode_test_root(&posts, ShardId::new(0), "alpha", &wrong_id_shard, None);
    let mut wrong_id_index = ShardedIndex::from_root_bytes(&wrong_id_root)?;
    assert!(matches!(
        wrong_id_index.load_shard(&wrong_id_shard),
        Err(ShardError::WrongShardId { expected, actual })
            if expected == ShardId::new(0) && actual == ShardId::new(1)
    ));

    let trailing_shard = encode_test_shard(ShardId::new(0), "alpha", &[0], true);
    let trailing_root = encode_test_root(&posts, ShardId::new(0), "alpha", &trailing_shard, None);
    let mut trailing_index = ShardedIndex::from_root_bytes(&trailing_root)?;
    assert!(matches!(
        trailing_index.load_shard(&trailing_shard),
        Err(ShardError::MalformedShard(_))
    ));

    let empty_postings = encode_test_shard(ShardId::new(0), "alpha", &[], false);
    let empty_root = encode_test_root(&posts, ShardId::new(0), "alpha", &empty_postings, None);
    let mut empty_index = ShardedIndex::from_root_bytes(&empty_root)?;
    assert!(matches!(
        empty_index.load_shard(&empty_postings),
        Err(ShardError::MalformedShard(_))
    ));

    let valid_shard = encode_test_shard(ShardId::new(0), "alpha", &[0], false);
    let wrong_length_root = encode_test_root(
        &posts,
        ShardId::new(0),
        "alpha",
        &valid_shard,
        Some(valid_shard.len() + 1),
    );
    let mut wrong_length_index = ShardedIndex::from_root_bytes(&wrong_length_root)?;
    assert!(matches!(
        wrong_length_index.load_shard(&valid_shard),
        Err(ShardError::LengthMismatch { expected, actual, .. })
            if expected == valid_shard.len() + 1 && actual == valid_shard.len()
    ));

    let mut malformed_count_root = ROOT_MAGIC.to_vec();
    malformed_count_root.push(ROOT_VERSION);
    let encoded_posts = encode_test_posts(&posts);
    write_varint(&mut malformed_count_root, encoded_posts.len());
    malformed_count_root.extend_from_slice(&encoded_posts);
    write_varint(&mut malformed_count_root, usize::MAX);
    let malformed_count_result = ShardedIndex::from_root_bytes(&malformed_count_root);
    assert!(
        matches!(malformed_count_result, Err(ShardError::MalformedRoot(_))),
        "unexpected malformed-count result: {malformed_count_result:?}"
    );
    Ok(())
}

#[test]
fn root_posts_reject_impossible_counts_and_nested_string_lengths() {
    let mut impossible_count = Vec::new();
    write_varint(&mut impossible_count, usize::MAX);
    assert!(matches!(
        ShardedIndex::from_root_bytes(&encode_root_with_post_payload(&impossible_count)),
        Err(ShardError::MalformedRoot(_))
    ));

    for string_position in 0..3 {
        let mut impossible_string = Vec::new();
        write_varint(&mut impossible_string, 1);
        for position in 0..=string_position {
            if position == string_position {
                write_varint(&mut impossible_string, usize::MAX);
            } else {
                write_string(&mut impossible_string, "");
            }
        }
        assert!(matches!(
            ShardedIndex::from_root_bytes(&encode_root_with_post_payload(&impossible_string)),
            Err(ShardError::MalformedRoot(_))
        ));
    }

    let mut trailing_post_bytes = Vec::new();
    write_varint(&mut trailing_post_bytes, 0);
    trailing_post_bytes.push(0);
    assert!(matches!(
        ShardedIndex::from_root_bytes(&encode_root_with_post_payload(&trailing_post_bytes)),
        Err(ShardError::MalformedRoot(_))
    ));
}

fn encode_test_shard(id: ShardId, term: &str, postings: &[usize], trailing: bool) -> Vec<u8> {
    let mut bytes = SHARD_MAGIC.to_vec();
    bytes.push(SHARD_VERSION);
    write_varint(
        &mut bytes,
        usize::try_from(id.get()).expect("u32 fits usize"),
    );
    write_varint(&mut bytes, 1);
    write_string(&mut bytes, term);
    write_varint(&mut bytes, postings.len());
    let mut previous = 0;
    for (position, document) in postings.iter().copied().enumerate() {
        let delta = if position == 0 {
            document
        } else {
            document - previous
        };
        write_varint(&mut bytes, delta);
        previous = document;
    }
    if trailing {
        bytes.push(0);
    }
    bytes
}

fn encode_test_posts(posts: &[PostId]) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_varint(&mut bytes, posts.len());
    for post in posts {
        write_string(&mut bytes, &post.title);
        write_string(&mut bytes, &post.url);
        write_string(&mut bytes, &post.meta);
    }
    bytes
}

fn encode_root_with_post_payload(post_payload: &[u8]) -> Vec<u8> {
    let mut bytes = ROOT_MAGIC.to_vec();
    bytes.push(ROOT_VERSION);
    write_varint(&mut bytes, post_payload.len());
    bytes.extend_from_slice(post_payload);
    write_varint(&mut bytes, 0);
    bytes
}

fn encode_test_root(
    posts: &[PostId],
    descriptor_id: ShardId,
    term: &str,
    shard: &[u8],
    encoded_len_override: Option<usize>,
) -> Vec<u8> {
    let encoded_posts = encode_test_posts(posts);
    let digest: [u8; 32] = Sha256::digest(shard).into();
    let filename = format!("{}{SHARD_SUFFIX}", digest_hex(&digest));
    let mut bytes = ROOT_MAGIC.to_vec();
    bytes.push(ROOT_VERSION);
    write_varint(&mut bytes, encoded_posts.len());
    bytes.extend_from_slice(&encoded_posts);
    write_varint(&mut bytes, 1);
    write_varint(
        &mut bytes,
        usize::try_from(descriptor_id.get()).expect("u32 fits usize"),
    );
    write_string(&mut bytes, term);
    write_string(&mut bytes, term);
    write_varint(&mut bytes, encoded_len_override.unwrap_or(shard.len()));
    bytes.extend_from_slice(&digest);
    write_string(&mut bytes, &filename);
    bytes
}

fn digest_hex(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for &byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn write_string(bytes: &mut Vec<u8>, value: &str) {
    write_varint(bytes, value.len());
    bytes.extend_from_slice(value.as_bytes());
}

fn write_varint(bytes: &mut Vec<u8>, mut value: usize) {
    while value >= 0x80 {
        bytes.push((value.to_le_bytes()[0] & 0x7f) | 0x80);
        value >>= 7;
    }
    bytes.push(value.to_le_bytes()[0]);
}
