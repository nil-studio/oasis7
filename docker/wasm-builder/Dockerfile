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
        python3 \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

COPY --chmod=0755 .github/actions/setup-linux-mold/setup.py /opt/oasis7/.github/actions/setup-linux-mold/setup.py
RUN python3 /opt/oasis7/.github/actions/setup-linux-mold/setup.py
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/opt/oasis7-mold/3.0.0-v1/bin/oasis7-mold-gcc

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none \
    && chmod -R a+rX /cargo /rustup

RUN rustup toolchain install "${WASM_TOOLCHAIN}" --profile minimal --component rust-src \
    && rustup target add "${WASM_TARGET}" --toolchain "${WASM_TOOLCHAIN}" \
    && chmod -R a+rX /rustup

WORKDIR /opt/oasis7
COPY --chmod=0755 docker/wasm-builder/Cargo.toml docker/wasm-builder/Cargo.lock /opt/oasis7/docker/wasm-builder/
COPY --chmod=0755 docker/wasm-builder/src /opt/oasis7/docker/wasm-builder/src
COPY --chmod=0755 crates/oasis7_wasm_build /opt/oasis7/crates/oasis7_wasm_build
COPY --chmod=0755 tools/wasm_build_suite /opt/oasis7/tools/wasm_build_suite
COPY --chmod=0755 scripts/build-wasm-module.sh /opt/oasis7/scripts/build-wasm-module.sh

RUN env -u RUSTC_WRAPPER CARGO_TARGET_DIR=/tmp/oasis7-producer-target cargo install --locked --path /opt/oasis7/docker/wasm-builder \
    && readelf -p .comment /cargo/bin/wasm_build_suite | grep -E 'mold 3[.]0[.]0([[:space:]]|$)' \
    && chmod a+rx /cargo/bin/wasm_build_suite \
    && rm -rf /tmp/oasis7-producer-target /cargo/registry /cargo/git

LABEL org.opencontainers.image.title="oasis7 wasm builder"
LABEL org.opencontainers.image.description="Canonical linux-x86_64 WASM builder for oasis7"
LABEL org.opencontainers.image.source="https://github.com/eng-cc/oasis7"
LABEL org.opencontainers.image.vendor="oasis7"

WORKDIR /workspace
ENTRYPOINT ["/opt/oasis7/scripts/build-wasm-module.sh"]
