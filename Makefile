.PHONY: build
build:
	wasm-pack build --target web

.PHONY: run
run: build
	python3 -m http.server	