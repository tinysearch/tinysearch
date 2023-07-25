# Docker tinysearch with deps
#   - binaryen
#   - wasm-pack
#   - terser

ARG TINY_REPO=https://github.com/tinysearch/tinysearch
ARG TINY_BRANCH=master
ARG RUST_IMAGE=rust:alpine

FROM $RUST_IMAGE AS builder

ARG TINY_REPO
ARG TINY_BRANCH

WORKDIR /build

# Install dependencies
RUN apk add --update --no-cache --virtual \
    .build-deps \
    musl-dev \
    openssl-dev \
    gcc \
    curl \
    git \
    npm \
    gcc \
    ca-certificates \
    libc6-compat \
    binaryen && \
    ln -s /lib64/ld-linux-x86-64.so.2 /lib/ld64.so.1 && \
    npm install terser -g

# Verify the installation
RUN terser --version

# Install wasm-pack
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Verify the installation
RUN wasm-pack --version && which wasm-pack 

# Clone the repo and build the binary
RUN git clone --branch "$TINY_BRANCH" "$TINY_REPO" tinysearch && \
    cd tinysearch && \
    cargo build --release --features=bin && \
    cp target/release/tinysearch $CARGO_HOME/bin

FROM $RUST_IMAGE

WORKDIR /app

# Install runtime dependencies
RUN apk add --update --no-cache libc6-compat musl-dev binaryen openssl-dev && \
    ln -s /lib64/ld-linux-x86-64.so.2 /lib/ld64.so.1

# Copy the build binaries and tinysearch directory
COPY --from=builder /usr/local/bin/ /usr/local/bin/
COPY --from=builder /usr/local/cargo/bin/ /usr/local/bin/
# Copy tinysearch build directory to be used as the engine (see `--engine-version` option below)
# This is done because we want to use the same image for building and running tinysearch
# and not depend on crates.io for the engine
COPY --from=builder /build/tinysearch/ /app/engine

# Initialize crate cache
RUN echo '[{"title":"","body":"","url":""}]' > build.json && \
    tinysearch --engine-version 'path= "/app/engine"' build.json

ENTRYPOINT ["tinysearch"]
