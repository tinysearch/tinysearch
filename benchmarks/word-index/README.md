# Word index benchmark

Self-contained benchmark harness for comparing global vocabulary representations and exact inverted indexes on the 73-post `endler.dev` corpus. The inverted indexes model replacing the per-document Xor8 filters with sorted exact terms and sorted document-ID posting lists.

This benchmark was added because the experimental global-prefix path can probe up to 32 terms in each document's Xor8 filter. Each probe is probabilistic, so probing several completions compounds false-positive exposure. The inverted-index contenders instead enumerate matching terms and union their exact posting lists; they have no Xor false positives.

## Reproduce

The corpus must exist at `target/bench/endler-index.json`. From the repository root:

```sh
cd benchmarks/word-index
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --release -- ../../target/bench/endler-index.json
```

The harness requires `gzip` and `brotli` on `PATH`. It validates every unique real 3–8-character prefix against plain sorted-vocabulary and posting-list baselines before reporting measurements. Build artifacts remain under this directory's ignored `target/`.

## Results

Generated on 2026-08-14 with an Apple M1 Max, macOS 15.7.1 arm64, Rust 1.97.0, Apple gzip 457.140.3, and Brotli 1.2.0.

- Posts: 73
- Unique exact terms: 7,437
- Exact document-term entries: 22,121
- Production-shaped prefix vocabulary terms: 6,954
- Prefixes validated: 17,120 (`3`: 1,529; `4`: 2,877; `5`: 3,553; `6`: 3,545; `7`: 3,115; `8`: 2,501)
- Timing sample: 1,536 deterministic exact terms and 1,536 deterministic, length-stratified real prefixes; 16 repetitions per query

### Global vocabulary representations

These existing contenders materialize at most 32 owned completion strings per query.

| representation | bytes | bytes/term | gzip -9 | Brotli q11 | completion p50 (µs) | completion p95 (µs) |
|---|---:|---:|---:|---:|---:|---:|
| raw newline | 61,309 | 8.24 | 22,712 | 19,904 | 0.146 | 0.310 |
| front-coded/4 | 63,709 | 8.57 | 29,563 | 26,180 | 0.271 | 0.521 |
| front-coded/8 | 57,914 | 7.79 | 26,114 | 23,699 | 0.279 | 0.549 |
| front-coded/16 | 54,974 | 7.39 | 23,902 | 21,671 | 0.315 | 0.588 |
| front-coded/32 | 53,532 | 7.20 | 22,714 | 20,637 | 0.401 | 0.685 |
| front-coded/64 | 52,780 | 7.10 | 21,949 | 20,114 | 0.555 | 1.039 |
| front-coded/128 | 52,447 | 7.05 | 21,555 | 19,670 | 0.865 | 1.654 |
| `fst::Set` | 35,012 | 4.71 | 26,290 | 24,862 | 1.341 | 1.854 |
| `fcsd::Set` (bucket 8) | 42,176 | 5.67 | 22,092 | 19,372 | 0.747 | 1.122 |

### Exact inverted indexes

Both standalone contenders contain all 7,437 exact title/body terms and all 22,121 document-term relationships. Production omits same-document title terms from postings because titles are scored directly; the generated-WASM comparison below measures that shipped shape. Exact queries return one term's postings. Prefix queries find all matching terms and union their postings into sorted, unique document IDs. “Prefix all” is uncapped and exact; “prefix cap32” uses only the first 32 lexicographic completions.

| representation | bytes | gzip -9 | Brotli q11 | exact p50 (µs) | exact p95 (µs) | prefix all p50 (µs) | prefix all p95 (µs) | prefix cap32 p50 (µs) | prefix cap32 p95 (µs) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| bincode standard `Vec<(String, Vec<u32>)>` | 90,870 | 51,499 | 45,219 | 0.128 | 0.138 | 0.203 | 0.471 | 0.198 | 0.453 |
| raw newline vocabulary + delta-varint postings | 90,876 | 41,463 | 36,704 | 0.130 | 0.148 | 0.198 | 0.518 | 0.198 | 0.477 |

The compact format is only six bytes larger uncompressed because bincode standard already varint-encodes sequence lengths and `u32` values. Separating the newline vocabulary from posting-list lengths and delta-coded IDs is much more compressor-friendly: it saves 10,036 bytes (19.5%) under gzip and 8,515 bytes (18.8%) under Brotli relative to the simple bincode encoding. Its query timings are effectively tied with the simple representation in this run.

### Real-prefix completion counts

| prefix chars | prefixes | completions p50 | completions p95 | max | prefixes over 32 |
|---|---:|---:|---:|---:|---:|
| 3 | 1,529 | 2 | 17 | 137 | 17 |
| 4 | 2,877 | 1 | 7 | 60 | 4 |
| 5 | 3,553 | 1 | 5 | 37 | 1 |
| 6 | 3,545 | 1 | 4 | 12 | 0 |
| 7 | 3,115 | 1 | 3 | 12 | 0 |
| 8 | 2,501 | 1 | 3 | 12 | 0 |
| all | 17,120 | 1 | 5 | 137 | 22 |

Only 22 of 17,120 real prefixes (0.13%) exceed 32 completions, which is why the uncapped and cap32 p50/p95 timings are nearly identical. Uncapped queries preserve exact prefix semantics for the long tail; cap32 can omit documents reached only by later completions.

### PostId-independent total storage comparison

The production-shaped baseline serializes exact Xor8 filters and the discarded 6,954-term vocabulary experiment with bincode legacy configuration and its `tinysearch\x01` envelope. PostIds are excluded from every row so the table compares only the replaceable search-index component.

| representation | bytes | gzip -9 | Brotli q11 | uncompressed delta vs Xor + production-shaped vocabulary |
|---|---:|---:|---:|---:|
| Xor8 exact filters only, legacy | 31,193 | 27,030 | 25,666 | -59,340 |
| Xor8 + production-shaped vocabulary/envelope | 90,533 | 50,136 | 44,974 | 0 |
| Xor8 + full vocabulary/envelope | 92,520 | 51,114 | 45,782 | +1,987 |
| inverted bincode standard | 90,870 | 51,499 | 45,219 | +337 |
| inverted raw+delta-varint | 90,876 | 41,463 | 36,704 | +343 |

Against the production-shaped Xor+vocabulary component, the compact exact index costs 343 bytes (0.4%) uncompressed but saves 8,673 bytes (17.3%) with gzip and 8,270 bytes (18.4%) with Brotli. It also replaces probabilistic multi-probe membership with exact posting-list lookup.

### Generated WASM comparison

Both current indexers were measured in complete generated WASM modules against `origin/master`. All artifacts use the same 73-post corpus and `wasm-opt --enable-bulk-memory -Oz`.

| artifact | storage | optimized WASM | gzip -9 | Brotli q11 |
|---|---:|---:|---:|---:|
| `origin/master` Xor8 | 38,078 | 129,298 | 72,798 | 65,018 |
| current `--indexer xor8` | 38,078 | 136,309 | 75,155 | 66,942 |
| current default exact | 95,676 | 178,911 | 83,003 | 71,798 |
| exact vs current Xor8 | +57,598 | +42,602 | +7,848 | +4,856 |

The complete artifact grows much less than the standalone index component suggests because the new format replaces Xor filters and their query/decoding paths rather than layering another data structure on top.

### Per-document Xor8 prefix checkpoints

This corrects the previous table, which accidentally used bincode standard encoding. All rows below serialize one `HashProxy<String, DefaultHasher, Xor8>` per document with the same bincode legacy configuration used by production.

| checkpoints | total filter entries | serialized bytes | gzip -9 | Brotli q11 | growth (bytes) | growth (%) |
|---|---:|---:|---:|---:|---:|---:|
| exact terms | 22,121 | 31,193 | 27,030 | 25,666 | 0 | 0.0% |
| `[3]` | 35,508 | 47,669 | 42,360 | 40,560 | +16,476 | +52.8% |
| `[3,4]` | 48,548 | 63,707 | 57,320 | 55,167 | +32,514 | +104.2% |
| `[3,4,6,8]` | 60,988 | 78,992 | 71,507 | 69,036 | +47,799 | +153.2% |

## Interpretation

- For the standalone completion vocabulary, `fst::Set` is smallest uncompressed, while `fcsd::Set` is smallest under Brotli. Raw newline remains the fastest and is highly compressible.
- A simple exact inverted index is already competitive with the production-shaped Xor+vocabulary component: +337 bytes uncompressed and +245 bytes under Brotli, while eliminating Xor false positives.
- The maintainable raw+delta-varint layout has no raw-size advantage over bincode standard on this corpus, but its grouped data compresses substantially better. This makes it the strongest exact-index storage result measured here.
- Uncapped exact prefix unions are practical for the observed distribution: p95 is around 0.5 µs in this in-process harness, and only 0.13% of real prefixes have more than 32 completions.
- Timings do not include title weighting, multi-term scoring, final ranking, WASM boundary costs, or decompression/startup. They compare loaded lookup structures only.

## Method and caveats

- Tokenization mirrors `src/api.rs`: strip Markdown, retain Unicode alphabetic characters and apostrophes, lowercase, split whitespace, and remove the repository's tinysearch stopword list. The corpus has title and body fields but no metadata.
- The replacement inverted index deliberately includes every exact title/body term, including terms of three characters or fewer, because it replaces exact Xor membership as well as prefix expansion.
- The discarded Xor-plus-vocabulary experiment had different responsibilities. It contained body/metadata terms absent from the same post's title, then excluded terms with three or fewer characters. The “production-shaped” comparison reproduces those body-minus-title and `>3` rules; there is no metadata in this corpus.
- Raw vocabulary serialization is one sorted term per line with a final newline. The compact inverted format stores a four-byte magic, varint term count, varint vocabulary-byte length, the raw newline vocabulary, then a varint posting count and delta-coded document IDs for each term. Query offsets are rebuilt while decoding and are not serialized.
- The simple inverted representation is exactly `Vec<(String, Vec<u32>)>` encoded with bincode 2 standard configuration. Xor8 comparisons use bincode 2 legacy configuration because that is production's compatibility format.
- Exact-index validation checks all exact postings and both uncapped and cap32 results for every real 3–8-character prefix. Exact prefix unions have no false positives. The harness does not estimate Xor8's empirical false-positive rate; its motivation is that up to 32 probabilistic probes amplify that known risk.
- Prefix timings include materializing sorted result vectors. They are in-process p50/p95 wall-clock measurements, not Criterion confidence intervals; compare relative values and rerun on the deployment target.
- Compression pipes the exact serialized bytes through `gzip -9 -c` and `brotli -q 11 -c`.
- `fcsd` 0.2.0's native `predictive_iter` missed a validated corpus prefix (`aby` should return `abysmal`). The benchmark therefore uses decoder-based binary search followed by sequential decode. This preserves correctness but affects its timing.
