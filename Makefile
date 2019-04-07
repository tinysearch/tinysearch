.PHONY: index
index:
	cargo build -vv

.PHONY: pack
pack:
	wasm-pack build --target web --release

.PHONY: profile
profile:
	wasm-pack build --target web
	twiggy top -n 20 pkg/tinysearch_bg.wasm

.PHONY: opt
opt:
	wasm-opt -Oz -o pkg/tinysearch_bg_opt.wasm pkg/tinysearch_bg.wasm
	ls -l pkg/tinysearch_bg_opt.wasm

.PHONY: build
build: index pack opt

.PHONY: run open
run open:
	open index.html
	python3 -m http.server
