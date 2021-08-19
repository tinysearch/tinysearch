# Docker tinysearch with deps
#   - binaryen
#   - wasm-pack
#   - terser

ARG TINY_REPO=https://github.com/tinysearch/tinysearch
ARG TINY_BRANCH=master
ARG RUST_IMAGE=rust:alpine

FROM $RUST_IMAGE AS binary-build

ARG TINY_REPO
ARG TINY_BRANCH

WORKDIR /tmp

RUN apk add --update --no-cache --virtual \
    build-dependencies \
    musl-dev \
    openssl-dev \
    gcc \
    curl \
    git \
    npm \
    gcc \
    ca-certificates \
    libc6-compat \
    binaryen

RUN set -eux -o pipefail; \
    ln -s /lib64/ld-linux-x86-64.so.2 /lib/ld64.so.1; \
    npm install terser -g;

RUN terser --version

RUN cd /tmp && git clone --branch "$TINY_BRANCH" "$TINY_REPO"
RUN set -ex -o pipefail; \
    cd /tmp/tinysearch \
    && cargo build --release \
    && cp target/release/tinysearch $CARGO_HOME/bin

RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

RUN wasm-pack --version

FROM $RUST_IMAGE

WORKDIR /tmp

RUN apk add --update --no-cache libc6-compat musl-dev binaryen

RUN set -eux -o pipefail; \
    ln -s /lib64/ld-linux-x86-64.so.2 /lib/ld64.so.1;

COPY --from=binary-build /usr/local/bin/ /usr/local/bin/
COPY --from=binary-build /usr/local/cargo/bin/ /usr/local/bin/

# crate cache init. No need to download crate for future usage
RUN set -eux -o pipefail; \
    echo '[{"title":"","body":"","url":""}]' > build.json; \
    tinysearch build.json; \
    rm -rf /tmp/*

ENTRYPOINT ["tinysearch"]
