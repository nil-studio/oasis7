ARG UBUNTU_BASE
ARG NODE_BASE=node:24-bookworm-slim
ARG RUST_BASE=rust:1.96.0-bookworm
FROM ${RUST_BASE} AS rust-tools
FROM ${NODE_BASE} AS node-tools

FROM ${UBUNTU_BASE} AS runtime
COPY runtime-packages.txt /tmp/ci-packages.txt
RUN apt-get update \
    && xargs apt-get install -y --no-install-recommends </tmp/ci-packages.txt \
    && rm -rf /var/lib/apt/lists/* /tmp/ci-packages.txt

FROM runtime AS build
COPY build-packages.txt /tmp/ci-packages.txt
RUN apt-get update \
    && xargs apt-get install -y --no-install-recommends </tmp/ci-packages.txt \
    && rm -rf /var/lib/apt/lists/* /tmp/ci-packages.txt

FROM runtime AS browser
COPY --from=node-tools /usr/local/bin/node /usr/local/bin/node
COPY --from=node-tools /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
COPY --from=browser-tools package.json package-lock.json /opt/oasis7-browser/
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/oasis7-playwright
ENV OASIS7_PLAYWRIGHT_MODULE=/opt/oasis7-browser/node_modules/playwright
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && npm ci --prefix /opt/oasis7-browser --ignore-scripts \
    && node "$OASIS7_PLAYWRIGHT_MODULE/cli.js" install chromium --with-deps --only-shell \
    && rm -rf /var/lib/apt/lists/* /root/.npm

# Build on Ubuntu so Git and its transport helpers use the consumer ABI.
FROM build AS git-tools
RUN apt-get update \
    && apt-get install -y --no-install-recommends libcurl4-openssl-dev libexpat1-dev xz-utils zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /tmp/git-source
RUN curl -fsSL --retry 3 https://www.kernel.org/pub/software/scm/git/git-2.56.1.tar.xz -o git.tar.xz \
    && echo 'f0b0bf89264b9bd2e1c640363e7529643989da76281600473c50745cd34a607f  git.tar.xz' | sha256sum -c - \
    && tar -xJf git.tar.xz --strip-components=1 \
    && make -j"$(nproc)" prefix=/usr/local NO_RUST=YesPlease NO_TCLTK=YesPlease NO_GETTEXT=YesPlease NO_PERL=YesPlease all \
    && make prefix=/usr/local NO_RUST=YesPlease NO_TCLTK=YesPlease NO_GETTEXT=YesPlease NO_PERL=YesPlease DESTDIR=/tmp/git-install install

FROM build AS rust-build
COPY --from=git-tools /tmp/git-install/usr/local/ /usr/local/
ARG RUST_TOOLCHAIN
ARG WASM_TOOLCHAIN
ARG WASM_TARGET
COPY --from=rust-tools /usr/local/cargo/bin/ /opt/oasis7-rust/bin/
COPY --from=rust-tools /usr/local/rustup/ /opt/oasis7-rust/rustup/
ENV RUSTUP_HOME=/opt/oasis7-rust/rustup
ENV PATH=/opt/oasis7-rust/bin:$PATH
ENV RUST_TOOLCHAIN=${RUST_TOOLCHAIN}
ENV OASIS7_WASM_TOOLCHAIN=${WASM_TOOLCHAIN}
ENV OASIS7_WASM_TARGET=${WASM_TARGET}
RUN rustup set auto-self-update disable \
    && rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal --component rustfmt --component clippy \
    && rustup default "$RUST_TOOLCHAIN" \
    && rustup toolchain install "$OASIS7_WASM_TOOLCHAIN" --profile minimal --component rust-src \
    && rustup target add "$OASIS7_WASM_TARGET" --toolchain "$OASIS7_WASM_TOOLCHAIN" \
    && rustup target add wasm32-unknown-unknown --toolchain "$RUST_TOOLCHAIN"

FROM rust-build AS rust-wasm-build
RUN apt-get update \
    && apt-get install -y --no-install-recommends clang llvm \
    && rm -rf /var/lib/apt/lists/*
