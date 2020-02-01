# Needed SHELL since I'm using zsh
SHELL := /bin/bash

.PHONY: help
help: ## This help message
	@echo -e "$$(grep -hE '^\S+:.*##' $(MAKEFILE_LIST) | sed -e 's/:.*##\s*/:/' -e 's/^\(.\+\):\(.*\)/\\x1b[36m\1\\x1b[m:\2/' | column -c2 -t -s :)"

.PHONY: clean
clean: ### Clean up project artifacts
	cargo clean

.PHONY: build
build: ### Build project
	cargo build

.PHONY: install
install: ## Install tinysearch
	cargo install --force --path bin 

.PHONY: test
test: ## Run unittests
	cargo test