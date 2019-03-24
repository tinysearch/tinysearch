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

https://rustwasm.github.io/docs/book/reference/code-size.html

# References

https://rustwasm.github.io/docs/book/reference/code-size.html