# TinySearch WASM Demo

This directory contains demo applications showing how to use TinySearch compiled to WebAssembly.

## Prerequisites

First, generate the WASM files:

```bash
# From the project root
make example
```

This will create `wasm_output/` directory with:
- `tinysearch_engine.wasm` - The compiled WebAssembly module
- `tinysearch_engine.js` - JavaScript loader (optional)

## Browser Demo

### Quick Start

```bash
# From the demo directory
cd demo
python3 -m http.server 8000
```

Open http://localhost:8000/browser.html in your browser.

### Features

- Real-time search with performance metrics
- Clean, responsive interface
- Enter key and click support
- Sample queries: "rust", "javascript", "blog"

### Integration

To integrate into your website:

```javascript
// Load WASM module
const response = await fetch('./path/to/tinysearch_engine.wasm');
const wasmBytes = await response.arrayBuffer();
const wasmModule = await WebAssembly.instantiate(wasmBytes);

// Create search wrapper
const search = {
    // ... (see browser.html for full implementation)
    
    search(query, maxResults = 5) {
        // Returns array of {title, url, meta} objects
    }
};

// Perform search
const results = search.search("your query", 5);
```

## Node.js Demo

### Quick Start

```bash
# From the demo directory
node node.js
```

### Output

```
TinySearch WASM Node.js Demo
============================

Initializing WASM module...
WASM module loaded successfully

Searching for: "rust"
Found 5 results in 1ms
  1. The Future of Rust
     https://endler.dev/2017/future-of-rust/
  ...

Performance Test
----------------
Average search time: 0.078ms (1000 iterations)
```

### Integration

```javascript
const TinySearchWasm = require('./tinysearch-wrapper');

async function setupSearch() {
    const search = await TinySearchWasm.init('./tinysearch_engine.wasm');
    const results = search.search('query', 5);
    return results;
}
```

## Performance

- **WASM File Size**: ~141KB (116KB optimized)
- **Search Speed**: ~0.1ms per query
- **Memory Usage**: Minimal, suitable for constrained environments
- **Dependencies**: None (vanilla WebAssembly)

## Build Process

TinySearch uses vanilla `cargo build` instead of `wasm-pack`:

```bash
# Generate WASM from search index
cargo run --features=bin -- -m wasm -p wasm_output fixtures/index.json

# With optimization (requires wasm-opt)
cargo run --features=bin -- -m wasm -p wasm_output -o fixtures/index.json
```

This creates a dependency-free, lightweight WebAssembly module perfect for static websites.