# tinysearch

![CI](https://github.com/mre/tinysearch/workflows/CI/badge.svg)

tinysearch is a lightweight, fast, full-text search engine. It is designed for static websites.

tinysearch is written in Rust, and then compiled to WebAssembly to run in a browser.  
It can be used together with static site generators such as [Jekyll](https://jekyllrb.com/),
[Hugo](https://gohugo.io/), [Zola](https://www.getzola.org/),
[Cobalt](https://github.com/cobalt-org/cobalt.rs), or [Pelican](https://getpelican.com).

![Demo](tinysearch.gif)

## Is it tiny?

The test index file of my blog with around 40 posts creates a WASM payload of 99kB (49kB gzipped, 40kB brotli).  
That is smaller than the demo image above; so yes.

## How it works

tinysearch is a Rust/WASM port of the Python code from the article ["Writing a full-text
search engine using Bloom filters"](https://www.stavros.io/posts/bloom-filter-search-engine/).
It can be seen as an alternative to [lunr.js](https://lunrjs.com/) and
[elasticlunr](http://elasticlunr.com/), which are too heavy for smaller websites and
load a lot of JavaScript.  

Under the hood it uses a [Xor Filter](https://arxiv.org/abs/1912.08258) &mdash; a
datastructure for fast approximation of set membership that is smaller than
bloom and cuckoo filters.  Each blog post gets converted into a filter that will
then be serialized to a binary blob using
[bincode](https://github.com/bincode-org/bincode).  Please note that the
underlying technologies are subject to change.

## Limitations

- Only finds entire words. As a consequence there are no search
  suggestions (yet).  This is a necessary tradeoff for reducing memory usage. A
  trie datastructure was about 10x bigger than the xor filters.  New research on
  compact datastructures for prefix searches might lift this limitation in the
  future.
- Since we bundle all search indices for all articles into one static binary, we
  recommend to only use it for small- to medium-size websites. Expect around 2 kB
  uncompressed per article (~1 kb compressed).

## Installation

[wasm-pack](https://rustwasm.github.io/wasm-pack/) is required to build the WASM
module. Install it with

```sh
cargo install wasm-pack
```

To optimize the JavaScript output, you'll also need
[terser](https://github.com/terser/terser):

```
npm install terser -g
```

If you want to make the WebAssembly as small as possible, we recommend to
install [binaryen](https://github.com/WebAssembly/binaryen) as well. On macOS
you can install it with [homebrew](https://brew.sh/):

```sh
brew install binaryen
```

Alternatively, you can download the binary from the [release
page](https://github.com/WebAssembly/binaryen/releases) or use your OS package
manager.

After that, you can install tinysearch itself:

```
cargo install tinysearch
```

## Usage

A JSON file, which contains the content to index, is required as an input.
Please take a look at the [example file](fixtures/index.json).

Once you created the index, you can run

```
tinysearch fixtures/index.json
```

ℹ️ You can take a look at the code examples for different static site generators [here](https://github.com/mre/tinysearch/tree/master/howto).  
ℹ️ The `body` field in the JSON document is optional and can be skipped to just index post titles.

This will create a WASM module and the JavaScript glue code to integrate it into
your website. You can open the `demo.html` from any webserver to see the
result.

For example, Python has a built-in webserver that can be used for a quick test:

```
python3 -m http.server 
```

then browse to http://0.0.0.0:8000/demo.html to run the demo.

## Advanced Usage

For advanced usage options, run

```
tinysearch --help
```

Please check what's required to [host WebAssembly in production](https://rustwasm.github.io/book/reference/deploying-to-production.html) -- you will need to explicitly set gzip mime types.

## Docker

If you don't have a full Rust setup available, you can also use our nightly-built Docker images.

Here is how to quickly try tinysearch with Docker:

```sh
# Download a sample blog index from endler.dev
curl -O https://raw.githubusercontent.com/tinysearch/tinysearch/master/fixtures/index.json
# Create the WASM output
docker run -v $PWD:/tmp tinysearch/cli index.json
```

By default, the most recent stable Alpine Rust image is used. To get nightly, run

```sh
docker build --build-arg RUST_IMAGE=rustlang/rust:nightly-alpine -t tinysearch/cli:nightly .
```

### Advanced Docker Build Args

 - `WASM_REPO`: Overwrite the wasm-pack repository
 - `WASM_BRANCH`: Overwrite the repository branch to use
 - `TINY_REPO`: Overwrite repository of tinysearch
 - `TINY_BRANCH`: Overwrite tinysearch branch

## Github action

To integrate tinysearch in continuous deployment pipelines, a [github action](https://github.com/marketplace/actions/tinysearch-action) is available.

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

* [Matthias Endler's blog](https://endler.dev/2019/tinysearch/)
* [OutOfCheeseError](https://out-of-cheese-error.netlify.app/)
* [Museum of Warsaw Archdiocese](https://maw.art.pl/cyfrowemaw/)

Are you using tinysearch, too? Add your site here!

## Maintainers

* Matthias Endler (@mre)
* Jorge-Luis Betancourt (@jorgelbg)
* Mad Mike (@fluential)

## License

tinysearch is licensed under either of

* Apache License, Version 2.0, (LICENSE-APACHE or
  http://www.apache.org/licenses/LICENSE-2.0)
* MIT license (LICENSE-MIT or http://opensource.org/licenses/MIT)

at your option.


[wasm-pack]: https://github.com/rustwasm/wasm-pack
