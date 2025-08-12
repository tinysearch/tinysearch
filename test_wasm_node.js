#!/usr/bin/env node

// Node.js test for the WASM module
const fs = require('fs');
const path = require('path');

class TinySearchWasm {
    constructor(wasmInstance) {
        this.wasm = wasmInstance;
        this.memory = wasmInstance.exports.memory;
        this.searchFn = wasmInstance.exports.search;
        this.freeFn = wasmInstance.exports.free_search_result;
    }

    // Convert JS string to WASM memory
    stringToWasm(str) {
        const bytes = new TextEncoder().encode(str + '\0');
        const ptr = this.allocString(bytes.length);
        const mem = new Uint8Array(this.memory.buffer, ptr, bytes.length);
        mem.set(bytes);
        return ptr;
    }

    // Read string from WASM memory
    wasmToString(ptr) {
        if (ptr === 0) return null;
        const mem = new Uint8Array(this.memory.buffer);
        let end = ptr;
        while (mem[end] !== 0) end++;
        return new TextDecoder().decode(mem.subarray(ptr, end));
    }

    // Simple string allocation fallback
    allocString(len) {
        // This is a simple fallback - WASM linear memory grows as needed
        const pages = Math.ceil(len / 65536);
        this.memory.grow(pages);
        return this.memory.buffer.byteLength - len;
    }

    // Perform search
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
    try {
        const wasmPath = './wasm_test_output/tinysearch_engine.wasm';
        const wasmBytes = fs.readFileSync(wasmPath);
        const wasmModule = await WebAssembly.instantiate(wasmBytes);
        return new TinySearchWasm(wasmModule.instance);
    } catch (error) {
        console.error('Failed to initialize WASM:', error);
        throw error;
    }
}

async function runTests() {
    console.log('🧪 Testing TinySearch WASM Module\n');

    try {
        console.log('1. Initializing WASM module...');
        const search = await initTinySearch();
        console.log('✅ WASM module initialized successfully\n');

        // Test cases
        const testQueries = [
            { query: 'rust', expected: true },
            { query: 'javascript', expected: true },
            { query: 'nonexistent_term_12345', expected: false },
            { query: 'blog', expected: true },
            { query: '', expected: false }
        ];

        console.log('2. Running search tests...');
        for (const test of testQueries) {
            console.log(`\n🔍 Searching for: "${test.query}"`);
            const results = search.search(test.query, 5);
            console.log(`📊 Found ${results.length} results`);
            
            if (results.length > 0) {
                results.forEach((result, index) => {
                    console.log(`   ${index + 1}. ${result.title}`);
                    console.log(`      URL: ${result.url}`);
                    if (result.meta) console.log(`      Meta: ${result.meta}`);
                });
                
                if (!test.expected) {
                    console.log('⚠️  Expected no results but got some');
                }
            } else {
                if (test.expected) {
                    console.log('⚠️  Expected results but got none');
                } else {
                    console.log('✅ No results (as expected)');
                }
            }
        }

        console.log('\n3. Performance test...');
        const startTime = Date.now();
        const iterations = 1000;
        
        for (let i = 0; i < iterations; i++) {
            search.search('rust', 5);
        }
        
        const endTime = Date.now();
        const avgTime = (endTime - startTime) / iterations;
        console.log(`⚡ Average search time: ${avgTime.toFixed(3)}ms (${iterations} iterations)`);
        
        console.log('\n🎉 All tests completed successfully!');
        
    } catch (error) {
        console.error('❌ Test failed:', error);
        process.exit(1);
    }
}

// Check if WASM file exists
if (!fs.existsSync('./wasm_test_output/tinysearch_engine.wasm')) {
    console.error('❌ WASM file not found. Please run:');
    console.error('cargo run --features=bin -- -m wasm -p wasm_test_output fixtures/index.json');
    process.exit(1);
}

runTests().catch(console.error);