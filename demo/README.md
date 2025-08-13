# TinySearch WASM Demo

This directory contains a browser demo showing how to use TinySearch compiled to WebAssembly.

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
const response = await fetch('../wasm_output/tinysearch_engine.wasm');
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