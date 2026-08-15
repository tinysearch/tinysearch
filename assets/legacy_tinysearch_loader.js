const DEFAULT_WASM_URL = new URL('./{WASM_FILE}', import.meta.url);

class TinySearchWasm {
    constructor(wasmInstance) {
        this.wasm = wasmInstance;
        this.memory = wasmInstance.exports.memory;
        this.searchFn = wasmInstance.exports.search;
        this.freeFn = wasmInstance.exports.free_search_result;
        this.allocQueryFn = wasmInstance.exports.alloc_query;
        this.freeQueryFn = wasmInstance.exports.free_query;
        if (!this.searchFn || !this.freeFn || !this.allocQueryFn || !this.freeQueryFn) {
            throw new Error('WASM module is missing required tinysearch exports');
        }
    }

    stringToWasm(str) {
        const bytes = new TextEncoder().encode(str + '\0');
        const ptr = this.allocQueryFn(bytes.length);
        if (ptr === 0) {
            throw new Error(`WASM module could not allocate ${bytes.length} query bytes`);
        }
        const mem = new Uint8Array(this.memory.buffer, ptr, bytes.length);
        mem.set(bytes);
        return { ptr, len: bytes.length };
    }

    wasmToString(ptr) {
        if (ptr === 0) return null;
        const mem = new Uint8Array(this.memory.buffer);
        let end = ptr;
        while (end < mem.length && mem[end] !== 0) end += 1;
        if (end === mem.length) {
            throw new Error('WASM search result is missing its null terminator');
        }
        return new TextDecoder().decode(mem.subarray(ptr, end));
    }


    get stats() {
        return { loadedShardCount: 0 };
    }

    async search(query, numResults = 5) {
        const input = this.stringToWasm(query);
        let resultPtr;
        try {
            resultPtr = this.searchFn(input.ptr, numResults);
        } finally {
            this.freeQueryFn(input.ptr, input.len);
        }
        if (resultPtr === 0) {
            throw new Error('WASM search returned a null result pointer');
        }

        let jsonStr;
        try {
            jsonStr = this.wasmToString(resultPtr);
        } finally {
            this.freeFn(resultPtr);
        }
        try {
            return JSON.parse(jsonStr);
        } catch (error) {
            throw new Error(`WASM search returned invalid JSON: ${error.message}`, { cause: error });
        }
    }
}

export async function initTinysearch(options = {}) {
    const wasmUrl = new URL(options.wasmUrl ?? DEFAULT_WASM_URL, import.meta.url);
    const fetchFn = options.fetch ?? globalThis.fetch?.bind(globalThis);
    if (typeof fetchFn !== 'function') {
        throw new Error('No fetch implementation is available; pass options.fetch');
    }

    const response = await fetchFn(wasmUrl);
    if (!response.ok) {
        throw new Error(`Failed to fetch WASM module ${wasmUrl.href}: HTTP ${response.status}`);
    }

    if (typeof WebAssembly.instantiateStreaming === 'function') {
        try {
            const result = await WebAssembly.instantiateStreaming(response.clone(), {});
            return new TinySearchWasm(result.instance ?? result);
        } catch {
            // Fall back when the server does not use application/wasm.
        }
    }

    const result = await WebAssembly.instantiate(await response.arrayBuffer(), {});
    return new TinySearchWasm(result.instance ?? result);
}

export const init_tinysearch = initTinysearch;
export { TinySearchWasm as TinySearch };
