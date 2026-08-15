const DEFAULT_WASM_URL = new URL('./{WASM_FILE}', import.meta.url);
const DEFAULT_ROOT_URL = new URL('./{ROOT_FILE}', import.meta.url);
const EXPECTED_ABI_VERSION = 1;
const WASM_PAGE_BYTES = 64 * 1024;

function asUrl(value, base = import.meta.url) {
    return value instanceof URL ? new URL(value.href) : new URL(String(value), base);
}

function asDirectoryUrl(value, base) {
    const url = asUrl(value, base);
    if (!url.pathname.endsWith('/')) {
        url.pathname += '/';
    }
    url.search = '';
    url.hash = '';
    return url;
}

function statusDescription(response) {
    const text = response.statusText ? ` ${response.statusText}` : '';
    return `${response.status}${text}`;
}

async function checkedFetch(fetchFn, url, kind) {
    let response;
    try {
        response = await fetchFn(url);
    } catch (error) {
        throw new TinySearchError(`Failed to fetch ${kind} ${url.href}: ${error.message}`, {
            cause: error,
            url: url.href,
        });
    }
    if (!response || !response.ok) {
        const status = response ? statusDescription(response) : 'no response';
        throw new TinySearchError(`Failed to fetch ${kind} ${url.href}: HTTP ${status}`, {
            url: url.href,
            status: response?.status,
        });
    }
    return response;
}

async function instantiateEngine(fetchFn, wasmUrl) {
    const response = await checkedFetch(fetchFn, wasmUrl, 'WASM module');
    let streamingError;

    if (typeof WebAssembly.instantiateStreaming === 'function') {
        try {
            const result = await WebAssembly.instantiateStreaming(response.clone(), {});
            return result.instance ?? result;
        } catch (error) {
            streamingError = error;
        }
    }

    try {
        const bytes = await response.arrayBuffer();
        const result = await WebAssembly.instantiate(bytes, {});
        return result.instance ?? result;
    } catch (error) {
        const streamingDetail = streamingError
            ? ` Streaming instantiation failed: ${streamingError.message}.`
            : '';
        throw new TinySearchError(
            `Failed to instantiate WASM module ${wasmUrl.href}.${streamingDetail} ` +
                `ArrayBuffer fallback failed: ${error.message}`,
            { cause: error, url: wasmUrl.href },
        );
    }
}

function requireEngineExports(instance) {
    const required = [
        'memory',
        'engine_abi_version',
        'engine_alloc',
        'engine_dealloc',
        'engine_load_root',
        'engine_plan_query',
        'engine_load_shard',
        'engine_search',
        'engine_result_ptr',
        'engine_result_len',
        'engine_result_free',
    ];
    const missing = required.filter((name) => instance.exports[name] === undefined);
    if (missing.length > 0) {
        throw new TinySearchError(`WASM module is missing required exports: ${missing.join(', ')}`);
    }

    const version = instance.exports.engine_abi_version() >>> 0;
    if (version !== EXPECTED_ABI_VERSION) {
        throw new TinySearchError(
            `Unsupported tinysearch engine ABI ${version}; expected ${EXPECTED_ABI_VERSION}`,
        );
    }
}

/** Error raised for network, ABI, and structured engine failures. */
export class TinySearchError extends Error {
    constructor(message, details = {}) {
        super(message, details.cause ? { cause: details.cause } : undefined);
        this.name = 'TinySearchError';
        Object.assign(this, details);
    }
}

/** Async loader and client for a raw-WASM sharded tinysearch engine. */
export class TinySearch {
    constructor(instance, options) {
        requireEngineExports(instance);
        this.instance = instance;
        this.exports = instance.exports;
        this.memory = instance.exports.memory;
        this.wasmUrl = options.wasmUrl;
        this.rootUrl = options.rootUrl;
        this.shardBaseUrl = options.shardBaseUrl;
        this._fetch = options.fetch;
        this._encoder = new TextEncoder();
        this._decoder = new TextDecoder('utf-8', { fatal: true });
        this._loadedUrls = new Set();
        this._shardPromises = new Map();
        this._requestCounts = options.requestCounts;
        this._engineStats = {
            shardCount: 0,
            loadedShardCount: 0,
            loadedShardBytes: 0,
        };
    }

    /** URLs of lexical shards retained by the WASM engine. */
    get loadedUrls() {
        return [...this._loadedUrls];
    }

    /**
     * A snapshot of loader, network, and WASM-memory statistics.
     * `loadedShardBytes` is the sum of loaded artifacts' encoded lengths;
     * `wasmMemoryBytes` reports the current linear-memory allocation.
     */
    get stats() {
        return {
            ...this._engineStats,
            loadedUrls: this.loadedUrls,
            inFlightShardCount: this._shardPromises.size,
            wasmMemoryBytes: this.memory.buffer.byteLength,
            wasmMemoryPages: this.memory.buffer.byteLength / WASM_PAGE_BYTES,
            requests: Object.fromEntries(this._requestCounts),
        };
    }

    _updateStats(response) {
        for (const key of ['shardCount', 'loadedShardCount', 'loadedShardBytes']) {
            if (Number.isSafeInteger(response[key])) {
                this._engineStats[key] = response[key];
            }
        }
    }

    _readResponse(handle, operation) {
        const resultHandle = handle >>> 0;
        if (resultHandle === 0) {
            throw new TinySearchError(`Engine ${operation} failed to allocate a response handle`);
        }

        try {
            const ptr = this.exports.engine_result_ptr(resultHandle) >>> 0;
            const len = this.exports.engine_result_len(resultHandle) >>> 0;
            if (ptr === 0 || len === 0 || ptr + len > this.memory.buffer.byteLength) {
                throw new TinySearchError(
                    `Engine ${operation} returned an invalid response region ` +
                        `(handle=${resultHandle}, ptr=${ptr}, len=${len})`,
                );
            }

            // The result allocation may have grown memory, so construct the view only now.
            const bytes = new Uint8Array(this.memory.buffer, ptr, len).slice();
            let response;
            try {
                response = JSON.parse(this._decoder.decode(bytes));
            } catch (error) {
                throw new TinySearchError(`Engine ${operation} returned invalid UTF-8 JSON: ${error.message}`, {
                    cause: error,
                });
            }
            if (!response || typeof response !== 'object' || typeof response.ok !== 'boolean') {
                throw new TinySearchError(`Engine ${operation} returned a malformed response object`);
            }
            this._updateStats(response);
            if (!response.ok) {
                const needs = Array.isArray(response.needs)
                    ? ` (needs: ${response.needs.map((shard) => shard.filename ?? shard.id).join(', ')})`
                    : '';
                throw new TinySearchError(
                    `Engine ${operation} failed${response.code ? ` [${response.code}]` : ''}: ` +
                        `${response.error ?? 'unknown engine error'}${needs}`,
                    { engine: response },
                );
            }
            return response;
        } finally {
            this.exports.engine_result_free(resultHandle);
        }
    }

    _invokeBytes(operation, bytes, ...args) {
        const input = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
        const len = input.byteLength;
        const ptr = len === 0 ? 0 : this.exports.engine_alloc(len) >>> 0;
        if (len !== 0 && ptr === 0) {
            throw new TinySearchError(`Engine ${operation} could not allocate ${len} input bytes`);
        }

        try {
            // engine_alloc may grow memory, so never retain a view across allocation.
            if (len !== 0) {
                new Uint8Array(this.memory.buffer, ptr, len).set(input);
            }
            const handle = this.exports[operation](ptr, len, ...args);
            return this._readResponse(handle, operation);
        } finally {
            if (len !== 0) {
                this.exports.engine_dealloc(ptr, len);
            }
        }
    }

    _invokeQuery(operation, query, ...args) {
        return this._invokeBytes(operation, this._encoder.encode(String(query)), ...args);
    }

    _descriptorWithUrl(descriptor) {
        if (!descriptor || !Number.isSafeInteger(descriptor.id) || typeof descriptor.filename !== 'string') {
            throw new TinySearchError('Engine query plan contains a malformed shard descriptor');
        }
        const url = new URL(descriptor.filename, this.shardBaseUrl);
        return { id: descriptor.id, filename: descriptor.filename, url: url.href };
    }

    /** Returns the shard plan for `query` without performing network requests. */
    plan(query) {
        const response = this._invokeQuery('engine_plan_query', query);
        if (!Array.isArray(response.required)) {
            throw new TinySearchError('Engine query plan is missing its required shard list');
        }
        return {
            ...response,
            required: response.required.map((descriptor) => this._descriptorWithUrl(descriptor)),
        };
    }

    async _fetchBytes(url, kind) {
        const response = await checkedFetch(this._fetch, url, kind);
        return new Uint8Array(await response.arrayBuffer());
    }

    async _ensureShard(descriptor) {
        const url = asUrl(descriptor.url);
        const key = url.href;
        if (this._loadedUrls.has(key)) {
            return;
        }

        const existing = this._shardPromises.get(key);
        if (existing) {
            return existing;
        }

        const promise = (async () => {
            const bytes = await this._fetchBytes(url, `shard ${descriptor.id}`);
            let response;
            try {
                response = this._invokeBytes('engine_load_shard', bytes);
            } catch (error) {
                throw new TinySearchError(
                    `Failed to load shard ${descriptor.id} from ${key}: ${error.message}`,
                    { cause: error, url: key, engine: error.engine },
                );
            }
            if (response.id !== descriptor.id) {
                throw new TinySearchError(
                    `Shard ${key} loaded as engine ID ${response.id}, expected ${descriptor.id}`,
                    { url: key },
                );
            }
            this._loadedUrls.add(key);
        })();

        this._shardPromises.set(key, promise);
        try {
            await promise;
        } finally {
            // Failures must not become sticky; successful shards are covered by loadedUrls.
            if (this._shardPromises.get(key) === promise) {
                this._shardPromises.delete(key);
            }
        }
    }

    /** Fetches and loads the shards needed by `query` without running a search. */
    async preload(query) {
        const plan = this.plan(query);
        await Promise.all(plan.required.map((descriptor) => this._ensureShard(descriptor)));
        return {
            ...plan,
            loadedUrls: this.loadedUrls,
        };
    }

    /** Lazily loads required shards, then returns stable `{title,url,meta}` results. */
    async search(query, limit = 5) {
        if (!Number.isInteger(limit) || limit < 0 || limit > 0xffff_ffff) {
            throw new TypeError('search limit must be an integer between 0 and 4294967295');
        }
        await this.preload(query);
        const response = this._invokeQuery('engine_search', query, limit);
        if (!Array.isArray(response.results)) {
            throw new TinySearchError('Engine search response is missing its results array');
        }
        return response.results.map((result) => ({
            title: result.title,
            url: result.url,
            meta: result.meta,
        }));
    }

    /**
     * Attaches debounced, stale-safe async typeahead behavior to an input-like
     * EventTarget. Planning and fetching begin only after `debounceMs` (60ms by
     * default), and `render` is called only for the newest input generation.
     * Requests that were already started are not canceled because they may be
     * shared with another query; their completion is cached but never rendered
     * when a newer input generation exists.
     */
    attachTypeahead(input, render, options = {}) {
        if (!input || typeof input.addEventListener !== 'function') {
            throw new TypeError('attachTypeahead requires an input-like EventTarget');
        }
        if (typeof render !== 'function') {
            throw new TypeError('attachTypeahead requires a render function');
        }

        const limit = options.limit ?? 5;
        const debounceMs = options.debounceMs ?? 60;
        if (!Number.isFinite(debounceMs) || debounceMs < 0) {
            throw new TypeError('typeahead debounceMs must be a non-negative finite number');
        }

        let generation = 0;
        let debounceTimer = null;
        const cancelDebounce = () => {
            if (debounceTimer !== null) {
                clearTimeout(debounceTimer);
                debounceTimer = null;
            }
        };
        const runSearch = async (current, query) => {
            if (current !== generation) {
                return;
            }
            try {
                const results = await this.search(query, limit);
                if (current === generation) {
                    render(results, query);
                    options.onStatus?.('ready', query, results);
                }
            } catch (error) {
                if (current === generation) {
                    options.onError?.(error, query);
                    options.onStatus?.('error', query, error);
                }
            }
        };
        const onInput = () => {
            const current = ++generation;
            cancelDebounce();
            const query = String(input.value ?? '').trim();
            options.onStatus?.(query ? 'searching' : 'empty', query);
            if (!query) {
                render([], query);
                return;
            }

            debounceTimer = setTimeout(() => {
                debounceTimer = null;
                void runSearch(current, query);
            }, debounceMs);
        };

        input.addEventListener('input', onInput);
        return () => {
            generation += 1;
            cancelDebounce();
            input.removeEventListener('input', onInput);
        };
    }
}

/** Fetches, instantiates, and initializes a sharded tinysearch engine. */
export async function initTinysearch(options = {}) {
    const wasmUrl = asUrl(options.wasmUrl ?? DEFAULT_WASM_URL);
    const rootUrl = asUrl(options.rootUrl ?? DEFAULT_ROOT_URL);
    const shardBaseUrl = options.shardBaseUrl === undefined
        ? new URL('.', rootUrl)
        : asDirectoryUrl(options.shardBaseUrl, rootUrl);
    const fetchFn = options.fetch ?? options.fetchFn ?? globalThis.fetch?.bind(globalThis);
    if (typeof fetchFn !== 'function') {
        throw new TinySearchError('No fetch implementation is available; pass options.fetch');
    }

    const requestCounts = new Map();
    const trackedFetch = async (url) => {
        const key = asUrl(url).href;
        requestCounts.set(key, (requestCounts.get(key) ?? 0) + 1);
        return fetchFn(url);
    };

    const instance = await instantiateEngine(trackedFetch, wasmUrl);
    const engine = new TinySearch(instance, {
        wasmUrl,
        rootUrl,
        shardBaseUrl,
        fetch: trackedFetch,
        requestCounts,
    });
    const rootResponse = await checkedFetch(trackedFetch, rootUrl, 'sharded root');
    const rootBytes = new Uint8Array(await rootResponse.arrayBuffer());
    try {
        engine._updateStats(engine._invokeBytes('engine_load_root', rootBytes));
    } catch (error) {
        throw new TinySearchError(`Failed to load sharded root ${rootUrl.href}: ${error.message}`, {
            cause: error,
            url: rootUrl.href,
            engine: error.engine,
        });
    }
    return engine;
}

/** Compatibility alias for the loader's original snake_case initializer. */
export const init_tinysearch = initTinysearch;

export default initTinysearch;
