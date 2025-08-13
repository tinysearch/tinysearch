#!/usr/bin/env node

// Node.js demo for TinySearch WASM module
const fs = require('fs');
const path = require('path');

class TinySearchWasm {
    constructor(wasmInstance) {
        this.wasm = wasmInstance;
        this.memory = wasmInstance.exports.memory;
        this.searchFn = wasmInstance.exports.search;
        this.freeFn = wasmInstance.exports.free_search_result;
    }

    stringToWasm(str) {
        const bytes = new TextEncoder().encode(str + '\0');
        const ptr = this.allocString(bytes.length);
        const mem = new Uint8Array(this.memory.buffer, ptr, bytes.length);
        mem.set(bytes);
        return ptr;
    }

    wasmToString(ptr) {
        if (ptr === 0) return null;
        const mem = new Uint8Array(this.memory.buffer);
        let end = ptr;
        while (mem[end] !== 0) end++;
        return new TextDecoder().decode(mem.subarray(ptr, end));
    }

    allocString(len) {
        const pages = Math.ceil(len / 65536);
        this.memory.grow(pages);
        return this.memory.buffer.byteLength - len;
    }

    search(query, numResults = 5) {
        const queryPtr = this.stringToWasm(query);
        const resultPtr = this.searchFn(queryPtr, numResults);
        
        if (resultPtr === 0) {
            return [];
        }

        const jsonStr = this.wasmToString(resultPtr);
        this.freeFn(resultPtr);
        
        try {
            return JSON.parse(jsonStr);
        } catch (e) {
            console.error('Failed to parse search results:', e);
            return [];
        }
    }
}

async function initTinySearch() {
    const wasmPath = '../wasm_output/tinysearch_engine.wasm';
    
    if (!fs.existsSync(wasmPath)) {
        throw new Error(`WASM file not found at ${wasmPath}. Please run: make example`);
    }
    
    const wasmBytes = fs.readFileSync(wasmPath);
    const wasmModule = await WebAssembly.instantiate(wasmBytes);
    return new TinySearchWasm(wasmModule.instance);
}

async function runDemo() {
    console.log('TinySearch WASM Node.js Demo');
    console.log('============================\n');

    try {
        console.log('Initializing WASM module...');
        const search = await initTinySearch();
        console.log('WASM module loaded successfully\n');

        // Demo queries
        const queries = ['rust', 'javascript', 'blog', 'nonexistent'];

        for (const query of queries) {
            console.log(`Searching for: "${query}"`);
            const startTime = Date.now();
            const results = search.search(query, 3);
            const endTime = Date.now();
            
            console.log(`Found ${results.length} results in ${endTime - startTime}ms`);
            
            if (results.length > 0) {
                results.forEach((result, i) => {
                    console.log(`  ${i + 1}. ${result.title}`);
                    console.log(`     ${result.url}`);
                });
            } else {
                console.log('  No results found');
            }
            console.log('');
        }

        // Performance test
        console.log('Performance Test');
        console.log('----------------');
        const iterations = 1000;
        const startTime = Date.now();
        
        for (let i = 0; i < iterations; i++) {
            search.search('rust', 5);
        }
        
        const endTime = Date.now();
        const avgTime = (endTime - startTime) / iterations;
        console.log(`Average search time: ${avgTime.toFixed(3)}ms (${iterations} iterations)`);
        
    } catch (error) {
        console.error('Demo failed:', error.message);
        console.error('\nTo generate WASM files, run:');
        console.error('  make example');
        process.exit(1);
    }
}

runDemo().catch(console.error);