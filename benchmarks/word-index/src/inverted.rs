use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};

const COMPACT_MAGIC: &[u8; 4] = b"IIV1";

pub type Entries = Vec<(String, Vec<u32>)>;

pub struct PrefixResult {
    pub documents: Vec<u32>,
    pub completions: usize,
}

pub enum ExactInvertedIndex {
    Bincode {
        entries: Entries,
        serialized: Vec<u8>,
    },
    Compact(CompactIndex),
}

pub struct CompactIndex {
    terms: Vec<String>,
    posting_offsets: Vec<usize>,
    serialized: Vec<u8>,
}

impl ExactInvertedIndex {
    pub fn build_all(entries: &Entries) -> Result<Vec<Self>> {
        let bincode_serialized =
            bincode::serde::encode_to_vec(entries, bincode::config::standard())
                .context("failed to serialize simple inverted index")?;
        let (decoded, consumed): (Entries, usize) =
            bincode::serde::decode_from_slice(&bincode_serialized, bincode::config::standard())
                .context("failed to deserialize simple inverted index")?;
        ensure!(
            consumed == bincode_serialized.len(),
            "simple inverted index left trailing bytes"
        );
        ensure!(
            decoded == *entries,
            "simple inverted index did not round-trip"
        );

        let compact_serialized = CompactIndex::encode(entries)?;
        let compact = CompactIndex::from_serialized(compact_serialized)?;
        ensure!(
            compact.entries() == *entries,
            "compact inverted index did not round-trip"
        );

        Ok(vec![
            Self::Bincode {
                entries: decoded,
                serialized: bincode_serialized,
            },
            Self::Compact(compact),
        ])
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Bincode { .. } => "inverted bincode standard",
            Self::Compact(_) => "inverted raw+delta-varint",
        }
    }

    pub fn serialized(&self) -> &[u8] {
        match self {
            Self::Bincode { serialized, .. } => serialized,
            Self::Compact(index) => &index.serialized,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Bincode { entries, .. } => entries.len(),
            Self::Compact(index) => index.terms.len(),
        }
    }

    pub fn term(&self, index: usize) -> &str {
        match self {
            Self::Bincode { entries, .. } => &entries[index].0,
            Self::Compact(compact) => &compact.terms[index],
        }
    }

    pub fn postings(&self, index: usize) -> Vec<u32> {
        match self {
            Self::Bincode { entries, .. } => entries[index].1.clone(),
            Self::Compact(compact) => compact.decode_postings(index),
        }
    }

    pub fn exact_query(&self, term: &str) -> Vec<u32> {
        let index = self.lower_bound(term);
        if index < self.len() && self.term(index) == term {
            self.postings(index)
        } else {
            Vec::new()
        }
    }

    pub fn prefix_query(
        &self,
        prefix: &str,
        completion_limit: Option<usize>,
        document_count: usize,
    ) -> PrefixResult {
        let mut seen = vec![false; document_count];
        let mut completions = 0;
        let mut index = self.lower_bound(prefix);

        while index < self.len() && self.term(index).starts_with(prefix) {
            if completion_limit.is_some_and(|limit| completions == limit) {
                break;
            }
            for document in self.postings(index) {
                seen[document as usize] = true;
            }
            completions += 1;
            index += 1;
        }

        let documents = seen
            .into_iter()
            .enumerate()
            .filter_map(|(document, present)| present.then_some(document as u32))
            .collect();
        PrefixResult {
            documents,
            completions,
        }
    }

    fn lower_bound(&self, term: &str) -> usize {
        let mut lo = 0;
        let mut hi = self.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.term(mid) < term {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

impl CompactIndex {
    fn encode(entries: &Entries) -> Result<Vec<u8>> {
        let mut vocabulary = entries
            .iter()
            .map(|(term, _)| term.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes();
        vocabulary.push(b'\n');

        let mut serialized = Vec::new();
        serialized.extend_from_slice(COMPACT_MAGIC);
        write_varint(&mut serialized, entries.len() as u64);
        write_varint(&mut serialized, vocabulary.len() as u64);
        serialized.extend_from_slice(&vocabulary);

        for (_, postings) in entries {
            write_varint(&mut serialized, postings.len() as u64);
            let mut previous = 0_u32;
            for (position, &document) in postings.iter().enumerate() {
                let delta = if position == 0 {
                    document
                } else {
                    document
                        .checked_sub(previous)
                        .context("posting lists must be sorted")?
                };
                ensure!(
                    position == 0 || delta > 0,
                    "posting lists must be strictly increasing"
                );
                write_varint(&mut serialized, u64::from(delta));
                previous = document;
            }
        }
        Ok(serialized)
    }

    fn from_serialized(serialized: Vec<u8>) -> Result<Self> {
        ensure!(
            serialized.starts_with(COMPACT_MAGIC),
            "compact inverted index has invalid magic"
        );
        let mut cursor = COMPACT_MAGIC.len();
        let term_count = usize::try_from(read_varint(&serialized, &mut cursor)?)?;
        let vocabulary_len = usize::try_from(read_varint(&serialized, &mut cursor)?)?;
        let vocabulary_end = cursor
            .checked_add(vocabulary_len)
            .context("compact vocabulary length overflowed")?;
        ensure!(
            vocabulary_end <= serialized.len(),
            "compact vocabulary extends beyond payload"
        );
        let vocabulary = std::str::from_utf8(&serialized[cursor..vocabulary_end])
            .context("compact vocabulary is not UTF-8")?;
        ensure!(
            vocabulary.ends_with('\n'),
            "compact vocabulary must end in a newline"
        );
        let terms: Vec<String> = vocabulary.lines().map(str::to_owned).collect();
        ensure!(
            terms.len() == term_count,
            "compact vocabulary term count does not match header"
        );
        ensure!(
            terms.windows(2).all(|pair| pair[0] < pair[1]),
            "compact vocabulary must be sorted and unique"
        );

        cursor = vocabulary_end;
        let mut posting_offsets = Vec::with_capacity(term_count);
        for _ in 0..term_count {
            posting_offsets.push(cursor);
            let posting_count = usize::try_from(read_varint(&serialized, &mut cursor)?)?;
            ensure!(posting_count > 0, "posting lists must not be empty");
            let mut previous = 0_u32;
            for position in 0..posting_count {
                let delta = u32::try_from(read_varint(&serialized, &mut cursor)?)?;
                ensure!(
                    position == 0 || delta > 0,
                    "posting deltas after the first must be positive"
                );
                previous = previous
                    .checked_add(delta)
                    .context("posting document ID overflowed")?;
            }
        }
        ensure!(
            cursor == serialized.len(),
            "compact inverted index has trailing bytes"
        );

        Ok(Self {
            terms,
            posting_offsets,
            serialized,
        })
    }

    fn decode_postings(&self, index: usize) -> Vec<u32> {
        let mut cursor = self.posting_offsets[index];
        let count = read_varint(&self.serialized, &mut cursor)
            .expect("validated compact posting length") as usize;
        let mut postings = Vec::with_capacity(count);
        let mut previous = 0_u32;
        for _ in 0..count {
            let delta = read_varint(&self.serialized, &mut cursor)
                .expect("validated compact posting delta") as u32;
            previous += delta;
            postings.push(previous);
        }
        postings
    }

    fn entries(&self) -> Entries {
        self.terms
            .iter()
            .enumerate()
            .map(|(index, term)| (term.clone(), self.decode_postings(index)))
            .collect()
    }
}

pub fn build_entries(posts: &[BTreeSet<String>]) -> Result<Entries> {
    ensure!(posts.len() <= u32::MAX as usize, "too many documents");
    let mut postings = BTreeMap::<String, Vec<u32>>::new();
    for (document, terms) in posts.iter().enumerate() {
        let document = u32::try_from(document)?;
        for term in terms {
            postings.entry(term.clone()).or_default().push(document);
        }
    }
    Ok(postings.into_iter().collect())
}

pub fn baseline_prefix_query(
    entries: &Entries,
    prefix: &str,
    completion_limit: Option<usize>,
    document_count: usize,
) -> PrefixResult {
    let start = entries.partition_point(|(term, _)| term.as_str() < prefix);
    let mut seen = vec![false; document_count];
    let mut completions = 0;
    for (_, postings) in entries[start..]
        .iter()
        .take_while(|(term, _)| term.starts_with(prefix))
    {
        if completion_limit.is_some_and(|limit| completions == limit) {
            break;
        }
        for &document in postings {
            seen[document as usize] = true;
        }
        completions += 1;
    }
    PrefixResult {
        documents: seen
            .into_iter()
            .enumerate()
            .filter_map(|(document, present)| present.then_some(document as u32))
            .collect(),
        completions,
    }
}

fn write_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn read_varint(input: &[u8], cursor: &mut usize) -> Result<u64> {
    let mut value = 0_u64;
    for shift in (0..=63).step_by(7) {
        let Some(&byte) = input.get(*cursor) else {
            bail!("truncated varint");
        };
        *cursor += 1;
        let payload = u64::from(byte & 0x7f);
        ensure!(shift != 63 || payload <= 1, "varint exceeds 64-bit range");
        value |= payload << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("unterminated varint")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Entries {
        vec![
            ("alpha".to_owned(), vec![0, 2, 7]),
            ("alphabet".to_owned(), vec![1, 2]),
            ("beta".to_owned(), vec![0, 7]),
        ]
    }

    #[test]
    fn compact_index_round_trips_and_queries() {
        let entries = entries();
        let serialized = CompactIndex::encode(&entries).unwrap();
        let index = CompactIndex::from_serialized(serialized).unwrap();
        assert_eq!(index.entries(), entries);
        let index = ExactInvertedIndex::Compact(index);
        assert_eq!(index.exact_query("alpha"), [0, 2, 7]);
        assert!(index.exact_query("missing").is_empty());
        assert_eq!(index.prefix_query("alph", None, 8).documents, [0, 1, 2, 7]);

        let capped = index.prefix_query("alph", Some(1), 8);
        assert_eq!(capped.documents, [0, 2, 7]);
        assert_eq!(capped.completions, 1);
    }

    #[test]
    fn varints_round_trip_boundaries() {
        for expected in [0, 1, 127, 128, 255, 16_384, u32::MAX as u64, u64::MAX] {
            let mut encoded = Vec::new();
            write_varint(&mut encoded, expected);
            let mut cursor = 0;
            assert_eq!(read_varint(&encoded, &mut cursor).unwrap(), expected);
            assert_eq!(cursor, encoded.len());
        }
    }
}
