# Needed SHELL since I'm using zsh
SHELL := /bin/bash

.PHONY: help
help: ## This help message
	@echo -e "$$(grep -hE '^\S+:.*##' $(MAKEFILE_LIST) | sed -e 's/:.*##\s*/:/' -e 's/^\(.\+\):\(.*\)/\\x1b[36m\1\\x1b[m:\2/' | column -c2 -t -s :)"

.PHONY: lint
lint: ### Lint project using clippy
	cargo clippy --all-targets --all-features -- -D warnings

.PHONY: clean
clean: ### Clean up build artifacts
	cargo clean
	rm -rf wasm_output 

.PHONY: build
build: ### Compile project
	cargo build --features=bin

.PHONY: build-docker
build-docker: ### Build Docker image
	docker build -t tinysearch/cli .

.PHONY: install
install: ## Install tinysearch
	cargo install --force --path tinysearch --features=bin

.PHONY: test
test: ## Run unit tests
	cargo test --features=bin

.PHONY: run
run: ## Run tinysearch with sample input
	cargo run --features="bin" -- fixtures/index.json
