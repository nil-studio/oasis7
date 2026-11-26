ARG UBUNTU_BASE
ARG NODE_BASE=node:24-bookworm-slim
ARG CUSTODY_NODE_BASE=node:22-bookworm-slim
ARG RUST_BASE=rust:1.96.0-bookworm
FROM ${RUST_BASE} AS rust-tools
FROM ${NODE_BASE} AS node-tools

FROM ${UBUNTU_BASE} AS runtime
COPY --from=node-tools /usr/local/bin/node /usr/local/bin/node
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
RUN apt-get update \
    && apt-get install -y --no-install-recommends ruby \
    && rm -rf /var/lib/apt/lists/*
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

FROM build AS codeql-rust
COPY codeql-rust-packages.txt /tmp/ci-packages.txt
RUN apt-get update \
    && xargs apt-get install -y --no-install-recommends </tmp/ci-packages.txt \
    && rm -rf /var/lib/apt/lists/* /tmp/ci-packages.txt
ARG CODEQL_RUST_TOOLCHAIN
COPY --from=rust-tools /usr/local/cargo/bin/ /opt/oasis7-rust/bin/
COPY --from=rust-tools /usr/local/rustup/ /opt/oasis7-rust/rustup/
ENV RUSTUP_HOME=/opt/oasis7-rust/rustup
ENV PATH=/opt/oasis7-rust/bin:$PATH
ENV RUSTUP_TOOLCHAIN=${CODEQL_RUST_TOOLCHAIN}
ENV RUSTUP_AUTO_INSTALL=0

FROM runtime AS api-runtime
RUN curl --fail --location --silent --show-error --retry 3 --connect-timeout 15 --max-time 120 \
      --proto '=https' --proto-redir '=https' \
      https://github.com/cli/cli/releases/download/v2.105.0/gh_2.105.0_linux_amd64.tar.gz \
      --output /tmp/gh.tar.gz \
    && echo '79e7ab5db738c48777b696e374f8b428f0647debfb424f44d2f426370407fe2f  /tmp/gh.tar.gz' | sha256sum --check \
    && tar -xOzf /tmp/gh.tar.gz gh_2.105.0_linux_amd64/bin/gh >/tmp/gh \
    && install -m 0755 /tmp/gh /usr/local/bin/gh \
    && rm -f /tmp/gh /tmp/gh.tar.gz

# Official Linux/amd64 CLI image containing Docker 28.5.2 and Buildx 0.29.1.
FROM docker:28.5.2-cli@sha256:cd58b396e427d1ee8cddcc3f3b7e8d8c2ba45755c5dd5b821b6e615e1ccf4586 AS docker-cli-tools

FROM rust-build AS rust-ordinary-builder
COPY --from=docker-cli-tools --chown=0:0 --chmod=0755 /usr/local/bin/docker /usr/local/bin/docker
COPY --from=docker-cli-tools --chown=0:0 --chmod=0755 /usr/local/libexec/docker/cli-plugins/docker-buildx /usr/local/libexec/docker/cli-plugins/docker-buildx

FROM ${CUSTODY_NODE_BASE} AS custody-node-tools

FROM rust-build AS rust-custody-wasm
COPY --from=custody-node-tools /usr/local/bin/node /opt/oasis7-node22/bin/node
COPY --from=custody-node-tools /usr/local/lib/node_modules/npm /opt/oasis7-node22/lib/node_modules/npm
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /opt/oasis7-node22/bin/npm \
    && ln -s ../lib/node_modules/npm/bin/npx-cli.js /opt/oasis7-node22/bin/npx \
    && install -d -m 0755 /.cache \
    && install -d -m 0700 -o 1001 -g 1001 /.cache/oasis7

FROM rust-build AS rust-viewer
COPY --from=node-tools /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
COPY --from=viewer-browser . /opt/oasis7-viewer-browser/
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx \
    && install -d -m 0755 /.cache \
    && install -d -m 0700 -o 1001 -g 1001 /.cache/oasis7
RUN bash -euo pipefail -c '\
    tools=/opt/oasis7-viewer-browser/scripts; \
    python3 "$tools/ci-browser-deps.py" tools; \
    chmod 0755 "$tools/ci-browser-tools/node_modules/@agent-browser/cli-linux-x64-gnu/bin/agent-browser"; \
    python3 "$tools/ci-browser-deps.py" prepare --browser agent --work-dir /tmp/oasis7-viewer-browser --cache-base /tmp/oasis7-viewer-archives; \
    mapfile -t packages </tmp/oasis7-viewer-browser/apt-arguments.txt; \
    if (( ${#packages[@]} )); then \
        python3 "$tools/ci-apt-install.py" --budget-seconds 300 --no-remove "${packages[@]}"; \
    fi; \
    mv /tmp/oasis7-viewer-browser/chrome /opt/oasis7-viewer-browser/chrome; \
    rm -rf /tmp/oasis7-viewer-browser /tmp/oasis7-viewer-archives /var/lib/apt/lists/* /root/.npm'
