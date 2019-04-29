.PHONY: build
build:
	cargo build

.PHONY: install
install:
	cargo install --force --path bin 