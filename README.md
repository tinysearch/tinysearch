# tinysearch

![Logo](logo.svg)

![CI](https://github.com/mre/tinysearch/workflows/CI/badge.svg)

tinysearch is a lightweight, fast, full-text search engine. It is designed for
static websites.

tinysearch is written in Rust, and then compiled to WebAssembly to run in a
browser.\
It can be used together with static site generators such as
[Jekyll](https://jekyllrb.com/), [Hugo](https://gohugo.io/),
[Zola](https://www.getzola.org/),
[Cobalt](https://github.com/cobalt-org/cobalt.rs), or
[Pelican](https://getpelican.com).

![Demo](tinysearch.gif)

## Is it tiny?

The current endler.dev index with 73 posts creates an optimized WASM payload of
179 kB (83 kB gzipped, 72 kB Brotli-compressed). That is smaller than the demo
image above; so yes.

## How it works

tinysearch is a Rust/WASM port of the Python code from the article
["Writing a full-text
search engine using Bloom filters"](https://www.stavros.io/posts/bloom-filter-search-engine/).
It can be seen as an alternative to [lunr.js](https://lunrjs.com/) and
[elasticlunr](http://elasticlunr.com/), which are too heavy for smaller websites
and load a lot of JavaScript.

By default, tinysearch stores one sorted vocabulary with exact posting lists
that map each word to the articles containing it. Document IDs are delta- and
varint-encoded, which keeps the index compact and compressible. Exact and prefix
searches use the same index and do not introduce probabilistic false positives.

WASM builds split the exact vocabulary into immutable lexical shards. A small
root routes each query to the relevant content-addressed shards, which the
JavaScript loader fetches and caches on demand. The WASM engine is independent
of corpus size and validates each shard before merging it into the live index.
See [Sharded indexes](docs/sharded-index.md) for the format, cache model, tests,
and initial size measurements.

The optional `xor8` indexer creates smaller probabilistic per-article filters.
It supports title prefixes, but body and metadata searches require complete
words. Previously generated Xor-filter indexes use this same path.

## Limitations

- With the default exact indexer, prefix matching starts once a query term
  reaches three characters. Shorter terms require an exact match.
- The `xor8` indexer only supports prefixes in titles.
- Query-selective lazy loading is available for the default exact backend. The
  optional Xor8 backend still embeds its complete index in the WASM module.
- Loaded exact shards remain in WASM memory for the lifetime of the engine. A
  long session that searches the entire vocabulary can eventually load the
  complete index.

## Installation

You can install tinysearch directly from crates.io:

```sh
cargo install tinysearch
```

To optimize the WebAssembly output, optionally install [binaryen](https://github.com/WebAssembly/binaryen). On macOS you can install it with [homebrew](https://brew.sh/):

```sh
brew install binaryen
```

Alternatively, you can download the binary from the [release page](https://github.com/WebAssembly/binaryen/releases) or use your OS package manager.

## Usage

A JSON file, which contains the content to index, is required as an input.
Please take a look at the [example file](fixtures/index.json).

ℹ️ The `body` field in the JSON document is optional and can be skipped to just
index post titles.

### Configuration

You can customize which fields are indexed and which are stored as metadata using a `tinysearch.toml` configuration file. Place this file in the same directory as your JSON index file.

```toml
[schema]
# Fields that will be indexed for full-text search
indexed_fields = ["title", "body", "description"]

# Fields returned as metadata and included in search
metadata_fields = ["author", "date", "category", "image_url"]

# Field that contains the URL for each document
url_field = "url"
```

If no configuration file is found, tinysearch will use the default schema (indexing `title` and `body` fields with `url` as the URL field).

Once you created the index, you can generate a WebAssembly search engine:

```sh
# Generate WASM files with demo for development
tinysearch -m wasm -p wasm_output fixtures/index.json

# Production-ready output (WASM only, no demo files)
tinysearch --release -m wasm -p wasm_output fixtures/index.json

# With optimization (requires wasm-opt from binaryen)
tinysearch --release -o -m wasm -p wasm_output fixtures/index.json

# Use the smaller Xor8 index format instead of the default exact index
tinysearch --indexer xor8 -m wasm -p wasm_output fixtures/index.json
```

This creates a dependency-free ES module loader, a corpus-independent WASM
engine, `tinysearch.root`, and content-addressed `.tinysearch-shard` files using
vanilla `cargo build` instead of `wasm-pack`. The default raw shard target is
64 KiB; tune it with `--shard-size`, for example `--shard-size 32768`.

Load and search it with:

```js
import { initTinysearch } from './tinysearch_engine.js';

const engine = await initTinysearch();
const results = await engine.search('rust wasm', 10);
```

The loader resolves the root, WASM, and shards relative to its own module URL.
Custom URLs and a custom `fetch` implementation can also be supplied.

### Migrating from 0.11

Version 0.12 makes the generated loader's `search()` method asynchronous for
both exact and Xor8 indexes. Callers must `await engine.search(...)`. The
original `init_tinysearch()` initializer remains available as an alias, but the
camel-case `initTinysearch()` spelling is preferred.

## Demo

Try the interactive demo with a single command:

```sh
make demo
```

This will generate WASM files and start a local server. Open http://localhost:8000/demo/ to try it out.

You can also take a look at the code examples for different static site generators [here](https://github.com/mre/tinysearch/tree/master/examples).

### Configuration Examples

#### E-commerce Site with Product Metadata

For an e-commerce site where you want to search product titles and descriptions but also store metadata like prices and image URLs:

```toml
[schema]
indexed_fields = ["title", "description", "category", "tags"]
metadata_fields = ["price", "image_url", "brand", "availability"]
url_field = "product_url"
```

JSON structure:
```json
[
    {
        "title": "Wireless Headphones",
        "description": "High-quality wireless headphones with noise cancellation",
        "category": "Electronics",
        "tags": "audio headphones wireless bluetooth",
        "product_url": "https://store.example.com/headphones-123",
        "price": "$199.99",
        "image_url": "https://store.example.com/images/headphones.jpg",
        "brand": "TechAudio",
        "availability": "In Stock"
    }
]
```

#### Blog with Author and Date Information

For a blog where you want to search titles and content but also store author and publication metadata:

```toml
[schema]
indexed_fields = ["title", "body", "excerpt"]
metadata_fields = ["author", "publish_date", "tags", "featured_image"]
url_field = "permalink"
```

#### Documentation Site

For a documentation site where you want extensive search across multiple content types:

```toml
[schema]
indexed_fields = ["title", "content", "section", "keywords"]
metadata_fields = ["version", "last_updated", "contributor"]
url_field = "doc_url"
```

## Library Usage (Experimental)

tinysearch can be used as a Rust library for programmatic search index generation and searching. This feature is experimental and the API may change.

Add tinysearch to your `Cargo.toml`:

```sh
cargo add tinysearch
```

Basic usage with the provided `BasicPost` struct:

```rust
use tinysearch::{BasicPost, TinySearch};
use std::collections::HashMap;

let posts = vec![
    BasicPost {
        title: "My Post".to_string(),
        url: "/my-post".to_string(),
        body: Some("Post content here".to_string()),
        meta: HashMap::new(),
    }
];

let search = TinySearch::new();
let index = search.build_index(&posts)?;
let results = search.search(&index, "content", 10);
```

Select `TinySearch::new().with_index_kind(IndexKind::Xor8)` to build the
probabilistic Xor8 representation. Both built-in indexers implement
`IndexBackend`.

For advanced usage including custom post types and configuration, see:
- [Basic library example](examples/library_basic/)
- [Advanced library example](examples/library_advanced/)

## Advanced Usage

For advanced usage options, run

```
tinysearch --help
```

Please check what's required to
[host WebAssembly in production](https://rustwasm.github.io/book/reference/deploying-to-production.html)
-- serve the WASM file as `application/wasm` and enable Brotli or gzip for the
WASM, root, loader, and shards. Content-addressed shards can use immutable cache
headers; the root should be revalidated.

## Docker

If you don't have a full Rust setup available, you can also use our
nightly-built Docker images.

Here is how to quickly try tinysearch with Docker:

```sh
# Download a sample blog index from endler.dev
curl -O https://raw.githubusercontent.com/tinysearch/tinysearch/master/fixtures/index.json
# Create the WASM output
docker run -v $PWD:/app tinysearch/cli -m wasm -p /app/wasm_output /app/index.json
```

By default, the most recent stable Alpine Rust image is used. To get nightly,
run

```sh
docker build --build-arg RUST_IMAGE=rustlang/rust:nightly-alpine -t tinysearch/cli:nightly .
```

### Advanced Docker Build Args

- `WASM_REPO`: Overwrite the wasm-pack repository
- `WASM_BRANCH`: Overwrite the repository branch to use
- `TINY_REPO`: Overwrite repository of tinysearch
- `TINY_BRANCH`: Overwrite tinysearch branch

## Github action

To integrate tinysearch in continuous deployment pipelines, a
[github action](https://github.com/marketplace/actions/tinysearch-action) is
available.

```yaml
- name: Build tinysearch
  uses: leonhfr/tinysearch-action@v1
  with:
    index: public/index.json
    output_dir: public/wasm
    output_types: |
      wasm
```

## Users

The following websites use tinysearch:

- [Matthias Endler's blog](https://endler.dev/2019/tinysearch/)
- [OutOfCheeseError](https://out-of-cheese-error.netlify.app/)
- [Museum of Warsaw Archdiocese](https://maw.art.pl/cyfrowemaw/)

Are you using tinysearch, too? Add your site here!

## Maintainers

- Matthias Endler (@mre)
- Jorge-Luis Betancourt (@jorgelbg)
- Mad Mike (@fluential)

## License

tinysearch is licensed under either of

- Apache License, Version 2.0, (LICENSE-APACHE or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license (LICENSE-MIT or http://opensource.org/licenses/MIT)

at your option.

[wasm-pack]: https://github.com/rustwasm/wasm-pack
