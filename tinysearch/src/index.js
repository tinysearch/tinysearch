import * as wasm from './index_bg.wasm';

const rust = import('./wasm');

console.log(rust)
rust.wasm.generate("fixtures/index.json", ".", true);
