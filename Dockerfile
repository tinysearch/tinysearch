ARG RUST_IMAGE=rust:1.85-alpine

FROM ${RUST_IMAGE} AS builder

RUN apk add --no-cache musl-dev \
    && rustup target add wasm32-unknown-unknown

WORKDIR /build/tinysearch

COPY Cargo.toml Cargo.lock README.md ./
COPY assets ./assets
COPY src ./src

RUN cargo build --locked --release --features=bin

FROM ${RUST_IMAGE}

RUN apk add --no-cache binaryen musl-dev \
    && rustup target add wasm32-unknown-unknown

WORKDIR /app

COPY --from=builder /build/tinysearch/target/release/tinysearch /usr/local/bin/tinysearch
COPY Cargo.toml Cargo.lock README.md /engine/
COPY assets /engine/assets
COPY src /engine/src

# Warm the Cargo cache used when tinysearch generates a WASM search engine.
RUN printf '[{"title":"","body":"","url":""}]' > build.json \
    && tinysearch --engine-version 'path="/engine"' build.json \
    && rm -rf build.json wasm_output

ENTRYPOINT ["tinysearch", "--engine-version", "path=\"/engine\""]
