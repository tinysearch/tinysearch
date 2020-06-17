# tinysearch

![CI](https://github.com/mre/tinysearch/workflows/CI/badge.svg)

This is a lightweight, fast, full-text search engine for static websites. I'm
using it on [my blog](https://endler.dev/2019/tinysearch/):

![Demo](tinysearch.gif)

It is a Rust/WASM port of the Python code from the article ["Writing a full-text
search engine using Bloom
filters"](https://www.stavros.io/posts/bloom-filter-search-engine/). This can be
seen as an alternative to [lunr.js](https://lunrjs.com/) and
[elasticlunr](http://elasticlunr.com/).

The idea is to generate a small, self-contained WASM module from a list of
articles on your website and ship it to browsers. tinysearch can be integrated
into the build process of generators like [Jekyll](https://jekyllrb.com/),
[Hugo](https://gohugo.io/), [zola](https://www.getzola.org/), or
[Cobalt](https://github.com/cobalt-org/cobalt.rs).

## Limitations

- Only searches for entire words. There are no search suggestions (yet).
- Since we bundle all search indices for all articles into one static binary, we
  recommend to only use it for small- to medium-size websites. Expect around 4kB
  (non-compressed) per article.

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

As an input, we require a JSON file, which contains a the content you like to index.
Check out this [example file](fixtures/index.json)).

```
tinysearch fixtures/index.json
```

(You can take a look at the code examples for different static site generators [here](https://github.com/mre/tinysearch/tree/master/howto).)

This will create a WASM module and the JavaScript glue code to integrate it into
your homepage. You can open the `demo.html` from any webserver to see the
result.

For example, Python has a built-in webserver for testing:

```
python3 -m http.server 
```

then browse to http://0.0.0.0:8000/demo.html to see the result.

For advanced usage options, try

```
tinysearch --help
```

Please check what's required to [host WebAssembly in production](https://rustwasm.github.io/book/reference/deploying-to-production.html) -- you will need to explicitly set mime gzip types
## Docker

### Build
Available buid args:
 - WASM_REPO
 - WASM_BRANCH
 - TINY_REPO
 - TINY_BRANCH
 - TINY_MAGIC (for a magic number see https://github.com/mre/tinysearch/issues/111)

#### Demo
```
wget https://raw.githubusercontent.com/tinysearch/tinysearch/master/fixtures/index.json
docker run $PWD:/tmp tinysearch/cli index.json
```

Custom repo/branch build
```
docker build --build-arg WASM_BRANCH=master --build-arg TINY_MAGIC=64 -t tinysearch/cli .
```

By default most recent stable alpine rust image is used. To get nightly just run
```
docker build --build-arg RUST_IMAGE=rustlang/rust:nightly-alpine -t tinysearch/cli:nightly .
```

## Maintainers

* Matthias Endler (@mre)
* Jorge-Luis Betancourt (@jorgelbg)

## License

tinysearch is licensed under either of

* Apache License, Version 2.0, (LICENSE-APACHE or
  http://www.apache.org/licenses/LICENSE-2.0)
* MIT license (LICENSE-MIT or http://opensource.org/licenses/MIT)

at your option.


[wasm-pack]: https://github.com/rustwasm/wasm-pack
