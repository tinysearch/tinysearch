.PHONY: index
index:
	cargo build -vv

.PHONY: pack
pack:
	wasm-pack build --target web --release

.PHONY: top
top:
	wasm-pack build --target web
	twiggy top -n 20 pkg/tinysearch_bg.wasm

.PHONY: opt
opt:
	wasm-opt -Oz -o pkg/tinysearch_bg_opt.wasm pkg/tinysearch_bg.wasm
	ls -l pkg/tinysearch_bg_opt.wasm

.PHONY: build
build: index pack opt

.PHONY: run
run:
	open index.html
	python3 -m http.server
