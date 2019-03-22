.PHONY: index
index:
	INPUT_DIR=$(INPUT_DIR) cargo build	

.PHONY: pack
pack:
	wasm-pack build --target web

.PHONY: build
build: index pack

.PHONY: run
run: build
	python3 -m http.server	