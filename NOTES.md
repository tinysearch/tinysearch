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

Manually serializing using the [from_existing](https://docs.rs/bloomfilter/0.0.12/bloomfilter/struct.Bloom.html#method.from_existing) method also didn't work, as it needed the `sip_keys`, which are only known at 


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


## Future steps

Stop words
https://gist.github.com/sebleier/554280

Index word tree

    w (maybe skip that one as every article will have every letter of the alphabet)
    wo
    wor
    word

Remove the hyphen between words

Find more gems at
https://journal.valeriansaliou.name/announcing-sonic-a-super-light-alternative-to-elasticsearch/

# References

https://rustwasm.github.io/docs/book/reference/code-size.html