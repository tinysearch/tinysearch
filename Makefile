# Needed SHELL since I'm using zsh
SHELL := /bin/bash

.PHONY: help
help: ## This help message
	@echo -e "$$(grep -hE '^\S+:.*##' $(MAKEFILE_LIST) | sed -e 's/:.*##\s*/:/' -e 's/^\(.\+\):\(.*\)/\\x1b[36m\1\\x1b[m:\2/' | column -c2 -t -s :)"

.PHONY: lint
lint: ### Lint project using clippy
	cargo clippy

.PHONY: clean
clean: ### Clean up build artifacts
	cargo clean

.PHONY: build
build: ### Compile project
	cargo build

.PHONY: install
install: ## Install tinysearch
	cargo install --force --path tinysearch --features=bin

.PHONY: test
test: ## Run unit tests
	cargo test --features=bin

.PHONY: run
run: ## Run tinysearch with sample input
	cargo run -- fixtures/index.json

.PHONY: pack
pack: ## Pack tinysearch node module
	wasm-pack build tinysearch
	wasm-pack pack

.PHONY: publish
publish: pack ## Publish tinysearch to NPM
	wasm-pack publish