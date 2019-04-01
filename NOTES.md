Your Browser Now Runs Rust Code - And You Didn't Even Notice
How I Shipped Some Rust Code To Your Browser - And You Didn't Even Notice
- Writing A Tiny Offline Search Engine Using WASM

I'm a big proponent of performance and privacy.
Therefore my website should be as lean as possible.
The homepage does not contain any images.

The challenges:

* Tiny - want to ship together with the rest of the page
* Cross-browser - Firefox, Chrome, Safari, you name it
* Offline support



https://stackoverflow.com/questions/867099/bloom-filter-or-cuckoo-hashing
https://www.cs.cmu.edu/~binfan/papers/login_cuckoofilter.pdf

It's not faster, but I'm more familiar with Rust than JavaScript. That's why I did that.

## Bloom filter

Tried https://github.com/jedisct1/rust-bloom-filter
first, but it didn't implement serialize/deserialize


cuckoofilter:
~/C/p/tinysearch ❯❯❯ l storage
Permissions Size User    Date Modified Name
.rw-r--r--   44k mendler 24 Mar 15:42  storage


# Binary size over time

"Vanilla WASM pack" 216316 

https://github.com/johnthagen/min-sized-rust

"opt-level = 'z'" 249665
"lto = true" 202516
"opt-level = 's'" 195950

 trades off size for speed. It has a tiny code-size footprint, but it is is not competitive in terms of performance with the default global allocator, for example.

"wee_alloc and nightly" 187560
"codegen-units = 1" 183294

```
brew install binaryen
```

"wasm-opt -Oz" 154413

"Remove web-sys as we don't have to bind to the DOM." 152858

clean up other dependencies that I added during testing

"remove structopt" 152910


```
twiggy top -n 20 pkg/tinysearch_bg.wasm
 Shallow Bytes │ Shallow % │ Item
───────────────┼───────────┼─────────────────────────────────────────────────────────────────────────────────────────────────────
         79256 ┊    44.37% ┊ data[0]
         13886 ┊     7.77% ┊ "function names" subsection
          7289 ┊     4.08% ┊ data[1]
          6888 ┊     3.86% ┊ core::fmt::float::float_to_decimal_common_shortest::hdd201d50dffd0509
          6080 ┊     3.40% ┊ core::fmt::float::float_to_decimal_common_exact::hcb5f56a54ebe7361
          5972 ┊     3.34% ┊ std::sync::once::Once::call_once::{{closure}}::ha520deb2caa7e231
          5869 ┊     3.29% ┊ search
```


## Analyzing the dehydration part

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BincodeError> {
        let decoded = bincode::deserialize(bytes)?;
        Ok(Storage {
            filters: Storage::hydrate(decoded),
        })
    }


    results in

twiggy top -n 10 dbg/tinysearch_bg.wasm
 Shallow Bytes │ Shallow % │ Item
───────────────┼───────────┼───────────────────────────────────────────────────────────────────────────────────────────────
         36040 ┊    25.62% ┊ data[0]
         14038 ┊     9.98% ┊ "function names" subsection
         10116 ┊     7.19% ┊ std::sync::once::Once::call_once::{{closure}}::h58fa0daaf41a010a
          7313 ┊     5.20% ┊ data[1]
          6888 ┊     4.90% ┊ core::fmt::float::float_to_decimal_common_shortest::hdd201d50dffd0509
          6226 ┊     4.43% ┊ search
          6080 ┊     4.32% ┊ core::fmt::float::float_to_decimal_common_exact::hcb5f56a54ebe7361
          4879 ┊     3.47% ┊ core::num::flt2dec::strategy::dragon::mul_pow10::h1f6e32d33228d12a
          2734 ┊     1.94% ┊ <serde_json::error::Error as serde::ser::Error>::custom::ha35c72a3e1216b8f
          1722 ┊     1.22% ┊ <std::path::Components<'a> as core::iter::traits::iterator::Iterator>::next::hdc7c6ef507797acc
         44531 ┊    31.66% ┊ ... and 464 more.
        140567 ┊    99.93% ┊ Σ [474 Total Rows]


    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BincodeError> {
        let decoded = bincode::deserialize(bytes)?;
        Ok(Storage {
            filters: Filters::new()
        })
    }

    results in

    twiggy top -n 10 dbg/tinysearch_bg.wasm
 Shallow Bytes │ Shallow % │ Item
───────────────┼───────────┼──────────────────────────────────────────────────────────────────────────────────────
         30839 ┊    40.79% ┊ data[0]
          7108 ┊     9.40% ┊ "function names" subsection
          6282 ┊     8.31% ┊ search
          5689 ┊     7.52% ┊ data[1]
          2727 ┊     3.61% ┊ <serde_json::error::Error as serde::ser::Error>::custom::ha35c72a3e1216b8f
          1437 ┊     1.90% ┊ std::sync::once::Once::call_inner::h35f0eda9cf9eca08
          1428 ┊     1.89% ┊ <std::sys_common::poison::PoisonError<T> as core::fmt::Debug>::fmt::h3c1beed6d984aee3
          1217 ┊     1.61% ┊ data[2]
          1182 ┊     1.56% ┊ core::fmt::write::hd4bdd4af2be576da
          1109 ┊     1.47% ┊ core::str::slice_error_fail::ha73ff2fecc9e819b
         16497 ┊    21.82% ┊ ... and 248 more.
         75515 ┊    99.88% ┊ Σ [258 Total Rows]






didn't help:
wasm-snip --snip-rust-fmt-code --snip-rust-panicking-code -o pkg/tinysearch_bg_snip.wasm pkg/tinysearch_bg_opt.wasm


# Tips

* This is still the wild west: unstable features, nightly rust, documentation gets outdated almost every day. I love it!
* Rust is very good with removing dead code, so you usually don't pay for unused crates. 
  I would still advise you to be very conservative about the dependencies you add, because it's tempting to add features which you don't need and which will add to the binary size.
  For example, I used Structopt during testing and I had a main function which was parsing these commandline arguments. This was not necessary for WASM, however. So I removed it later.


I understand that not everyone wants to write Rust code. It's complicated to get started with.
The cool thing is, that you can do the same with almost any other language, too. For example, you can write Go code and transpile to WASM or maybe you prefer
PHP or Haskell.

https://github.com/appcypher/awesome-wasm-langs



The message is: the web is for EVERYONE, not just JavaScript programmers.
What was very hard just a two years ago is easy now: shipping code in any language to every browser.
We should make better use of it.


Now the challenge becomes optimizing the index datastructure

* cuckoofilter, all posts 45221 bytes

[GRAPH SHOWING SPACE USAGE WITH DIFFERENT DATASTRUCTURES]

## Lessons learned

A lot of people dismiss WebAssembly as a toy technology.
This cannot be further from the truth.
WebAssembly will revolutionize the way we build web technology.
Whenever there is a new technology, ask yourself: "What changed? What is possible now?".

## Future steps

Stop words
https://gist.github.com/sebleier/554280

Index word tree

    w (maybe skip that one as every article will have every letter of the alphabet)
    wo
    wor
    word

Find more gems at
https://journal.valeriansaliou.name/announcing-sonic-a-super-light-alternative-to-elasticsearch/

# References

https://rustwasm.github.io/docs/book/reference/code-size.html