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
        const ptr = this.wasm.exports.malloc ? this.wasm.exports.malloc(bytes.length) : this.allocString(bytes.length);
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

export async function init_tinysearch() {
    try {
        // Try streaming first (preferred)
        const wasmModule = await WebAssembly.instantiateStreaming(fetch('./{WASM_FILE}'));
        return new TinySearchWasm(wasmModule.instance);
    } catch (e) {
        console.warn('Streaming failed, falling back to fetch + instantiate:', e.message);
        // Fallback for servers with wrong MIME type
        const response = await fetch('./{WASM_FILE}');
        const wasmBytes = await response.arrayBuffer();
        const wasmModule = await WebAssembly.instantiate(wasmBytes);
        return new TinySearchWasm(wasmModule.instance);
    }
}

// Backward compatibility
export { TinySearchWasm as TinySearch };