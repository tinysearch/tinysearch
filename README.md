# tinysearch

[![Build Status](https://travis-ci.org/mre/tinysearch.svg?branch=master)](https://travis-ci.org/mre/tinysearch)

This is a Rust implementation of ["Writing a full-text search engine using Bloom filters"](https://www.stavros.io/posts/bloom-filter-search-engine/).

I'm planning to use this on [my homepage](http://matthias-endler.de/) as a way to search through articles.
The idea is to run all posts on there through tinysearch, which will generate a small WASM library that can be shipped to clients. This way, we get a tiny, fast full-text search written in Rust that also works offline. :blush:

The target are plain-old HTML websites commonly rendered with static site-generators like [Jekyll](https://jekyllrb.com/), [Hugo](https://gohugo.io/), [Gutenberg](https://github.com/Keats/gutenberg), or [Cobalt](https://github.com/cobalt-org/cobalt.rs).


## License

tinysearch is licensed under either of


* Apache License, Version 2.0, (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license (LICENSE-MIT or http://opensource.org/licenses/MIT)

at your option.

