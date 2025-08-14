# Needed SHELL since I'm using zsh
SHELL := /bin/bash

# Default target
.DEFAULT_GOAL := help

# PHONY targets
.PHONY: help clean build build-release build-docker docker install test test-unit test-integration
.PHONY: lint fmt check audit run example demo deps update
.PHONY: ci-check ci-test ci-build ci-lint ci-fmt ci-audit

help: ## Display this help message
	@echo "Available targets:"
	@echo
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-20s %s\n", $$1, $$2}'
	@echo

clean: ## Clean up build artifacts
	cargo clean
	rm -rf wasm_output target/criterion demo docker_output
	rm -rf examples/*/dist
	find . -name "*.wasm" -type f -delete
	find . -name "*.js" -type f -path "*/pkg/*" -delete

build: ## Build the project in debug mode
	cargo build --features=bin

build-release: ## Build the project in release mode
	cargo build --release --features=bin

build-docker: ## Build Docker image
	docker build --progress=plain -t tinysearch/cli .

docker: ## Build and run Docker container with sample data
	@echo "🐳 Building Docker image..."
	@docker build -t tinysearch/cli .
	@echo "🚀 Running Docker container with sample data..."
	@mkdir -p docker_output
	@docker run --rm -v $(PWD)/docker_output:/app/output -v $(PWD)/fixtures:/app/fixtures tinysearch/cli -m wasm -p /app/output /app/fixtures/index.json
	@echo "📂 Output files created in docker_output/"
	@ls -la docker_output/

install: ## Install tinysearch locally
	cargo install --force --path . --features=bin

test: test-unit test-integration ## Run all tests

test-unit: ## Run unit tests only
	cargo test --lib --features=bin

test-integration: ## Run integration tests only
	cargo test --test integration_test

test-wasm: check-wasm-target ## Run WASM integration tests only
	cargo test --test integration_test test_cli_wasm_mode

lint: ## Run clippy linter
	cargo clippy --all-targets --all-features -- -D warnings

fmt: ## Format code with rustfmt
	cargo fmt --all
	
fmt-check: ## Check if code is formatted
	cargo fmt --all -- --check

check: ## Run cargo check
	cargo check --all-targets --all-features

audit: ## Run security audit
	cargo audit

run: ## Run tinysearch with sample input
	cargo run --features=bin -- fixtures/index.json

release: ## Run tinysearch release build
	cargo run --features=bin -- --release fixtures/index.json

example: check-wasm-target ## Generate WASM output with sample data
	mkdir -p wasm_output
	cargo run --features=bin -- -m wasm -p wasm_output fixtures/index.json

demo: check-wasm-target ## Run interactive demo (generates WASM and starts server)
	@echo "🚀 Building TinySearch and generating WASM demo..."
	@mkdir -p demo
	@cargo run --features=bin -- -m wasm -p demo fixtures/index.json
	@mv demo/demo.html demo/index.html
	@echo "🌐 Starting demo server at http://localhost:8000/demo/"
	@echo "   Press Ctrl+C to stop the server"
	@echo ""
	@python3 -m http.server 8000

deps: ## Show dependency tree
	cargo tree

update: ## Update dependencies
	cargo update

# CI targets
ci-check: check lint fmt-check ## Run all CI checks
ci-test: test ## Run tests for CI
ci-build: build build-release ## Build for CI
ci-lint: lint ## Run linting for CI
ci-fmt: fmt-check ## Check formatting for CI
ci-audit: audit ## Run audit for CI

# Development targets
dev-setup: ## Set up development environment
	rustup component add clippy rustfmt
	cargo install cargo-audit cargo-machete
	@echo "Installing WASM target for builds..."
	rustup target add wasm32-unknown-unknown

check-wasm-target: ## Check if wasm32-unknown-unknown target is installed
	@rustup target list --installed | grep -q "wasm32-unknown-unknown" || (echo "wasm32-unknown-unknown target not found. Run 'make dev-setup' or 'rustup target add wasm32-unknown-unknown'" && exit 1)

dev-watch: ## Watch for changes and run tests
	cargo watch -x 'test --features=bin'

dev-clean-deps: ## Remove unused dependencies
	cargo machete

# Performance targets
bench: ## Run benchmarks (if any)
	cargo bench

profile: ## Profile the application
	cargo build --release --features=bin
	perf record ./target/release/tinysearch fixtures/index.json
	perf report
