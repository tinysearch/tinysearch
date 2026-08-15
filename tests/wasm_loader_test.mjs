import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import test from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';

const outputArgument = process.argv[2];
if (!outputArgument) {
    throw new Error('Usage: node tests/wasm_loader_test.mjs <generated-output-directory>');
}
if (typeof Response !== 'function') {
    throw new Error('The WASM loader test requires Node.js 18 or newer (global Response is missing)');
}

const outputDirectory = path.resolve(outputArgument);
const ROOT_MAGIC = Buffer.from('tinysearch-sharded-root');
const SHARD_MAGIC = Buffer.from('tinysearch-shard');
const PAGE_BYTES = 64 * 1024;

function sleep(milliseconds) {
    return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function waitFor(predicate, timeoutMs = 3_000) {
    const deadline = Date.now() + timeoutMs;
    while (!predicate()) {
        if (Date.now() >= deadline) {
            throw new Error(`condition was not met within ${timeoutMs}ms`);
        }
        await sleep(5);
    }
}

async function walk(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    const nested = await Promise.all(entries.map(async (entry) => {
        const filename = path.join(directory, entry.name);
        return entry.isDirectory() ? walk(filename) : [filename];
    }));
    return nested.flat();
}

function startsWith(buffer, prefix) {
    return buffer.length >= prefix.length && buffer.subarray(0, prefix.length).equals(prefix);
}

async function discoverArtifacts() {
    const files = await walk(outputDirectory);
    const wasmFiles = files.filter((filename) => filename.endsWith('.wasm'));
    assert.equal(wasmFiles.length, 1, `expected one .wasm file, found: ${wasmFiles.join(', ')}`);

    const javascriptFiles = files.filter((filename) => filename.endsWith('.js'));
    const loaderMatches = [];
    for (const filename of javascriptFiles) {
        const source = await readFile(filename, 'utf8');
        if (source.includes('initTinysearch') && source.includes('engine_plan_query')) {
            loaderMatches.push(filename);
        }
    }
    assert.equal(
        loaderMatches.length,
        1,
        `expected one generated tinysearch loader, found: ${loaderMatches.join(', ')}`,
    );

    const roots = [];
    const shards = [];
    for (const filename of files) {
        if (filename.endsWith('.wasm') || filename.endsWith('.js') || filename.endsWith('.html')) {
            continue;
        }
        const bytes = await readFile(filename);
        if (startsWith(bytes, ROOT_MAGIC)) {
            roots.push(filename);
        } else if (startsWith(bytes, SHARD_MAGIC)) {
            shards.push(filename);
        }
    }
    assert.equal(roots.length, 1, `expected one sharded root artifact, found: ${roots.join(', ')}`);
    assert.ok(shards.length > 0, 'expected at least one lexical shard artifact');

    const shardDirectories = new Set(shards.map((filename) => path.dirname(filename)));
    assert.equal(
        shardDirectories.size,
        1,
        `loader test expects descriptor filenames in one shard directory, found: ${[...shardDirectories].join(', ')}`,
    );

    return {
        wasm: wasmFiles[0],
        loader: loaderMatches[0],
        root: roots[0],
        shards,
        shardDirectory: [...shardDirectories][0],
    };
}

function createFileFetch({ wasmContentType = 'application/wasm', delayMs = 0 } = {}) {
    const counts = new Map();
    let failureUrl = null;
    let failuresRemaining = 0;

    const fetch = async (input) => {
        const url = input instanceof URL
            ? input
            : new URL(typeof input === 'string' ? input : input.url);
        counts.set(url.href, (counts.get(url.href) ?? 0) + 1);
        if (delayMs > 0) {
            await sleep(delayMs);
        }

        if (url.href === failureUrl && failuresRemaining > 0) {
            failuresRemaining -= 1;
            return new Response('injected shard failure', {
                status: 503,
                statusText: 'Injected Failure',
            });
        }
        if (url.protocol !== 'file:') {
            return new Response(`unsupported test URL ${url.href}`, { status: 400 });
        }

        try {
            const bytes = await readFile(fileURLToPath(url));
            const contentType = url.pathname.endsWith('.wasm')
                ? wasmContentType
                : 'application/octet-stream';
            return new Response(bytes, {
                status: 200,
                headers: { 'content-type': contentType },
            });
        } catch (error) {
            return new Response(`${error.code ?? 'read error'}: ${url.href}`, {
                status: error.code === 'ENOENT' ? 404 : 500,
            });
        }
    };

    return {
        fetch,
        counts,
        failOnce(url) {
            failureUrl = String(url);
            failuresRemaining = 1;
        },
    };
}

function countFor(controller, url) {
    return controller.counts.get(String(url)) ?? 0;
}

function assertResultShape(result) {
    assert.deepEqual(Object.keys(result).sort(), ['meta', 'title', 'url']);
    assert.equal(typeof result.title, 'string');
    assert.equal(typeof result.url, 'string');
    assert.equal(typeof result.meta, 'string');
}

async function findLazyQuery(engine) {
    const candidates = [
        'review',
        'plumber',
        'podcast',
        'firefox',
        'calendar',
        'millionaires',
        'programming',
        'sponsors',
    ];
    const plans = candidates.map((query) => ({ query, plan: engine.plan(query) }));
    const match = plans.find(({ plan }) => plan.required.length > 0 && plan.required.length < engine.stats.shardCount);
    assert.ok(
        match,
        `no candidate exercised a strict subset of ${engine.stats.shardCount} shards: ` +
            plans.map(({ query, plan }) => `${query}=${plan.required.length}`).join(', '),
    );
    return match;
}

function optionsFor(artifacts, controller) {
    return {
        wasmUrl: pathToFileURL(artifacts.wasm),
        rootUrl: pathToFileURL(artifacts.root),
        shardBaseUrl: pathToFileURL(`${artifacts.shardDirectory}${path.sep}`),
        fetch: controller.fetch,
    };
}

function rawInputCall(engine, operation, bytes, ...args) {
    const input = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const ptr = input.byteLength === 0 ? 0 : engine.exports.engine_alloc(input.byteLength) >>> 0;
    assert.ok(input.byteLength === 0 || ptr !== 0, `raw ${operation} input allocation failed`);
    try {
        if (input.byteLength > 0) {
            new Uint8Array(engine.memory.buffer, ptr, input.byteLength).set(input);
        }
        return engine.exports[operation](ptr, input.byteLength, ...args) >>> 0;
    } finally {
        if (input.byteLength > 0) {
            engine.exports.engine_dealloc(ptr, input.byteLength);
        }
    }
}

function readRawResponse(engine, handle) {
    const ptr = engine.exports.engine_result_ptr(handle) >>> 0;
    const len = engine.exports.engine_result_len(handle) >>> 0;
    assert.ok(ptr > 0 && len > 0, `raw response handle ${handle} is invalid`);
    try {
        const bytes = new Uint8Array(engine.memory.buffer, ptr, len).slice();
        return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
    } finally {
        engine.exports.engine_result_free(handle);
    }
}

class FakeInput extends EventTarget {
    value = '';
}

test('generated sharded WASM loader is lazy, retryable, deduplicated, and bounded', async (context) => {
    const artifacts = await discoverArtifacts();
    const loaderUrl = pathToFileURL(artifacts.loader);
    loaderUrl.searchParams.set('wasm-loader-test', String(Date.now()));
    const { initTinysearch, init_tinysearch } = await import(loaderUrl.href);
    assert.equal(typeof initTinysearch, 'function', 'generated loader must export initTinysearch');
    assert.equal(typeof init_tinysearch, 'function', 'generated loader must export init_tinysearch');
    assert.equal(init_tinysearch, initTinysearch, 'compatibility initializer must alias initTinysearch');

    // A wrong MIME type forces instantiateStreaming to fail. The loader must
    // fall back to the original response body without issuing a second fetch.
    const mainFetch = createFileFetch({ wasmContentType: 'application/octet-stream' });
    const engine = await initTinysearch(optionsFor(artifacts, mainFetch));
    const wasmUrl = pathToFileURL(artifacts.wasm).href;
    const rootUrl = pathToFileURL(artifacts.root).href;
    assert.equal(countFor(mainFetch, wasmUrl), 1, 'initialization must fetch WASM exactly once');
    assert.equal(countFor(mainFetch, rootUrl), 1, 'initialization must fetch the root exactly once');
    assert.equal(engine.stats.loadedShardCount, 0, 'initialization must not eagerly load shards');
    assert.equal(engine.stats.shardCount, artifacts.shards.length, 'root shard count must match emitted artifacts');

    const handleFetch = createFileFetch();
    const handleEngine = await init_tinysearch(optionsFor(artifacts, handleFetch));
    const staleHandle = rawInputCall(
        handleEngine,
        'engine_plan_query',
        new TextEncoder().encode('review'),
    );
    assert.ok(staleHandle > 0, 'raw plan must return a result handle');
    const reloadHandle = rawInputCall(
        handleEngine,
        'engine_load_root',
        await readFile(artifacts.root),
    );
    assert.ok(
        reloadHandle > staleHandle,
        `root reload reused or rewound result handles: stale=${staleHandle}, reload=${reloadHandle}`,
    );
    assert.equal(handleEngine.exports.engine_result_ptr(staleHandle) >>> 0, 0);
    assert.equal(handleEngine.exports.engine_result_len(staleHandle) >>> 0, 0);
    assert.equal(readRawResponse(handleEngine, reloadHandle).ok, true);

    const { query: lazyQuery, plan: lazyPlan } = await findLazyQuery(engine);
    const lazyResults = await engine.search(lazyQuery, 10);
    assert.ok(lazyResults.length > 0, `known fixture query ${JSON.stringify(lazyQuery)} returned no results`);
    lazyResults.forEach(assertResultShape);
    assert.equal(engine.loadedUrls.length, lazyPlan.required.length);
    assert.ok(
        engine.loadedUrls.length < engine.stats.shardCount,
        `lazy query loaded all ${engine.stats.shardCount} shards instead of a strict subset`,
    );
    for (const descriptor of lazyPlan.required) {
        assert.equal(countFor(mainFetch, descriptor.url), 1, `required shard was not fetched once: ${descriptor.url}`);
    }

    const requestsBeforeRepeat = new Map(mainFetch.counts);
    const repeated = await engine.search(lazyQuery, 10);
    assert.deepEqual(repeated, lazyResults, 'repeated search results must be stable');
    assert.deepEqual(mainFetch.counts, requestsBeforeRepeat, 'repeated search must not refetch loaded shards');

    const unicodeResults = await engine.search(`${lazyQuery} 🦀 東京`, 10);
    assert.ok(Array.isArray(unicodeResults), 'Unicode query must return a results array');
    unicodeResults.forEach(assertResultShape);

    const reviewResults = await engine.search('review', 10);
    assert.ok(
        reviewResults.some((result) => /review code/i.test(result.title)),
        `known review search did not find “How To Review Code”: ${JSON.stringify(reviewResults.slice(0, 3))}`,
    );
    const plumberResults = await engine.search('plumber', 10);
    assert.ok(
        plumberResults.some((result) => /paolo.*plumber/i.test(result.title)),
        `known plumber search did not find “Paolo the Plumber”: ${JSON.stringify(plumberResults.slice(0, 3))}`,
    );

    const concurrentFetch = createFileFetch();
    const concurrentEngine = await initTinysearch(optionsFor(artifacts, concurrentFetch));
    const concurrentPlan = concurrentEngine.plan(lazyQuery);
    assert.ok(concurrentPlan.required.length > 0, 'concurrent dedupe query must require a shard');
    const [concurrentA, concurrentB] = await Promise.all([
        concurrentEngine.search(lazyQuery, 10),
        concurrentEngine.search(lazyQuery, 10),
    ]);
    assert.deepEqual(concurrentA, concurrentB, 'concurrent searches must produce identical results');
    for (const descriptor of concurrentPlan.required) {
        assert.equal(
            countFor(concurrentFetch, descriptor.url),
            1,
            `concurrent queries refetched shard ${descriptor.url}`,
        );
    }

    const typeaheadFetch = createFileFetch({ delayMs: 20 });
    const typeaheadEngine = await initTinysearch(optionsFor(artifacts, typeaheadFetch));
    const supersededPlan = typeaheadEngine.plan('review');
    const newestPlan = typeaheadEngine.plan('plumber');
    assert.ok(supersededPlan.required.length > 0, 'superseded typeahead query must require a shard');
    assert.ok(newestPlan.required.length > 0, 'newest typeahead query must require a shard');

    const originalPlan = typeaheadEngine.plan.bind(typeaheadEngine);
    const plannedQueries = [];
    typeaheadEngine.plan = (query) => {
        plannedQueries.push(String(query));
        return originalPlan(query);
    };
    const fakeInput = new FakeInput();
    const rendered = [];
    const typeaheadErrors = [];
    const detachTypeahead = typeaheadEngine.attachTypeahead(
        fakeInput,
        (items, query) => rendered.push({ items, query }),
        {
            debounceMs: 60,
            onError: (error) => typeaheadErrors.push(error),
        },
    );

    fakeInput.value = 'review';
    fakeInput.dispatchEvent(new Event('input'));
    await sleep(10);
    fakeInput.value = 'plumber';
    fakeInput.dispatchEvent(new Event('input'));
    await waitFor(() => rendered.length > 0 || typeaheadErrors.length > 0);

    assert.deepEqual(typeaheadErrors, [], 'debounced typeahead search should not fail');
    assert.deepEqual(plannedQueries, ['plumber'], 'superseded input must not be planned');
    assert.equal(rendered.length, 1, 'typeahead must render only the newest generation');
    assert.equal(rendered[0].query, 'plumber');
    assert.ok(
        rendered[0].items.some((result) => /paolo.*plumber/i.test(result.title)),
        'newest typeahead result should be rendered',
    );
    const newestShardUrls = new Set(newestPlan.required.map((descriptor) => descriptor.url));
    const fetchedTypeaheadShards = [...typeaheadFetch.counts.keys()]
        .filter((url) => url.endsWith('.tinysearch-shard'));
    assert.ok(fetchedTypeaheadShards.length > 0, 'newest typeahead query should fetch a shard');
    assert.ok(
        fetchedTypeaheadShards.every((url) => newestShardUrls.has(url)),
        `superseded typeahead fetched an unnecessary shard: ${fetchedTypeaheadShards.join(', ')}`,
    );

    const plannedBeforeCleanup = plannedQueries.length;
    const renderedBeforeCleanup = rendered.length;
    fakeInput.value = 'podcast';
    fakeInput.dispatchEvent(new Event('input'));
    detachTypeahead();
    await sleep(90);
    fakeInput.value = 'firefox';
    fakeInput.dispatchEvent(new Event('input'));
    await sleep(70);
    assert.equal(plannedQueries.length, plannedBeforeCleanup, 'cleanup must cancel pending planning');
    assert.equal(rendered.length, renderedBeforeCleanup, 'cleanup must prevent later rendering');

    const retryFetch = createFileFetch();
    const retryEngine = await initTinysearch(optionsFor(artifacts, retryFetch));
    const retryPlan = retryEngine.plan(lazyQuery);
    assert.ok(retryPlan.required.length > 0, 'retry query must require a shard');
    const failedShard = retryPlan.required[0];
    retryFetch.failOnce(failedShard.url);
    await assert.rejects(
        retryEngine.search(lazyQuery, 10),
        (error) => {
            assert.match(error.message, /503 Injected Failure/);
            assert.match(error.message, new RegExp(failedShard.filename.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
            return true;
        },
        'first injected shard fetch should fail',
    );
    const retried = await retryEngine.search(lazyQuery, 10);
    assert.ok(retried.length > 0, 'search should succeed after retrying a failed shard fetch');
    assert.equal(countFor(retryFetch, failedShard.url), 2, 'failed shard promise must be evicted for retry');

    for (let iteration = 0; iteration < 20; iteration += 1) {
        await engine.search(lazyQuery, 10);
    }
    const memoryBefore = engine.stats.wasmMemoryBytes;
    const searchIterations = 250;
    const timingStart = performance.now();
    for (let iteration = 0; iteration < searchIterations; iteration += 1) {
        await engine.search(lazyQuery, 10);
    }
    const warmedSearchTotalMs = performance.now() - timingStart;
    const memoryAfter = engine.stats.wasmMemoryBytes;
    const memoryGrowth = memoryAfter - memoryBefore;
    assert.ok(
        memoryGrowth <= 8 * PAGE_BYTES,
        `WASM memory grew by ${memoryGrowth} bytes across ${searchIterations} repeated searches`,
    );
    assert.ok(
        memoryGrowth < searchIterations * PAGE_BYTES,
        'WASM memory appears to grow by a page per query (result handles may be leaking)',
    );

    const diagnostics = {
        outputDirectory,
        artifacts: {
            wasm: path.basename(artifacts.wasm),
            loader: path.basename(artifacts.loader),
            root: path.basename(artifacts.root),
            shardCount: artifacts.shards.length,
        },
        lazyQuery,
        lazyShardCount: lazyPlan.required.length,
        finalStats: engine.stats,
        repeatedSearchMemoryGrowth: memoryGrowth,
        warmedSearchTiming: {
            iterations: searchIterations,
            totalMs: Number(warmedSearchTotalMs.toFixed(3)),
            averageMs: Number((warmedSearchTotalMs / searchIterations).toFixed(4)),
        },
    };
    context.diagnostic(JSON.stringify(diagnostics, null, 2));
});
