use std::collections::{BTreeSet, HashSet};

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail, ensure};
use fst::automaton::Str;
use fst::{Automaton, IntoStreamer, Streamer};
use serde::Deserialize;
use xorf::{HashProxy, Xor8};

mod inverted;

const EXPECTED_POSTS: usize = 73;
const PREFIX_MIN: usize = 3;
const PREFIX_MAX: usize = 8;
const RESULT_LIMIT: usize = 32;
const TIMING_SAMPLES_PER_LENGTH: usize = 256;
const TIMING_REPEATS: usize = 16;
const EXACT_TIMING_SAMPLES: usize = 1_536;
const FRONT_CODING_MAGIC: &[u8; 4] = b"FCB1";
const PRODUCTION_STORAGE_MAGIC: &[u8] = b"tinysearch\x01";
const FRONT_CODING_HEADER_LEN: usize = 4 + 2 + 4 + 4;
const FRONT_CODING_BLOCK_SIZES: [usize; 6] = [4, 8, 16, 32, 64, 128];

#[derive(Debug, Deserialize)]
struct Post {
    title: String,
    body: Option<String>,
}

#[derive(Debug)]
struct Corpus {
    posts: Vec<BTreeSet<String>>,
    vocabulary: Vec<String>,
    production_prefix_vocabulary: Vec<String>,
}

#[derive(Debug)]
struct FrontCodedSet {
    serialized: Vec<u8>,
    block_size: usize,
    term_count: usize,
    block_count: usize,
    data_start: usize,
}

enum Index {
    Raw {
        terms: Vec<String>,
        serialized: Vec<u8>,
    },
    FrontCoded {
        name: String,
        set: FrontCodedSet,
    },
    Fst {
        set: fst::Set<Vec<u8>>,
        serialized: Vec<u8>,
    },
    Fcsd {
        set: fcsd::Set,
        serialized: Vec<u8>,
    },
}

#[derive(Debug)]
struct SizeRow {
    name: String,
    bytes: usize,
    gzip_bytes: usize,
    brotli_bytes: usize,
    p50_us: f64,
    p95_us: f64,
}

#[derive(Debug)]
struct XorRow {
    label: &'static str,
    entries: usize,
    bytes: usize,
    gzip_bytes: usize,
    brotli_bytes: usize,
    growth: isize,
    growth_pct: f64,
}

struct XorResults {
    rows: Vec<XorRow>,
    exact_filters: Vec<DocumentFilter>,
}

struct InvertedRow {
    name: &'static str,
    bytes: usize,
    gzip_bytes: usize,
    brotli_bytes: usize,
    exact_p50_us: f64,
    exact_p95_us: f64,
    prefix_all_p50_us: f64,
    prefix_all_p95_us: f64,
    prefix_cap32_p50_us: f64,
    prefix_cap32_p95_us: f64,
}

struct CompletionRow {
    label: String,
    prefixes: usize,
    p50: usize,
    p95: usize,
    max: usize,
    over_32: usize,
}

struct StorageRow {
    name: String,
    bytes: usize,
    gzip_bytes: usize,
    brotli_bytes: usize,
}

struct Report<'a> {
    corpus_path: &'a Path,
    corpus: &'a Corpus,
    prefixes_by_length: &'a [Vec<String>],
    timing_prefix_count: usize,
    timing_term_count: usize,
    postings_entries: &'a inverted::Entries,
    vocabulary_rows: &'a [SizeRow],
    inverted_rows: &'a [InvertedRow],
    completion_rows: &'a [CompletionRow],
    storage_rows: &'a [StorageRow],
    xor_rows: &'a [XorRow],
}

impl FrontCodedSet {
    fn build(terms: &[String], block_size: usize) -> Result<Self> {
        ensure!(
            block_size.is_power_of_two(),
            "block size must be a power of two"
        );
        ensure!(block_size <= u16::MAX as usize, "block size is too large");
        ensure!(
            terms.len() <= u32::MAX as usize,
            "too many vocabulary terms"
        );

        let block_count = terms.len().div_ceil(block_size);
        ensure!(
            block_count <= u32::MAX as usize,
            "too many front-coding blocks"
        );

        let mut block_offsets = Vec::with_capacity(block_count);
        let mut data = Vec::new();

        for block in terms.chunks(block_size) {
            block_offsets
                .push(u32::try_from(data.len()).context("front-coded data exceeds 4 GiB")?);
            write_short_bytes(&mut data, block[0].as_bytes())?;

            let mut previous = block[0].as_bytes();
            for term in &block[1..] {
                let bytes = term.as_bytes();
                let common = common_prefix_len(previous, bytes);
                let suffix = &bytes[common..];
                data.extend_from_slice(&u16::try_from(common)?.to_le_bytes());
                write_short_bytes(&mut data, suffix)?;
                previous = bytes;
            }
        }

        let data_start = FRONT_CODING_HEADER_LEN + block_offsets.len() * size_of::<u32>();
        let mut serialized = Vec::with_capacity(data_start + data.len());
        serialized.extend_from_slice(FRONT_CODING_MAGIC);
        serialized.extend_from_slice(&u16::try_from(block_size)?.to_le_bytes());
        serialized.extend_from_slice(&u32::try_from(terms.len())?.to_le_bytes());
        serialized.extend_from_slice(&u32::try_from(block_count)?.to_le_bytes());
        for offset in block_offsets {
            serialized.extend_from_slice(&offset.to_le_bytes());
        }
        serialized.extend_from_slice(&data);

        Ok(Self {
            serialized,
            block_size,
            term_count: terms.len(),
            block_count,
            data_start,
        })
    }

    fn query(&self, prefix: &str) -> Vec<String> {
        if self.block_count == 0 {
            return Vec::new();
        }

        let prefix = prefix.as_bytes();
        let mut lo = 0;
        let mut hi = self.block_count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.first_term(mid) <= prefix {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        let first_block = lo.saturating_sub(1);
        let mut results = Vec::with_capacity(RESULT_LIMIT);
        for block_index in first_block..self.block_count {
            let terms_in_block = self
                .block_size
                .min(self.term_count - block_index * self.block_size);
            let mut cursor = self.block_absolute_offset(block_index);
            let mut term = read_short_bytes(&self.serialized, &mut cursor).to_vec();

            if collect_match(&term, prefix, &mut results) {
                return results;
            }

            for _ in 1..terms_in_block {
                let common = read_u16(&self.serialized, &mut cursor) as usize;
                let suffix = read_short_bytes(&self.serialized, &mut cursor);
                term.truncate(common);
                term.extend_from_slice(suffix);
                if collect_match(&term, prefix, &mut results) {
                    return results;
                }
            }
        }
        results
    }

    fn first_term(&self, block_index: usize) -> &[u8] {
        let mut cursor = self.block_absolute_offset(block_index);
        read_short_bytes(&self.serialized, &mut cursor)
    }

    fn block_absolute_offset(&self, block_index: usize) -> usize {
        let offset_pos = FRONT_CODING_HEADER_LEN + block_index * size_of::<u32>();
        let relative = u32::from_le_bytes(
            self.serialized[offset_pos..offset_pos + size_of::<u32>()]
                .try_into()
                .expect("front-coding offset has fixed width"),
        ) as usize;
        self.data_start + relative
    }
}

impl Index {
    fn name(&self) -> &str {
        match self {
            Self::Raw { .. } => "raw newline",
            Self::FrontCoded { name, .. } => name,
            Self::Fst { .. } => "fst::Set",
            Self::Fcsd { .. } => "fcsd::Set (bucket 8)",
        }
    }

    fn serialized(&self) -> &[u8] {
        match self {
            Self::Raw { serialized, .. }
            | Self::Fst { serialized, .. }
            | Self::Fcsd { serialized, .. } => serialized,
            Self::FrontCoded { set, .. } => &set.serialized,
        }
    }

    fn query(&self, prefix: &str) -> Vec<String> {
        match self {
            Self::Raw { terms, .. } => plain_prefix_query(terms, prefix),
            Self::FrontCoded { set, .. } => set.query(prefix),
            Self::Fst { set, .. } => {
                let automaton = Str::new(prefix).starts_with();
                let mut stream = set.search(automaton).into_stream();
                let mut results = Vec::with_capacity(RESULT_LIMIT);
                while let Some(key) = stream.next() {
                    results.push(
                        String::from_utf8(key.to_vec()).expect("vocabulary terms are valid UTF-8"),
                    );
                    if results.len() == RESULT_LIMIT {
                        break;
                    }
                }
                results
            }
            Self::Fcsd { set, .. } => fcsd_prefix_query(set, prefix),
        }
    }
}

fn main() -> Result<()> {
    let corpus_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../target/bench/endler-index.json"));

    let corpus = load_corpus(&corpus_path)?;
    let prefixes_by_length = real_prefixes(&corpus.vocabulary);
    let all_prefixes: Vec<&str> = prefixes_by_length
        .iter()
        .flat_map(|prefixes| prefixes.iter().map(String::as_str))
        .collect();
    let timing_prefixes = timing_sample(&prefixes_by_length);
    let timing_terms = even_sample(&corpus.vocabulary, EXACT_TIMING_SAMPLES);
    let indexes = build_indexes(&corpus.vocabulary)?;
    let postings_entries = inverted::build_entries(&corpus.posts)?;
    let inverted_indexes = inverted::ExactInvertedIndex::build_all(&postings_entries)?;

    validate_indexes(&indexes, &corpus.vocabulary, &all_prefixes)?;
    validate_inverted_indexes(
        &inverted_indexes,
        &postings_entries,
        &all_prefixes,
        corpus.posts.len(),
    )?;

    let mut rows = Vec::with_capacity(indexes.len());
    for index in &indexes {
        let (p50_us, p95_us) = time_queries(index, &timing_prefixes);
        rows.push(SizeRow {
            name: index.name().to_owned(),
            bytes: index.serialized().len(),
            gzip_bytes: compressed_size("gzip", &["-9", "-c"], index.serialized())?,
            brotli_bytes: compressed_size("brotli", &["-q", "11", "-c"], index.serialized())?,
            p50_us,
            p95_us,
        });
    }

    let mut inverted_rows = Vec::with_capacity(inverted_indexes.len());
    for index in &inverted_indexes {
        let (exact_p50_us, exact_p95_us) = time_exact_queries(index, &timing_terms);
        let (prefix_all_p50_us, prefix_all_p95_us) =
            time_inverted_prefix_queries(index, &timing_prefixes, None, corpus.posts.len());
        let (prefix_cap32_p50_us, prefix_cap32_p95_us) = time_inverted_prefix_queries(
            index,
            &timing_prefixes,
            Some(RESULT_LIMIT),
            corpus.posts.len(),
        );
        inverted_rows.push(InvertedRow {
            name: index.name(),
            bytes: index.serialized().len(),
            gzip_bytes: compressed_size("gzip", &["-9", "-c"], index.serialized())?,
            brotli_bytes: compressed_size("brotli", &["-q", "11", "-c"], index.serialized())?,
            exact_p50_us,
            exact_p95_us,
            prefix_all_p50_us,
            prefix_all_p95_us,
            prefix_cap32_p50_us,
            prefix_cap32_p95_us,
        });
    }

    let completion_rows = completion_rows(&corpus.vocabulary, &prefixes_by_length);
    let xor_results = xor_growth(&corpus.posts)?;
    let storage_rows =
        storage_comparison_rows(&corpus, &xor_results.exact_filters, &inverted_indexes)?;
    print_report(Report {
        corpus_path: &corpus_path,
        corpus: &corpus,
        prefixes_by_length: &prefixes_by_length,
        timing_prefix_count: timing_prefixes.len(),
        timing_term_count: timing_terms.len(),
        postings_entries: &postings_entries,
        vocabulary_rows: &rows,
        inverted_rows: &inverted_rows,
        completion_rows: &completion_rows,
        storage_rows: &storage_rows,
        xor_rows: &xor_results.rows,
    });
    Ok(())
}

fn load_corpus(path: &Path) -> Result<Corpus> {
    let raw = std::fs::read(path)
        .with_context(|| format!("failed to read corpus at {}", path.display()))?;
    let posts: Vec<Post> = serde_json::from_slice(&raw).context("failed to parse corpus JSON")?;
    ensure!(
        posts.len() == EXPECTED_POSTS,
        "expected {EXPECTED_POSTS} posts, found {}",
        posts.len()
    );

    let stopwords: HashSet<&str> = include_str!("../stopwords.txt")
        .split_whitespace()
        .collect();
    let mut global = BTreeSet::new();
    let mut production_prefix_vocabulary = BTreeSet::new();
    let mut per_post = Vec::with_capacity(posts.len());

    for post in posts {
        let title = tokenize(&post.title, &stopwords);
        let body = post
            .body
            .as_deref()
            .map(|body| tokenize(body, &stopwords))
            .unwrap_or_default();
        production_prefix_vocabulary.extend(
            body.difference(&title)
                .filter(|term| term.chars().count() > PREFIX_MIN)
                .cloned(),
        );

        let mut terms = title;
        terms.extend(body);
        global.extend(terms.iter().cloned());
        per_post.push(terms);
    }

    Ok(Corpus {
        posts: per_post,
        vocabulary: global.into_iter().collect(),
        production_prefix_vocabulary: production_prefix_vocabulary.into_iter().collect(),
    })
}

fn tokenize(text: &str, stopwords: &HashSet<&str>) -> BTreeSet<String> {
    strip_markdown::strip_markdown(text)
        .replace(
            |character: char| !(character.is_alphabetic() || character == '\''),
            " ",
        )
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|term| !stopwords.contains(term.as_str()))
        .collect()
}

fn raw_newline_vocabulary(terms: &[String]) -> Vec<u8> {
    let mut raw = terms.join("\n").into_bytes();
    raw.push(b'\n');
    raw
}

fn build_indexes(terms: &[String]) -> Result<Vec<Index>> {
    let raw = raw_newline_vocabulary(terms);

    let mut indexes = vec![Index::Raw {
        terms: terms.to_vec(),
        serialized: raw,
    }];

    for block_size in FRONT_CODING_BLOCK_SIZES {
        indexes.push(Index::FrontCoded {
            name: format!("front-coded/{block_size}"),
            set: FrontCodedSet::build(terms, block_size)?,
        });
    }

    let fst_set = fst::Set::from_iter(terms).context("failed to build fst::Set")?;
    let fst_serialized = fst_set.as_fst().as_bytes().to_vec();
    indexes.push(Index::Fst {
        set: fst_set,
        serialized: fst_serialized,
    });

    let fcsd_set = fcsd::Set::new(terms).context("failed to build fcsd::Set")?;
    let mut fcsd_serialized = Vec::with_capacity(fcsd_set.size_in_bytes());
    fcsd_set
        .serialize_into(&mut fcsd_serialized)
        .context("failed to serialize fcsd::Set")?;
    indexes.push(Index::Fcsd {
        set: fcsd_set,
        serialized: fcsd_serialized,
    });

    Ok(indexes)
}

fn real_prefixes(terms: &[String]) -> Vec<Vec<String>> {
    (PREFIX_MIN..=PREFIX_MAX)
        .map(|length| {
            terms
                .iter()
                .filter_map(|term| prefix_at_character_length(term, length))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        })
        .collect()
}

fn timing_sample(prefixes_by_length: &[Vec<String>]) -> Vec<&str> {
    prefixes_by_length
        .iter()
        .flat_map(|prefixes| even_sample(prefixes, TIMING_SAMPLES_PER_LENGTH))
        .collect()
}

fn even_sample(values: &[String], maximum: usize) -> Vec<&str> {
    let count = values.len().min(maximum);
    (0..count)
        .map(|index| values[index * values.len() / count].as_str())
        .collect()
}

fn validate_indexes(indexes: &[Index], terms: &[String], prefixes: &[&str]) -> Result<()> {
    for prefix in prefixes {
        let expected = plain_prefix_query(terms, prefix);
        for index in indexes {
            let actual = index.query(prefix);
            if actual != expected {
                bail!(
                    "{} returned wrong results for prefix {prefix:?}: expected {expected:?}, got {actual:?}",
                    index.name()
                );
            }
        }
    }
    Ok(())
}

fn validate_inverted_indexes(
    indexes: &[inverted::ExactInvertedIndex],
    entries: &inverted::Entries,
    prefixes: &[&str],
    document_count: usize,
) -> Result<()> {
    for (term, expected) in entries {
        for index in indexes {
            let actual = index.exact_query(term);
            if actual != *expected {
                bail!(
                    "{} returned wrong exact postings for {term:?}: expected {expected:?}, got {actual:?}",
                    index.name()
                );
            }
        }
    }

    for prefix in prefixes {
        for completion_limit in [None, Some(RESULT_LIMIT)] {
            let expected =
                inverted::baseline_prefix_query(entries, prefix, completion_limit, document_count);
            for index in indexes {
                let actual = index.prefix_query(prefix, completion_limit, document_count);
                if actual.documents != expected.documents
                    || actual.completions != expected.completions
                {
                    bail!(
                        "{} returned wrong prefix postings for {prefix:?} with limit {completion_limit:?}: expected {:?}/{} completions, got {:?}/{} completions",
                        index.name(),
                        expected.documents,
                        expected.completions,
                        actual.documents,
                        actual.completions
                    );
                }
            }
        }
    }
    Ok(())
}

fn plain_prefix_query(terms: &[String], prefix: &str) -> Vec<String> {
    let first = terms.partition_point(|term| term.as_str() < prefix);
    terms[first..]
        .iter()
        .take_while(|term| term.starts_with(prefix))
        .take(RESULT_LIMIT)
        .cloned()
        .collect()
}

fn fcsd_prefix_query(set: &fcsd::Set, prefix: &str) -> Vec<String> {
    // fcsd 0.2.0's native predictive iterator misses some prefixes that fall
    // between bucket headers ("aby" -> "abysmal" in this corpus). Decode-based
    // lower_bound preserves a useful comparison without accepting wrong results.
    let prefix = prefix.as_bytes();
    let mut decoder = set.decoder();
    let mut lo = 0;
    let mut hi = set.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if decoder.run(mid).as_slice() < prefix {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }

    let mut results = Vec::with_capacity(RESULT_LIMIT);
    for index in lo..set.len() {
        let term = decoder.run(index);
        if !term.starts_with(prefix) {
            break;
        }
        results.push(String::from_utf8(term).expect("vocabulary terms are valid UTF-8"));
        if results.len() == RESULT_LIMIT {
            break;
        }
    }
    results
}

fn time_queries(index: &Index, prefixes: &[&str]) -> (f64, f64) {
    for prefix in prefixes {
        std::hint::black_box(index.query(std::hint::black_box(prefix)));
    }

    let mut samples = Vec::with_capacity(prefixes.len());
    let mut checksum = 0_usize;
    for prefix in prefixes {
        let start = Instant::now();
        for _ in 0..TIMING_REPEATS {
            let matches = index.query(std::hint::black_box(prefix));
            checksum = checksum.wrapping_add(std::hint::black_box(matches.len()));
        }
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.0 / TIMING_REPEATS as f64);
    }
    std::hint::black_box(checksum);

    samples.sort_by(f64::total_cmp);
    (percentile(&samples, 50), percentile(&samples, 95))
}

fn time_exact_queries(index: &inverted::ExactInvertedIndex, terms: &[&str]) -> (f64, f64) {
    for term in terms {
        std::hint::black_box(index.exact_query(std::hint::black_box(term)));
    }

    let mut samples = Vec::with_capacity(terms.len());
    let mut checksum = 0_usize;
    for term in terms {
        let start = Instant::now();
        for _ in 0..TIMING_REPEATS {
            let documents = index.exact_query(std::hint::black_box(term));
            checksum = checksum.wrapping_add(std::hint::black_box(documents.len()));
        }
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.0 / TIMING_REPEATS as f64);
    }
    std::hint::black_box(checksum);
    samples.sort_by(f64::total_cmp);
    (percentile(&samples, 50), percentile(&samples, 95))
}

fn time_inverted_prefix_queries(
    index: &inverted::ExactInvertedIndex,
    prefixes: &[&str],
    completion_limit: Option<usize>,
    document_count: usize,
) -> (f64, f64) {
    for prefix in prefixes {
        std::hint::black_box(index.prefix_query(
            std::hint::black_box(prefix),
            completion_limit,
            document_count,
        ));
    }

    let mut samples = Vec::with_capacity(prefixes.len());
    let mut checksum = 0_usize;
    for prefix in prefixes {
        let start = Instant::now();
        for _ in 0..TIMING_REPEATS {
            let result = index.prefix_query(
                std::hint::black_box(prefix),
                completion_limit,
                document_count,
            );
            checksum = checksum
                .wrapping_add(std::hint::black_box(result.documents.len()))
                .wrapping_add(std::hint::black_box(result.completions));
        }
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.0 / TIMING_REPEATS as f64);
    }
    std::hint::black_box(checksum);
    samples.sort_by(f64::total_cmp);
    (percentile(&samples, 50), percentile(&samples, 95))
}

fn completion_rows(terms: &[String], prefixes_by_length: &[Vec<String>]) -> Vec<CompletionRow> {
    let mut rows: Vec<CompletionRow> = (PREFIX_MIN..=PREFIX_MAX)
        .zip(prefixes_by_length)
        .map(|(length, prefixes)| {
            summarize_completions(
                length.to_string(),
                prefixes
                    .iter()
                    .map(|prefix| completion_count(terms, prefix))
                    .collect(),
            )
        })
        .collect();
    rows.push(summarize_completions(
        "all".to_owned(),
        prefixes_by_length
            .iter()
            .flatten()
            .map(|prefix| completion_count(terms, prefix))
            .collect(),
    ));
    rows
}

fn completion_count(terms: &[String], prefix: &str) -> usize {
    let start = terms.partition_point(|term| term.as_str() < prefix);
    terms[start..]
        .iter()
        .take_while(|term| term.starts_with(prefix))
        .count()
}

fn summarize_completions(label: String, mut counts: Vec<usize>) -> CompletionRow {
    counts.sort_unstable();
    CompletionRow {
        label,
        prefixes: counts.len(),
        p50: usize_percentile(&counts, 50),
        p95: usize_percentile(&counts, 95),
        max: *counts.last().expect("real prefix groups are non-empty"),
        over_32: counts.iter().filter(|&&count| count > RESULT_LIMIT).count(),
    }
}

fn storage_comparison_rows(
    corpus: &Corpus,
    exact_filters: &[DocumentFilter],
    indexes: &[inverted::ExactInvertedIndex],
) -> Result<Vec<StorageRow>> {
    let exact_xor = bincode::serde::encode_to_vec(exact_filters, bincode::config::legacy())
        .context("failed to serialize exact Xor8 filters with production legacy config")?;
    let xor_production_vocabulary =
        production_storage_payload(exact_filters, &corpus.production_prefix_vocabulary)?;
    let xor_full_vocabulary = production_storage_payload(exact_filters, &corpus.vocabulary)?;

    let mut payloads = vec![
        ("Xor8 exact filters only (legacy)".to_owned(), exact_xor),
        (
            "Xor8 + production-shaped vocabulary/envelope".to_owned(),
            xor_production_vocabulary,
        ),
        (
            "Xor8 + full vocabulary/envelope".to_owned(),
            xor_full_vocabulary,
        ),
    ];
    payloads.extend(
        indexes
            .iter()
            .map(|index| (index.name().to_owned(), index.serialized().to_vec())),
    );

    payloads
        .into_iter()
        .map(|(name, payload)| {
            Ok(StorageRow {
                name,
                bytes: payload.len(),
                gzip_bytes: compressed_size("gzip", &["-9", "-c"], &payload)?,
                brotli_bytes: compressed_size("brotli", &["-q", "11", "-c"], &payload)?,
            })
        })
        .collect()
}

fn production_storage_payload(
    filters: &[DocumentFilter],
    vocabulary: &[String],
) -> Result<Vec<u8>> {
    let vocabulary = vocabulary.join("\n");
    let payload = bincode::serde::encode_to_vec((filters, vocabulary), bincode::config::legacy())
        .context(
        "failed to serialize Xor8 filters and vocabulary with production legacy config",
    )?;
    let mut serialized = Vec::with_capacity(PRODUCTION_STORAGE_MAGIC.len() + payload.len());
    serialized.extend_from_slice(PRODUCTION_STORAGE_MAGIC);
    serialized.extend_from_slice(&payload);
    Ok(serialized)
}

fn percentile(sorted: &[f64], percentile: usize) -> f64 {
    let index = (sorted.len() - 1) * percentile / 100;
    sorted[index]
}

fn usize_percentile(sorted: &[usize], percentile: usize) -> usize {
    let index = (sorted.len() - 1) * percentile / 100;
    sorted[index]
}

fn compressed_size(program: &str, arguments: &[&str], input: &[u8]) -> Result<usize> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute {program}"))?;

    child
        .stdin
        .take()
        .context("compressor stdin was unavailable")?
        .write_all(input)
        .with_context(|| format!("failed to send input to {program}"))?;

    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to wait for {program}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout.len())
}

type DocumentFilter = HashProxy<String, std::collections::hash_map::DefaultHasher, Xor8>;

fn xor_growth(posts: &[BTreeSet<String>]) -> Result<XorResults> {
    let configurations: [(&str, &[usize]); 4] = [
        ("exact terms", &[]),
        ("[3]", &[3]),
        ("[3,4]", &[3, 4]),
        ("[3,4,6,8]", &[3, 4, 6, 8]),
    ];

    let mut measurements = Vec::with_capacity(configurations.len());
    let mut exact_filters = None;
    for (label, lengths) in configurations {
        let expanded: Vec<Vec<String>> = posts
            .iter()
            .map(|terms| add_prefix_checkpoints(terms, lengths).into_iter().collect())
            .collect();
        let entry_count = expanded.iter().map(Vec::len).sum();
        let filters: Vec<DocumentFilter> = expanded.iter().map(DocumentFilter::from).collect();
        let serialized = bincode::serde::encode_to_vec(&filters, bincode::config::legacy())
            .context("failed to serialize Xor8 filters with production legacy config")?;
        let gzip_bytes = compressed_size("gzip", &["-9", "-c"], &serialized)?;
        let brotli_bytes = compressed_size("brotli", &["-q", "11", "-c"], &serialized)?;
        measurements.push((label, entry_count, serialized, gzip_bytes, brotli_bytes));
        if lengths.is_empty() {
            exact_filters = Some(filters);
        }
    }

    let baseline = measurements[0].2.len();
    let rows = measurements
        .into_iter()
        .map(|(label, entries, serialized, gzip_bytes, brotli_bytes)| {
            let bytes = serialized.len();
            let growth = bytes as isize - baseline as isize;
            let growth_pct = growth as f64 * 100.0 / baseline as f64;
            XorRow {
                label,
                entries,
                bytes,
                gzip_bytes,
                brotli_bytes,
                growth,
                growth_pct,
            }
        })
        .collect();
    Ok(XorResults {
        rows,
        exact_filters: exact_filters.context("missing exact Xor8 baseline")?,
    })
}

fn add_prefix_checkpoints(terms: &BTreeSet<String>, lengths: &[usize]) -> BTreeSet<String> {
    let mut expanded = terms.clone();
    for term in terms {
        for &length in lengths {
            if let Some(prefix) = prefix_at_character_length(term, length) {
                expanded.insert(prefix);
            }
        }
    }
    expanded
}

fn prefix_at_character_length(term: &str, length: usize) -> Option<String> {
    let mut chars = term.chars();
    let prefix: String = chars.by_ref().take(length).collect();
    (prefix.chars().count() == length).then_some(prefix)
}

fn collect_match(term: &[u8], prefix: &[u8], results: &mut Vec<String>) -> bool {
    if term.starts_with(prefix) {
        results.push(String::from_utf8(term.to_vec()).expect("front-coded terms are valid UTF-8"));
        results.len() == RESULT_LIMIT
    } else {
        term > prefix
    }
}

fn common_prefix_len(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

fn write_short_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    output.extend_from_slice(&u16::try_from(bytes.len())?.to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn read_short_bytes<'a>(bytes: &'a [u8], cursor: &mut usize) -> &'a [u8] {
    let length = read_u16(bytes, cursor) as usize;
    let value = &bytes[*cursor..*cursor + length];
    *cursor += length;
    value
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> u16 {
    let value = u16::from_le_bytes(
        bytes[*cursor..*cursor + size_of::<u16>()]
            .try_into()
            .expect("front-coding integer has fixed width"),
    );
    *cursor += size_of::<u16>();
    value
}

fn print_report(report: Report<'_>) {
    let Report {
        corpus_path,
        corpus,
        prefixes_by_length,
        timing_prefix_count,
        timing_term_count,
        postings_entries,
        vocabulary_rows,
        inverted_rows,
        completion_rows,
        storage_rows,
        xor_rows,
    } = report;
    let doc_term_entries: usize = postings_entries
        .iter()
        .map(|(_, postings)| postings.len())
        .sum();

    println!("# Word index benchmark results\n");
    println!("- Corpus: `{}`", corpus_path.display());
    println!("- Posts: {}", corpus.posts.len());
    println!("- Unique exact terms: {}", corpus.vocabulary.len());
    println!("- Exact document-term entries: {doc_term_entries}");
    println!(
        "- Production-shaped prefix vocabulary terms: {}",
        corpus.production_prefix_vocabulary.len()
    );
    println!("- Vocabulary completion cap: {RESULT_LIMIT}");
    println!(
        "- Validated real prefixes: {} ({})",
        prefixes_by_length.iter().map(Vec::len).sum::<usize>(),
        (PREFIX_MIN..=PREFIX_MAX)
            .zip(prefixes_by_length)
            .map(|(length, prefixes)| format!("{length}: {}", prefixes.len()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "- Timing samples: {timing_term_count} exact terms and {timing_prefix_count} prefixes, {TIMING_REPEATS} repetitions each\n"
    );

    println!("## Global vocabulary representations\n");
    println!(
        "| representation | bytes | bytes/term | gzip -9 | Brotli q11 | completion p50 (µs) | completion p95 (µs) |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for row in vocabulary_rows {
        println!(
            "| {} | {} | {:.2} | {} | {} | {:.3} | {:.3} |",
            row.name,
            row.bytes,
            row.bytes as f64 / corpus.vocabulary.len() as f64,
            row.gzip_bytes,
            row.brotli_bytes,
            row.p50_us,
            row.p95_us
        );
    }

    println!("\n## Exact inverted indexes\n");
    println!(
        "| representation | bytes | gzip -9 | Brotli q11 | exact p50 (µs) | exact p95 (µs) | prefix all p50 (µs) | prefix all p95 (µs) | prefix cap32 p50 (µs) | prefix cap32 p95 (µs) |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for row in inverted_rows {
        println!(
            "| {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            row.name,
            row.bytes,
            row.gzip_bytes,
            row.brotli_bytes,
            row.exact_p50_us,
            row.exact_p95_us,
            row.prefix_all_p50_us,
            row.prefix_all_p95_us,
            row.prefix_cap32_p50_us,
            row.prefix_cap32_p95_us
        );
    }

    println!("\n## Real-prefix completion counts\n");
    println!(
        "| prefix chars | prefixes | completions p50 | completions p95 | max | prefixes over 32 |"
    );
    println!("|---|---:|---:|---:|---:|---:|");
    for row in completion_rows {
        println!(
            "| {} | {} | {} | {} | {} | {} |",
            row.label, row.prefixes, row.p50, row.p95, row.max, row.over_32
        );
    }

    println!("\n## PostId-independent storage comparison\n");
    println!(
        "| representation | bytes | gzip -9 | Brotli q11 | delta vs Xor + production-shaped vocabulary |"
    );
    println!("|---|---:|---:|---:|---:|");
    let production_baseline = storage_rows[1].bytes;
    for row in storage_rows {
        println!(
            "| {} | {} | {} | {} | {:+} |",
            row.name,
            row.bytes,
            row.gzip_bytes,
            row.brotli_bytes,
            row.bytes as isize - production_baseline as isize
        );
    }

    println!("\n## Per-document Xor8 prefix checkpoints (production legacy bincode)\n");
    println!(
        "| checkpoints | total filter entries | serialized bytes | gzip -9 | Brotli q11 | growth (bytes) | growth (%) |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for row in xor_rows {
        println!(
            "| {} | {} | {} | {} | {} | {:+} | {:+.1}% |",
            row.label,
            row.entries,
            row.bytes,
            row.gzip_bytes,
            row.brotli_bytes,
            row.growth,
            row.growth_pct
        );
    }

    println!("\n## Method notes\n");
    println!(
        "- Index tokenization mirrors `src/api.rs`: strip Markdown, retain Unicode alphabetic characters and apostrophes, lowercase, split whitespace, and remove the copied tinysearch stopword list."
    );
    println!(
        "- The inverted indexes map every exact title/body term to sorted document IDs. They do not model PostId storage, title weighting, multi-term scoring, or final result ranking."
    );
    println!(
        "- Uncapped inverted-prefix queries union postings from every matching vocabulary term and therefore have exact set semantics. Cap32 uses only the first 32 lexicographic completions and can omit documents reached only by later completions."
    );
    println!(
        "- Production handles title prefixes by scanning tokenized PostId titles directly. Its global vocabulary contains body/metadata terms not present in that document's title and only terms longer than three characters. Queries shorter than three characters get no prefix expansion, but exact Xor/title checks still run."
    );
    println!(
        "- The benchmark's full vocabulary deliberately includes title terms and all lengths because the inverted index also replaces exact Xor membership. The production-shaped baseline reproduces the discarded Xor-plus-vocabulary experiment's body-minus-title and >3-character rules; this corpus has no metadata."
    );
    println!(
        "- The simple inverted row uses `Vec<(String, Vec<u32>)>` with bincode standard encoding. The compact row stores newline-separated vocabulary followed by varint list lengths and delta-coded document IDs; query offsets are rebuilt while decoding and are not serialized."
    );
    println!(
        "- Xor8 rows serialize one `HashProxy<String, DefaultHasher, Xor8>` per document using the same bincode legacy configuration as production. PostIds are intentionally excluded from both Xor and inverted rows."
    );
    println!(
        "- Query timings include materializing result vectors. They are in-process wall-clock p50/p95 measurements, not Criterion confidence intervals."
    );
    println!(
        "- `fcsd` 0.2.0's native predictive iterator missed a validated corpus prefix (`aby` -> `abysmal`), so its row uses decoder-based binary search plus sequential decode for correctness."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> Vec<String> {
        ["alpha", "alphabet", "alpine", "beta", "better", "zeta"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn front_coding_matches_plain_prefix_queries() {
        let terms = terms();
        for block_size in [1, 2, 4, 8] {
            let set = FrontCodedSet::build(&terms, block_size).unwrap();
            for prefix in ["a", "alp", "alpha", "b", "bet", "z", "zz"] {
                assert_eq!(set.query(prefix), plain_prefix_query(&terms, prefix));
            }
        }
    }

    #[test]
    fn fcsd_decoder_query_matches_plain_prefix_queries() {
        let terms = terms();
        let set = fcsd::Set::new(&terms).unwrap();
        for prefix in ["a", "alp", "alpha", "b", "bet", "z", "zz"] {
            assert_eq!(
                fcsd_prefix_query(&set, prefix),
                plain_prefix_query(&terms, prefix)
            );
        }
    }

    #[test]
    fn real_prefixes_use_character_lengths() {
        let terms = vec!["überraschung".to_owned()];
        let prefixes = real_prefixes(&terms);
        assert_eq!(prefixes[0], ["übe"]);
        assert_eq!(prefixes[5], ["überrasc"]);
    }
}
