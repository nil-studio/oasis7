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

FROM build AS rust-build
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
RUN rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal --component rustfmt --component clippy \
    && rustup default "$RUST_TOOLCHAIN" \
    && rustup toolchain install "$OASIS7_WASM_TOOLCHAIN" --profile minimal --component rust-src \
    && rustup target add "$OASIS7_WASM_TARGET" --toolchain "$OASIS7_WASM_TOOLCHAIN" \
    && rustup target add wasm32-unknown-unknown --toolchain "$RUST_TOOLCHAIN"
