//! Sharded exact-index storage and incremental search.

use crate::{
    ExactIndex, MIN_PREFIX_LEN, PostId, SearchIndex, SearchIndexData, matching_term_range,
    ranked_posts, title_scores, tokenize,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const ROOT_MAGIC: &[u8] = b"tinysearch-sharded-root";
const SHARD_MAGIC: &[u8] = b"tinysearch-shard";
const ROOT_VERSION: u8 = 1;
const SHARD_VERSION: u8 = 1;
const DIGEST_LEN: usize = 32;
// Conservative lower bounds used before allocating from untrusted counts:
// ID + two one-byte strings + length + digest + filename length; and
// term length + one-byte term + posting count + first document ID.
const MINIMUM_DESCRIPTOR_BYTES: usize = 39;
const MINIMUM_TERM_BYTES: usize = 4;
const DEFAULT_TARGET_BYTES: usize = 64 * 1024;
const SHARD_FILE_SUFFIX: &str = ".tinysearch-shard";

/// Configuration for partitioning an exact index into lexical shards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardConfig {
    target_bytes: usize,
}

impl ShardConfig {
    /// Creates a configuration with the requested encoded raw-byte target.
    ///
    /// A single term and its posting list are kept together, so an oversized
    /// term may produce a shard larger than this target.
    ///
    /// # Errors
    ///
    /// Returns [`ShardError::InvalidTargetBytes`] when `target_bytes` is zero.
    pub const fn new(target_bytes: usize) -> Result<Self, ShardError> {
        if target_bytes == 0 {
            Err(ShardError::InvalidTargetBytes)
        } else {
            Ok(Self { target_bytes })
        }
    }

    /// Returns the encoded raw-byte target for each lexical shard.
    #[must_use]
    pub const fn target_bytes(self) -> usize {
        self.target_bytes
    }
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            target_bytes: DEFAULT_TARGET_BYTES,
        }
    }
}

impl TryFrom<usize> for ShardConfig {
    type Error = ShardError;

    fn try_from(target_bytes: usize) -> Result<Self, Self::Error> {
        Self::new(target_bytes)
    }
}

/// Stable numeric identifier for one lexical shard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShardId(u32);

impl ShardId {
    /// Creates a shard identifier from its numeric value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the numeric value of this identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for ShardId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Root metadata describing one immutable lexical shard artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardDescriptor {
    /// Contiguous numeric shard identifier.
    pub id: ShardId,
    /// First normalized term contained in the shard.
    pub first_term: String,
    /// Last normalized term contained in the shard.
    pub last_term: String,
    /// Content-addressed artifact filename.
    pub filename: String,
    /// Exact encoded artifact length in bytes.
    pub encoded_len: usize,
    /// SHA-256 digest of the encoded artifact.
    pub digest: [u8; DIGEST_LEN],
}

impl ShardDescriptor {
    /// Returns the lowercase hexadecimal SHA-256 digest.
    #[must_use]
    pub fn digest_hex(&self) -> String {
        digest_hex(&self.digest)
    }
}

/// Encoded bytes for one immutable lexical shard and their descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardArtifact {
    descriptor: ShardDescriptor,
    bytes: Vec<u8>,
}

impl ShardArtifact {
    /// Returns the root descriptor for this artifact.
    #[must_use]
    pub const fn descriptor(&self) -> &ShardDescriptor {
        &self.descriptor
    }

    /// Returns the encoded shard bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Splits the artifact into its descriptor and encoded bytes.
    #[must_use]
    pub fn into_parts(self) -> (ShardDescriptor, Vec<u8>) {
        (self.descriptor, self.bytes)
    }
}

/// A root index and all content-addressed lexical shard artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardedIndexBundle {
    root_bytes: Vec<u8>,
    shards: Vec<ShardArtifact>,
}

impl ShardedIndexBundle {
    /// Returns the encoded root bytes.
    #[must_use]
    pub fn root_bytes(&self) -> &[u8] {
        &self.root_bytes
    }

    /// Returns the lexical shard artifacts in identifier order.
    #[must_use]
    pub fn shards(&self) -> &[ShardArtifact] {
        &self.shards
    }

    /// Splits this bundle into its root bytes and lexical artifacts.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, Vec<ShardArtifact>) {
        (self.root_bytes, self.shards)
    }
}

/// Error produced while building, decoding, loading, or searching a sharded index.
#[derive(Debug)]
#[non_exhaustive]
pub enum ShardError {
    /// A shard target of zero bytes is invalid.
    InvalidTargetBytes,
    /// Only the exact backend can be converted to lexical shards.
    UnsupportedBackend,

    /// The root envelope or its descriptors are malformed.
    MalformedRoot(&'static str),
    /// A shard envelope, vocabulary, or posting list is malformed.
    MalformedShard(&'static str),
    /// The artifact length differs from the root descriptor.
    LengthMismatch {
        /// Shard whose length did not match.
        id: ShardId,
        /// Length declared by the root.
        expected: usize,
        /// Length of the supplied artifact.
        actual: usize,
    },
    /// The artifact digest differs from the root descriptor.
    DigestMismatch {
        /// Shard whose digest did not match.
        id: ShardId,
    },
    /// The shard envelope contains an identifier different from its descriptor.
    WrongShardId {
        /// Identifier declared by the root descriptor.
        expected: ShardId,
        /// Identifier encoded in the shard envelope.
        actual: ShardId,
    },
    /// The supplied artifact does not belong to this root index.
    UnknownShard(ShardId),
    /// Different bytes were supplied for a shard that is already loaded.
    ConflictingShard(ShardId),
    /// A query cannot run until the listed lexical shards are loaded.
    NeedsShards(Vec<ShardId>),
}

impl std::fmt::Display for ShardError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTargetBytes => {
                formatter.write_str("shard target must be greater than zero")
            }
            Self::UnsupportedBackend => formatter.write_str("sharding requires the exact backend"),
            Self::MalformedRoot(reason) => write!(formatter, "malformed sharded root: {reason}"),
            Self::MalformedShard(reason) => write!(formatter, "malformed lexical shard: {reason}"),
            Self::LengthMismatch {
                id,
                expected,
                actual,
            } => write!(
                formatter,
                "shard {id} has length {actual}, expected {expected}"
            ),
            Self::DigestMismatch { id } => write!(formatter, "shard {id} digest does not match"),
            Self::WrongShardId { expected, actual } => write!(
                formatter,
                "shard envelope has id {actual}, expected {expected}"
            ),
            Self::UnknownShard(id) => write!(formatter, "shard {id} does not belong to this root"),
            Self::ConflictingShard(id) => {
                write!(
                    formatter,
                    "different bytes are already loaded for shard {id}"
                )
            }
            Self::NeedsShards(ids) => write!(formatter, "query needs unloaded shards {ids:?}"),
        }
    }
}

impl std::error::Error for ShardError {}

#[derive(Debug)]
struct LoadedShard {
    terms: Vec<String>,
    postings: Vec<Vec<usize>>,
    digest: [u8; DIGEST_LEN],
}

/// An exact index whose root is available immediately and whose lexical shards
/// can be loaded incrementally.
#[derive(Debug)]
pub struct ShardedIndex {
    posts: Vec<PostId>,
    descriptors: Vec<ShardDescriptor>,
    loaded: BTreeMap<ShardId, LoadedShard>,
    loaded_bytes: usize,
}

impl ShardedIndex {
    /// Decodes a sharded root envelope without loading lexical artifacts.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported versions, malformed post metadata,
    /// invalid descriptor order, non-canonical filenames, or trailing bytes.
    pub fn from_root_bytes(bytes: &[u8]) -> Result<Self, ShardError> {
        let (posts, descriptors) = decode_root(bytes)?;
        Ok(Self {
            posts,
            descriptors,
            loaded: BTreeMap::new(),
            loaded_bytes: 0,
        })
    }

    /// Returns the root descriptors in numeric and lexical order.
    #[must_use]
    pub fn descriptors(&self) -> &[ShardDescriptor] {
        &self.descriptors
    }

    /// Plans the deduplicated shards needed to evaluate `query` completely.
    #[must_use]
    pub fn required_shards(&self, query: &str) -> Vec<ShardId> {
        let terms = tokenize(query);
        self.required_shards_for_terms(&terms)
    }

    /// Validates and loads one content-addressed lexical shard.
    ///
    /// Loading the same bytes more than once is idempotent. Supplying different
    /// bytes for an already loaded identifier returns a conflict.
    ///
    /// # Errors
    ///
    /// Returns an error when the artifact is unknown, malformed, corrupt,
    /// conflicts with a loaded artifact, or disagrees with its root descriptor.
    pub fn load_shard(&mut self, bytes: &[u8]) -> Result<ShardId, ShardError> {
        let digest = sha256(bytes);
        let Some(descriptor) = self
            .descriptors
            .iter()
            .find(|descriptor| descriptor.digest == digest)
        else {
            let id = decode_shard_id(bytes)?;
            if self.loaded.contains_key(&id) {
                return Err(ShardError::ConflictingShard(id));
            }
            if self
                .descriptors
                .iter()
                .any(|descriptor| descriptor.id == id)
            {
                return Err(ShardError::DigestMismatch { id });
            }
            return Err(ShardError::UnknownShard(id));
        };

        if let Some(loaded) = self.loaded.get(&descriptor.id) {
            if loaded.digest == digest {
                return Ok(descriptor.id);
            }
            return Err(ShardError::ConflictingShard(descriptor.id));
        }
        if bytes.len() != descriptor.encoded_len {
            return Err(ShardError::LengthMismatch {
                id: descriptor.id,
                expected: descriptor.encoded_len,
                actual: bytes.len(),
            });
        }

        let shard = decode_shard(bytes, descriptor, self.posts.len())?;
        let id = descriptor.id;
        self.loaded_bytes = self
            .loaded_bytes
            .checked_add(bytes.len())
            .ok_or(ShardError::MalformedShard("loaded byte count overflowed"))?;
        self.loaded.insert(id, shard);
        Ok(id)
    }

    /// Searches loaded shards while preserving monolithic exact-index ranking.
    ///
    /// # Errors
    ///
    /// Returns [`ShardError::NeedsShards`] when any lexical shard needed by the
    /// query has not been loaded.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<&PostId>, ShardError> {
        let search_terms = tokenize(query);
        let required = self.required_shards_for_terms(&search_terms);
        let missing: Vec<ShardId> = required
            .iter()
            .copied()
            .filter(|id| !self.loaded.contains_key(id))
            .collect();
        if !missing.is_empty() {
            return Err(ShardError::NeedsShards(missing));
        }

        let mut scores = title_scores(&self.posts, &search_terms);
        let mut content_matches = vec![false; self.posts.len()];

        for search_term in &search_terms {
            content_matches.fill(false);
            for id in self.required_shards_for_term(search_term) {
                let shard = self
                    .loaded
                    .get(&id)
                    .ok_or_else(|| ShardError::NeedsShards(vec![id]))?;
                mark_content_matches(shard, search_term, &mut content_matches);
            }
            for (score, &content_match) in scores.iter_mut().zip(&content_matches) {
                *score = score.saturating_add(usize::from(content_match));
            }
        }

        Ok(ranked_posts(&self.posts, scores, limit))
    }

    /// Returns the number of currently loaded lexical shards.
    #[must_use]
    pub fn loaded_shard_count(&self) -> usize {
        self.loaded.len()
    }

    /// Returns the sum of the encoded artifact lengths of loaded lexical shards.
    ///
    /// This is an accounting metric for transferred shard data, not the retained
    /// decoded heap size or the WebAssembly linear-memory allocation.
    #[must_use]
    pub const fn loaded_shard_bytes(&self) -> usize {
        self.loaded_bytes
    }

    fn required_shards_for_terms(&self, terms: &[String]) -> Vec<ShardId> {
        terms
            .iter()
            .flat_map(|term| self.required_shards_for_term(term))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn required_shards_for_term(&self, term: &str) -> Vec<ShardId> {
        let prefix = term.chars().count() >= MIN_PREFIX_LEN;
        self.descriptors
            .iter()
            .filter(|descriptor| {
                if prefix {
                    descriptor.last_term.as_str() >= term
                        && (descriptor.first_term.as_str() <= term
                            || descriptor.first_term.starts_with(term))
                } else {
                    descriptor.first_term.as_str() <= term && term <= descriptor.last_term.as_str()
                }
            })
            .map(|descriptor| descriptor.id)
            .collect()
    }
}

pub(crate) fn build_bundle(
    index: &SearchIndex,
    config: ShardConfig,
) -> Result<ShardedIndexBundle, ShardError> {
    match &index.data {
        SearchIndexData::Exact(exact) => build_exact_bundle(exact, config),
        SearchIndexData::Xor8(_) => Err(ShardError::UnsupportedBackend),
    }
}

fn build_exact_bundle(
    index: &ExactIndex,
    config: ShardConfig,
) -> Result<ShardedIndexBundle, ShardError> {
    validate_exact_index(index)?;
    let ranges = partition_ranges(index, config)?;
    let mut artifacts = Vec::with_capacity(ranges.len());

    for (number, (start, end)) in ranges.into_iter().enumerate() {
        let numeric_id = u32::try_from(number)
            .map_err(|_error| ShardError::MalformedShard("too many lexical shards"))?;
        let id = ShardId::new(numeric_id);
        let bytes = encode_shard(
            id,
            index
                .terms
                .get(start..end)
                .ok_or(ShardError::MalformedShard("shard term range is invalid"))?,
            index
                .postings
                .get(start..end)
                .ok_or(ShardError::MalformedShard("shard posting range is invalid"))?,
            index.posts.len(),
        )?;
        let digest = sha256(&bytes);
        let first_term = index
            .terms
            .get(start)
            .cloned()
            .ok_or(ShardError::MalformedShard("shard has no first term"))?;
        let last_position = end
            .checked_sub(1)
            .ok_or(ShardError::MalformedShard("shard range is empty"))?;
        let last_term = index
            .terms
            .get(last_position)
            .cloned()
            .ok_or(ShardError::MalformedShard("shard has no last term"))?;
        let descriptor = ShardDescriptor {
            id,
            first_term,
            last_term,
            filename: shard_filename(&digest),
            encoded_len: bytes.len(),
            digest,
        };
        artifacts.push(ShardArtifact { descriptor, bytes });
    }

    let descriptors: Vec<ShardDescriptor> = artifacts
        .iter()
        .map(|artifact| artifact.descriptor.clone())
        .collect();
    let root_bytes = encode_root(&index.posts, &descriptors);
    Ok(ShardedIndexBundle {
        root_bytes,
        shards: artifacts,
    })
}

fn validate_exact_index(index: &ExactIndex) -> Result<(), ShardError> {
    if index.terms.len() != index.postings.len() {
        return Err(ShardError::MalformedShard(
            "terms and postings must have equal lengths",
        ));
    }
    if index.terms.iter().any(String::is_empty)
        || !index.terms.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(ShardError::MalformedShard(
            "terms must be non-empty, sorted, and unique",
        ));
    }
    for postings in &index.postings {
        validate_postings(postings, index.posts.len())?;
    }
    Ok(())
}

fn partition_ranges(
    index: &ExactIndex,
    config: ShardConfig,
) -> Result<Vec<(usize, usize)>, ShardError> {
    let mut ranges = Vec::new();
    let mut start = 0_usize;
    while start < index.terms.len() {
        let number = u32::try_from(ranges.len())
            .map_err(|_error| ShardError::MalformedShard("too many lexical shards"))?;
        let id = ShardId::new(number);
        let mut end = start;
        let mut payload_len = 0_usize;
        while end < index.terms.len() {
            let term = index
                .terms
                .get(end)
                .ok_or(ShardError::MalformedShard("term range is invalid"))?;
            let postings = index
                .postings
                .get(end)
                .ok_or(ShardError::MalformedShard("posting range is invalid"))?;
            let term_len = encoded_term_len(term, postings)?;
            let candidate_payload = payload_len
                .checked_add(term_len)
                .ok_or(ShardError::MalformedShard("shard size overflowed"))?;
            let candidate_count = end
                .checked_sub(start)
                .and_then(|count| count.checked_add(1))
                .ok_or(ShardError::MalformedShard("shard term count overflowed"))?;
            let candidate_len = shard_encoded_len(id, candidate_count, candidate_payload)?;
            if end > start && candidate_len > config.target_bytes() {
                break;
            }
            payload_len = candidate_payload;
            end = end
                .checked_add(1)
                .ok_or(ShardError::MalformedShard("shard range overflowed"))?;
            if candidate_len > config.target_bytes() {
                break;
            }
        }
        ranges.push((start, end));
        start = end;
    }
    Ok(ranges)
}

fn encoded_term_len(term: &str, postings: &[usize]) -> Result<usize, ShardError> {
    let mut length = varint_len(term.len())
        .checked_add(term.len())
        .and_then(|value| value.checked_add(varint_len(postings.len())))
        .ok_or(ShardError::MalformedShard("encoded term size overflowed"))?;
    let mut previous = 0_usize;
    for (position, &document) in postings.iter().enumerate() {
        let delta = posting_delta(position, document, previous)?;
        length = length
            .checked_add(varint_len(delta))
            .ok_or(ShardError::MalformedShard(
                "encoded posting size overflowed",
            ))?;
        previous = document;
    }
    Ok(length)
}

fn shard_encoded_len(
    id: ShardId,
    term_count: usize,
    payload_len: usize,
) -> Result<usize, ShardError> {
    SHARD_MAGIC
        .len()
        .checked_add(1)
        .and_then(|value| value.checked_add(varint_len(id_value(id))))
        .and_then(|value| value.checked_add(varint_len(term_count)))
        .and_then(|value| value.checked_add(payload_len))
        .ok_or(ShardError::MalformedShard("encoded shard size overflowed"))
}

fn encode_root(posts: &[PostId], descriptors: &[ShardDescriptor]) -> Vec<u8> {
    let mut encoded_posts = Vec::new();
    write_varint(&mut encoded_posts, posts.len());
    for post in posts {
        write_string(&mut encoded_posts, &post.title);
        write_string(&mut encoded_posts, &post.url);
        write_string(&mut encoded_posts, &post.meta);
    }

    let mut output = Vec::new();
    output.extend_from_slice(ROOT_MAGIC);
    output.push(ROOT_VERSION);
    write_varint(&mut output, encoded_posts.len());
    output.extend_from_slice(&encoded_posts);
    write_varint(&mut output, descriptors.len());
    for descriptor in descriptors {
        write_varint(&mut output, id_value(descriptor.id));
        write_string(&mut output, &descriptor.first_term);
        write_string(&mut output, &descriptor.last_term);
        write_varint(&mut output, descriptor.encoded_len);
        output.extend_from_slice(&descriptor.digest);
        write_string(&mut output, &descriptor.filename);
    }
    output
}

fn decode_posts(input: &[u8]) -> Result<Vec<PostId>, &'static str> {
    const MINIMUM_ENCODED_POST_BYTES: usize = 3;

    let mut cursor = 0_usize;
    let post_count = read_varint(input, &mut cursor)?;
    let remaining = input.len().saturating_sub(cursor);
    if post_count > remaining / MINIMUM_ENCODED_POST_BYTES {
        return Err("post count exceeds the post payload bounds");
    }

    let mut posts = Vec::with_capacity(post_count);
    for _position in 0..post_count {
        let title = read_string(input, &mut cursor)?;
        let url = read_string(input, &mut cursor)?;
        let meta = read_string(input, &mut cursor)?;
        posts.push(PostId { title, url, meta });
    }
    if cursor != input.len() {
        return Err("post payload has trailing bytes");
    }
    Ok(posts)
}

fn decode_root(bytes: &[u8]) -> Result<(Vec<PostId>, Vec<ShardDescriptor>), ShardError> {
    let mut cursor = envelope_cursor(bytes, ROOT_MAGIC, ROOT_VERSION, true)?;
    let posts_len = read_varint(bytes, &mut cursor).map_err(ShardError::MalformedRoot)?;
    let posts_section =
        take_section(bytes, &mut cursor, posts_len).map_err(ShardError::MalformedRoot)?;
    let posts = decode_posts(posts_section).map_err(ShardError::MalformedRoot)?;

    let descriptor_count = read_varint(bytes, &mut cursor).map_err(ShardError::MalformedRoot)?;
    if descriptor_count
        > bytes
            .len()
            .saturating_sub(cursor)
            .checked_div(MINIMUM_DESCRIPTOR_BYTES)
            .unwrap_or(0)
    {
        return Err(ShardError::MalformedRoot(
            "descriptor count exceeds the remaining root bytes",
        ));
    }
    if posts.is_empty() && descriptor_count != 0 {
        return Err(ShardError::MalformedRoot(
            "an empty post table cannot have lexical shards",
        ));
    }

    let mut descriptors = Vec::with_capacity(descriptor_count);
    for position in 0..descriptor_count {
        let encoded_id = read_varint(bytes, &mut cursor).map_err(ShardError::MalformedRoot)?;
        let numeric_id = u32::try_from(encoded_id)
            .map_err(|_error| ShardError::MalformedRoot("shard id exceeds u32"))?;
        let id = ShardId::new(numeric_id);
        let expected = u32::try_from(position)
            .map_err(|_error| ShardError::MalformedRoot("too many descriptors"))?;
        if numeric_id != expected {
            return Err(ShardError::MalformedRoot(
                "shard ids must be contiguous and ordered",
            ));
        }
        let first_term = read_string(bytes, &mut cursor).map_err(ShardError::MalformedRoot)?;
        let last_term = read_string(bytes, &mut cursor).map_err(ShardError::MalformedRoot)?;
        if first_term.is_empty() || first_term > last_term {
            return Err(ShardError::MalformedRoot(
                "descriptor term range is empty or reversed",
            ));
        }
        if descriptors
            .last()
            .is_some_and(|previous: &ShardDescriptor| previous.last_term >= first_term)
        {
            return Err(ShardError::MalformedRoot(
                "descriptor term ranges must be strictly ordered",
            ));
        }
        let encoded_len = read_varint(bytes, &mut cursor).map_err(ShardError::MalformedRoot)?;
        if encoded_len == 0 {
            return Err(ShardError::MalformedRoot(
                "descriptor artifact length must be nonzero",
            ));
        }
        let digest_section =
            take_section(bytes, &mut cursor, DIGEST_LEN).map_err(ShardError::MalformedRoot)?;
        let digest: [u8; DIGEST_LEN] = digest_section
            .try_into()
            .map_err(|_error| ShardError::MalformedRoot("digest length is invalid"))?;
        if descriptors
            .iter()
            .any(|descriptor| descriptor.digest == digest)
        {
            return Err(ShardError::MalformedRoot(
                "descriptor digests must be unique",
            ));
        }
        let filename = read_string(bytes, &mut cursor).map_err(ShardError::MalformedRoot)?;
        if filename != shard_filename(&digest) {
            return Err(ShardError::MalformedRoot(
                "descriptor filename is not content-addressed",
            ));
        }
        descriptors.push(ShardDescriptor {
            id,
            first_term,
            last_term,
            filename,
            encoded_len,
            digest,
        });
    }
    if cursor != bytes.len() {
        return Err(ShardError::MalformedRoot("root has trailing bytes"));
    }
    Ok((posts, descriptors))
}

fn encode_shard(
    id: ShardId,
    terms: &[String],
    postings: &[Vec<usize>],
    document_count: usize,
) -> Result<Vec<u8>, ShardError> {
    if terms.is_empty() || terms.len() != postings.len() {
        return Err(ShardError::MalformedShard(
            "shard terms and postings must be non-empty and aligned",
        ));
    }
    let mut output = Vec::new();
    output.extend_from_slice(SHARD_MAGIC);
    output.push(SHARD_VERSION);
    write_varint(&mut output, id_value(id));
    write_varint(&mut output, terms.len());
    for (term, posting_list) in terms.iter().zip(postings) {
        if term.is_empty() {
            return Err(ShardError::MalformedShard("shard term is empty"));
        }
        validate_postings(posting_list, document_count)?;
        write_string(&mut output, term);
        write_varint(&mut output, posting_list.len());
        let mut previous = 0_usize;
        for (position, &document) in posting_list.iter().enumerate() {
            let delta = posting_delta(position, document, previous)?;
            write_varint(&mut output, delta);
            previous = document;
        }
    }
    Ok(output)
}

fn decode_shard(
    bytes: &[u8],
    descriptor: &ShardDescriptor,
    document_count: usize,
) -> Result<LoadedShard, ShardError> {
    let mut cursor = envelope_cursor(bytes, SHARD_MAGIC, SHARD_VERSION, false)?;
    let encoded_id = read_varint(bytes, &mut cursor).map_err(ShardError::MalformedShard)?;
    let numeric_id = u32::try_from(encoded_id)
        .map_err(|_error| ShardError::MalformedShard("shard id exceeds u32"))?;
    let actual = ShardId::new(numeric_id);
    if actual != descriptor.id {
        return Err(ShardError::WrongShardId {
            expected: descriptor.id,
            actual,
        });
    }
    let term_count = read_varint(bytes, &mut cursor).map_err(ShardError::MalformedShard)?;
    if term_count == 0
        || term_count
            > bytes
                .len()
                .saturating_sub(cursor)
                .checked_div(MINIMUM_TERM_BYTES)
                .unwrap_or(0)
    {
        return Err(ShardError::MalformedShard("shard term count is invalid"));
    }

    let mut terms = Vec::with_capacity(term_count);
    let mut postings = Vec::with_capacity(term_count);
    for _position in 0..term_count {
        let term = read_string(bytes, &mut cursor).map_err(ShardError::MalformedShard)?;
        if term.is_empty()
            || terms
                .last()
                .is_some_and(|previous: &String| previous >= &term)
        {
            return Err(ShardError::MalformedShard(
                "shard terms must be non-empty, sorted, and unique",
            ));
        }
        let posting_list = decode_posting_list(bytes, &mut cursor, document_count)?;
        terms.push(term);
        postings.push(posting_list);
    }
    if cursor != bytes.len() {
        return Err(ShardError::MalformedShard("shard has trailing bytes"));
    }
    if terms.first() != Some(&descriptor.first_term) || terms.last() != Some(&descriptor.last_term)
    {
        return Err(ShardError::MalformedShard(
            "shard terms disagree with the descriptor range",
        ));
    }
    Ok(LoadedShard {
        terms,
        postings,
        digest: descriptor.digest,
    })
}

fn decode_shard_id(bytes: &[u8]) -> Result<ShardId, ShardError> {
    let mut cursor = envelope_cursor(bytes, SHARD_MAGIC, SHARD_VERSION, false)?;
    let encoded_id = read_varint(bytes, &mut cursor).map_err(ShardError::MalformedShard)?;
    let numeric_id = u32::try_from(encoded_id)
        .map_err(|_error| ShardError::MalformedShard("shard id exceeds u32"))?;
    Ok(ShardId::new(numeric_id))
}

fn decode_posting_list(
    bytes: &[u8],
    cursor: &mut usize,
    document_count: usize,
) -> Result<Vec<usize>, ShardError> {
    let posting_count = read_varint(bytes, cursor).map_err(ShardError::MalformedShard)?;
    if posting_count == 0
        || posting_count > document_count
        || posting_count > bytes.len().saturating_sub(*cursor)
    {
        return Err(ShardError::MalformedShard(
            "posting count is invalid for this root",
        ));
    }
    let mut documents = Vec::with_capacity(posting_count);
    let mut previous = 0_usize;
    for position in 0..posting_count {
        let delta = read_varint(bytes, cursor).map_err(ShardError::MalformedShard)?;
        if position > 0 && delta == 0 {
            return Err(ShardError::MalformedShard(
                "posting documents must be strictly increasing",
            ));
        }
        let document = previous
            .checked_add(delta)
            .ok_or(ShardError::MalformedShard("posting document overflowed"))?;
        if document >= document_count {
            return Err(ShardError::MalformedShard(
                "posting document is out of range",
            ));
        }
        documents.push(document);
        previous = document;
    }
    Ok(documents)
}

fn validate_postings(postings: &[usize], document_count: usize) -> Result<(), ShardError> {
    if postings.is_empty() {
        return Err(ShardError::MalformedShard(
            "posting lists must not be empty",
        ));
    }
    let mut previous = None;
    for &document in postings {
        if document >= document_count || previous.is_some_and(|value| value >= document) {
            return Err(ShardError::MalformedShard(
                "posting documents must be in range and strictly increasing",
            ));
        }
        previous = Some(document);
    }
    Ok(())
}

fn posting_delta(position: usize, document: usize, previous: usize) -> Result<usize, ShardError> {
    if position == 0 {
        Ok(document)
    } else {
        document
            .checked_sub(previous)
            .filter(|delta| *delta > 0)
            .ok_or(ShardError::MalformedShard(
                "posting documents must be strictly increasing",
            ))
    }
}

fn mark_content_matches(shard: &LoadedShard, search_term: &str, matches: &mut [bool]) {
    let matching_terms = matching_term_range(&shard.terms, search_term);
    for posting_list in shard
        .postings
        .iter()
        .skip(matching_terms.start)
        .take(matching_terms.len())
    {
        for &document in posting_list {
            if let Some(content_match) = matches.get_mut(document) {
                *content_match = true;
            }
        }
    }
}

fn envelope_cursor(
    bytes: &[u8],
    magic: &[u8],
    version: u8,
    root: bool,
) -> Result<usize, ShardError> {
    let Some(after_magic) = bytes.strip_prefix(magic) else {
        return if root {
            Err(ShardError::MalformedRoot("magic is missing"))
        } else {
            Err(ShardError::MalformedShard("magic is missing"))
        };
    };
    let Some((&actual_version, _payload)) = after_magic.split_first() else {
        return if root {
            Err(ShardError::MalformedRoot("version is missing"))
        } else {
            Err(ShardError::MalformedShard("version is missing"))
        };
    };
    if actual_version != version {
        return if root {
            Err(ShardError::MalformedRoot("version is unsupported"))
        } else {
            Err(ShardError::MalformedShard("version is unsupported"))
        };
    }
    magic.len().checked_add(1).ok_or(if root {
        ShardError::MalformedRoot("envelope cursor overflowed")
    } else {
        ShardError::MalformedShard("envelope cursor overflowed")
    })
}

fn write_string(output: &mut Vec<u8>, value: &str) {
    write_varint(output, value.len());
    output.extend_from_slice(value.as_bytes());
}

fn read_string(input: &[u8], cursor: &mut usize) -> Result<String, &'static str> {
    let length = read_varint(input, cursor)?;
    let section = take_section(input, cursor, length)?;
    let value = std::str::from_utf8(section).map_err(|_error| "string is not valid UTF-8")?;
    Ok(value.to_owned())
}

fn take_section<'input>(
    input: &'input [u8],
    cursor: &mut usize,
    length: usize,
) -> Result<&'input [u8], &'static str> {
    let end = (*cursor)
        .checked_add(length)
        .ok_or("section length overflowed")?;
    let section = input
        .get(*cursor..end)
        .ok_or("section extends beyond the envelope")?;
    *cursor = end;
    Ok(section)
}

fn write_varint(output: &mut Vec<u8>, mut value: usize) {
    while value >= 0x80 {
        output.push((value.to_le_bytes()[0] & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value.to_le_bytes()[0]);
}

fn read_varint(input: &[u8], cursor: &mut usize) -> Result<usize, &'static str> {
    let start = *cursor;
    let mut value = 0_usize;
    for shift in (0..usize::BITS).step_by(7) {
        let byte = input.get(*cursor).copied().ok_or("varint is truncated")?;
        *cursor = (*cursor).checked_add(1).ok_or("varint cursor overflowed")?;
        let payload = usize::from(byte & 0x7f);
        if payload > (usize::MAX >> shift) {
            return Err("varint exceeds the platform range");
        }
        value |= payload << shift;
        if byte & 0x80 == 0 {
            let consumed = (*cursor)
                .checked_sub(start)
                .ok_or("varint cursor moved backwards")?;
            if consumed != varint_len(value) {
                return Err("varint is not canonically encoded");
            }
            return Ok(value);
        }
    }
    Err("varint is unterminated")
}

const fn varint_len(mut value: usize) -> usize {
    let mut length = 1_usize;
    while value >= 0x80 {
        length = length.saturating_add(1);
        value >>= 7;
    }
    length
}

fn id_value(id: ShardId) -> usize {
    match usize::try_from(id.get()) {
        Ok(value) => value,
        Err(_error) => usize::MAX,
    }
}

fn sha256(bytes: &[u8]) -> [u8; DIGEST_LEN] {
    Sha256::digest(bytes).into()
}

fn shard_filename(digest: &[u8; DIGEST_LEN]) -> String {
    format!("{}{SHARD_FILE_SUFFIX}", digest_hex(digest))
}

fn digest_hex(digest: &[u8; DIGEST_LEN]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(DIGEST_LEN.saturating_mul(2));
    for &byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}
