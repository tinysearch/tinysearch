import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';

const outputArgument = process.argv[2];
if (!outputArgument) {
    throw new Error('Usage: node tests/legacy_wasm_loader_test.mjs <generated-output-directory>');
}

const outputDirectory = path.resolve(outputArgument);

async function findArtifact(extension) {
    const entries = await readdir(outputDirectory, { withFileTypes: true });
    const matches = entries
        .filter((entry) => entry.isFile() && entry.name.endsWith(extension))
        .map((entry) => path.join(outputDirectory, entry.name));
    assert.equal(matches.length, 1, `expected one ${extension} artifact, found ${matches.join(', ')}`);
    return matches[0];
}

function createFileFetch() {
    const counts = new Map();
    return {
        counts,
        async fetch(input) {
            const url = input instanceof URL ? input : new URL(String(input));
            counts.set(url.href, (counts.get(url.href) ?? 0) + 1);
            assert.equal(url.protocol, 'file:');
            const bytes = await readFile(fileURLToPath(url));
            return new Response(bytes, {
                status: 200,
                headers: { 'content-type': 'application/octet-stream' },
            });
        },
    };
}

let loaderModulePromise;
function loadGeneratedLoader() {
    loaderModulePromise ??= (async () => {
        const loader = await findArtifact('.js');
        const loaderUrl = pathToFileURL(loader);
        loaderUrl.searchParams.set('legacy-loader-test', String(Date.now()));
        return import(loaderUrl.href);
    })();
    return loaderModulePromise;
}

function createStubEngine(TinySearch, searchFn) {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const engine = new TinySearch({
        exports: {
            memory,
            search: searchFn,
            free_search_result() {},
            alloc_query() {
                return 1024;
            },
            free_query() {},
        },
    });
    return { engine, memory };
}

test('generated legacy WASM loader owns query memory and keeps an async API', async () => {
    const wasm = await findArtifact('.wasm');
    const { initTinysearch, init_tinysearch } = await loadGeneratedLoader();
    assert.equal(init_tinysearch, initTinysearch);

    const controller = createFileFetch();
    const wasmUrl = pathToFileURL(wasm);
    const engine = await initTinysearch({ wasmUrl, fetch: controller.fetch });
    assert.equal(controller.counts.get(wasmUrl.href), 1, 'streaming fallback must reuse the response');

    const results = await engine.search('decades', 5);
    assert.ok(results.length > 0, 'known fixture query returned no Xor8 results');
    assert.deepEqual(Object.keys(results[0]).sort(), ['meta', 'title', 'url']);

    for (let iteration = 0; iteration < 20; iteration += 1) {
        await engine.search('decades', 5);
    }
    const memoryBefore = engine.memory.buffer.byteLength;
    for (let iteration = 0; iteration < 250; iteration += 1) {
        await engine.search('decades', 5);
    }
    const memoryGrowth = engine.memory.buffer.byteLength - memoryBefore;
    assert.ok(memoryGrowth <= 64 * 1024, `legacy WASM memory grew by ${memoryGrowth} bytes`);
});

test('legacy loader rejects corrupt ABI results', async () => {
    const { TinySearch } = await loadGeneratedLoader();
    const { engine: nullResultEngine } = createStubEngine(TinySearch, () => 0);
    await assert.rejects(
        nullResultEngine.search('query'),
        /WASM search returned a null result pointer/,
    );

    const { engine: invalidJsonEngine, memory } = createStubEngine(TinySearch, () => 16);
    new Uint8Array(memory.buffer).set(new TextEncoder().encode('not JSON\0'), 16);
    await assert.rejects(invalidJsonEngine.search('query'), /WASM search returned invalid JSON/);
});
