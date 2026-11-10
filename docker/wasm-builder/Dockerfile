FROM debian:bookworm-slim

ARG WASM_TOOLCHAIN=nightly-2025-12-11
ARG WASM_TARGET=wasm32-unknown-unknown

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_HOME=/cargo
ENV RUSTUP_HOME=/rustup
ENV PATH=/cargo/bin:${PATH}
ENV OASIS7_WASM_TOOLCHAIN=${WASM_TOOLCHAIN}
ENV OASIS7_WASM_TARGET=${WASM_TARGET}
ENV OASIS7_WASM_BUILD_STD=1
ENV OASIS7_WASM_BUILD_STD_COMPONENTS=std,panic_abort
ENV OASIS7_WASM_BUILD_STD_FEATURES=
ENV OASIS7_WASM_BUILD_IN_CONTAINER=1
ENV OASIS7_WASM_BUILD_SUITE_BIN=/cargo/bin/wasm_build_suite
ENV TZ=UTC
ENV LANG=C
ENV LC_ALL=C
ENV SOURCE_DATE_EPOCH=0

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        build-essential \
        ca-certificates \
        curl \
        git \
        pkg-config \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none \
    && chmod -R a+rX /cargo /rustup

RUN rustup toolchain install "${WASM_TOOLCHAIN}" --profile minimal --component rust-src \
    && rustup target add "${WASM_TARGET}" --toolchain "${WASM_TOOLCHAIN}" \
    && chmod -R a+rX /rustup

WORKDIR /opt/oasis7
COPY --chmod=0755 Cargo.toml Cargo.lock /opt/oasis7/
COPY --chmod=0755 crates /opt/oasis7/crates
COPY --chmod=0755 vendor-if-watch-3.2.2 /opt/oasis7/vendor-if-watch-3.2.2
COPY --chmod=0755 vendor-libp2p-swarm-0.48.0 /opt/oasis7/vendor-libp2p-swarm-0.48.0
COPY --chmod=0755 vendor-libp2p-rendezvous-0.18.1 /opt/oasis7/vendor-libp2p-rendezvous-0.18.1
COPY --chmod=0755 tools/scenario_test_runner /opt/oasis7/tools/scenario_test_runner
COPY --chmod=0755 tools/wasm_build_suite /opt/oasis7/tools/wasm_build_suite
COPY --chmod=0755 tools/wasm_module_observe /opt/oasis7/tools/wasm_module_observe
COPY --chmod=0755 scripts/build-wasm-module.sh /opt/oasis7/scripts/build-wasm-module.sh
COPY --chmod=0755 docker/wasm-builder/prepare-workspace.sh /opt/oasis7/docker/wasm-builder/prepare-workspace.sh

RUN bash /opt/oasis7/docker/wasm-builder/prepare-workspace.sh \
    && env -u RUSTC_WRAPPER CARGO_TARGET_DIR=/tmp/oasis7-producer-target cargo install --locked --path /opt/oasis7/tools/wasm_build_suite \
    && chmod a+rx /cargo/bin/wasm_build_suite \
    && rm -rf /tmp/oasis7-producer-target /cargo/registry /cargo/git

LABEL org.opencontainers.image.title="oasis7 wasm builder"
LABEL org.opencontainers.image.description="Canonical linux-x86_64 WASM builder for oasis7"
LABEL org.opencontainers.image.source="https://github.com/eng-cc/oasis7"
LABEL org.opencontainers.image.vendor="oasis7"

WORKDIR /workspace
ENTRYPOINT ["/opt/oasis7/scripts/build-wasm-module.sh"]
