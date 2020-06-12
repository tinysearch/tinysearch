# Docker tinsearch with deps
#   - binaryen
#   - wasm-pack
#   - terser
FROM rust:1.44-alpine as binary-build

WORKDIR /tmp

RUN apk add --update --no-cache --virtual build-dependencies musl-dev openssl-dev gcc curl git npm gcc ca-certificates libc6-compat

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=1.44.0 \
    BINARYEN="https://github.com/WebAssembly/binaryen/releases/download/version_93/binaryen-version_93-x86_64-linux.tar.gz"

RUN rustup install $RUST_VERSION
RUN rustup override set $RUST_VERSION
RUN rustup target add asmjs-unknown-emscripten --toolchain $RUST_VERSION
RUN rustup target add wasm32-unknown-emscripten --toolchain $RUST_VERSION

RUN set -eux; \
    ln -s /lib64/ld-linux-x86-64.so.2 /lib/ld64.so.1; \
    npm install terser -g; \
    curl -sL $BINARYEN |tar zxpvf -; \
    cp -rp binaryen*/* /usr/local/bin/.

RUN time cargo install wasm-pack
RUN time cargo install tinysearch

RUN wasm-pack --version
RUN tinysearch --version

RUN rm -rf /tmp/*

CMD ["/usr/local/cargo/bin/tinysearch"]
